//! Cross-platform transport for the daemon protocol.
//!
//! Unix systems use a filesystem socket inside the per-user runtime
//! directory; Windows uses a per-user named pipe instead.

use crate::{LocalWebError, Paths, Result};
use std::path::{Path, PathBuf};

#[cfg(unix)]
#[derive(Debug)]
struct EndpointLock(std::fs::File);

#[cfg(unix)]
impl Drop for EndpointLock {
    fn drop(&mut self) {
        // Explicit unlock also releases ownership if a concurrently spawned
        // child briefly inherited a duplicate descriptor before exec.
        let _ = self.0.unlock();
    }
}

/// Client end of a daemon connection.
#[cfg(unix)]
pub type ClientStream = tokio::net::UnixStream;
/// Client end of a daemon connection.
#[cfg(windows)]
pub type ClientStream = tokio::net::windows::named_pipe::NamedPipeClient;

/// Server end of an accepted daemon connection.
#[cfg(unix)]
pub type ServerStream = tokio::net::UnixStream;
/// Server end of an accepted daemon connection.
#[cfg(windows)]
pub type ServerStream = tokio::net::windows::named_pipe::NamedPipeServer;

/// Listening side of the daemon protocol endpoint.
#[derive(Debug)]
pub struct DaemonListener {
    path: PathBuf,
    #[cfg(unix)]
    listener: tokio::net::UnixListener,
    #[cfg(unix)]
    _lock: EndpointLock,
    #[cfg(windows)]
    pending: Option<tokio::net::windows::named_pipe::NamedPipeServer>,
}

impl DaemonListener {
    /// Binds the endpoint, replacing a stale socket file on Unix.
    ///
    /// # Errors
    ///
    /// Returns an error when another daemon already answers or the endpoint
    /// cannot be created.
    pub fn bind(paths: &Paths) -> impl std::future::Future<Output = Result<Self>> + '_ {
        #[cfg(unix)]
        {
            Self::bind_unix(paths)
        }
        #[cfg(windows)]
        {
            std::future::ready(Self::bind_windows(paths))
        }
    }

    #[cfg(unix)]
    async fn bind_unix(paths: &Paths) -> Result<Self> {
        let path = paths.socket();
        #[cfg(unix)]
        {
            use std::os::unix::fs::{FileTypeExt as _, OpenOptionsExt as _, PermissionsExt as _};
            let lock = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(false)
                .mode(0o600)
                .open(paths.runtime.join("daemon.lock"))?;
            lock.try_lock().map_err(|error| {
                LocalWebError::Service(format!(
                    "cannot lock the daemon endpoint; another daemon may be running: {error}"
                ))
            })?;
            let lock = EndpointLock(lock);
            if let Ok(metadata) = std::fs::symlink_metadata(&path) {
                if !metadata.file_type().is_socket() {
                    return Err(LocalWebError::Config(format!(
                        "daemon endpoint is not a socket: {}",
                        path.display()
                    )));
                }
                match tokio::net::UnixStream::connect(&path).await {
                    Ok(_) => {
                        return Err(LocalWebError::Service(format!(
                            "localwebd is already running at {}",
                            path.display()
                        )));
                    }
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound
                        ) =>
                    {
                        std::fs::remove_file(&path)?;
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            let listener = tokio::net::UnixListener::bind(&path)?;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
            Ok(Self {
                path,
                listener,
                _lock: lock,
            })
        }
    }

    #[cfg(windows)]
    fn bind_windows(paths: &Paths) -> Result<Self> {
        let path = paths.socket();
        let name = path
            .to_str()
            .ok_or_else(|| LocalWebError::Config("non-UTF-8 pipe name".into()))?;
        let pending = crate::windows_ipc::create_server(name, true)?;
        Ok(Self {
            path,
            pending: Some(pending),
        })
    }

    /// Returns the endpoint path or pipe name.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Accepts one client connection.
    ///
    /// # Errors
    ///
    /// Returns an error when the listener cannot produce another connection.
    pub async fn accept(&mut self) -> Result<ServerStream> {
        #[cfg(unix)]
        {
            let (stream, _) = self.listener.accept().await?;
            Ok(stream)
        }
        #[cfg(windows)]
        {
            let name = self
                .path
                .to_str()
                .ok_or_else(|| LocalWebError::Config("non-UTF-8 pipe name".into()))?;
            // A previous accept may have failed to create its replacement
            // instance. Recreate it here instead of leaving the listener wedged.
            let pending = match self.pending.take() {
                Some(pending) => pending,
                None => crate::windows_ipc::create_server(name, false)?,
            };
            if let Err(error) = pending.connect().await {
                // The failed instance is dropped; the next accept starts from a
                // fresh instance instead of reusing a partially connected one.
                return Err(error.into());
            }
            match crate::windows_ipc::create_server(name, false) {
                Ok(next) => self.pending = Some(next),
                Err(error) => {
                    // The accepted connection is still valid. Remember that the
                    // next instance is missing so the following accept recovers.
                    eprintln!("cannot create the next daemon pipe instance: {error}");
                    self.pending = None;
                }
            }
            Ok(pending)
        }
    }

    /// Releases the endpoint after the daemon stops.
    ///
    /// # Errors
    ///
    /// Returns an error when a Unix socket file cannot be removed.
    pub fn cleanup(&self) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            if self.path.exists() {
                std::fs::remove_file(&self.path)?;
            }
        }
        #[cfg(windows)]
        {
            let _ = &self.path;
        }
        Ok(())
    }
}

/// Connects to the per-user daemon.
///
/// # Errors
///
/// Returns an error when no daemon is listening at the endpoint.
pub async fn connect(paths: &Paths) -> Result<ClientStream> {
    let path = paths.socket();
    #[cfg(unix)]
    {
        Ok(tokio::net::UnixStream::connect(&path).await?)
    }
    #[cfg(windows)]
    {
        let name = path
            .to_str()
            .ok_or_else(|| LocalWebError::Config("non-UTF-8 pipe name".into()))?;
        Ok(crate::windows_ipc::connect(name).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn fixture() -> (PathBuf, Paths) {
        let root = std::env::temp_dir().join(format!(
            "lw-sock-{}",
            &uuid::Uuid::new_v4().simple().to_string()[..8]
        ));
        let paths = Paths {
            config: root.join("c"),
            data: root.join("d"),
            state: root.join("s"),
            runtime: root.join("r"),
        };
        paths.create().unwrap();
        (root, paths)
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn binding_preserves_an_existing_regular_file() {
        let (root, paths) = fixture();
        std::fs::write(paths.socket(), b"keep").unwrap();
        DaemonListener::bind(&paths).await.unwrap_err();
        assert_eq!(std::fs::read(paths.socket()).unwrap(), b"keep");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn stale_socket_fixture() {
        let Some(path) = std::env::var_os("LOCALWEB_TEST_STALE_SOCKET") else {
            return;
        };
        drop(std::os::unix::net::UnixListener::bind(path).unwrap());
    }

    #[cfg(unix)]
    #[tokio::test]
    #[expect(
        clippy::used_underscore_binding,
        reason = "models an inherited copy of the deliberately held RAII lock"
    )]
    async fn only_one_daemon_can_own_the_endpoint() {
        let (root, paths) = fixture();
        let first = DaemonListener::bind(&paths).await.unwrap();
        DaemonListener::bind(&paths).await.unwrap_err();
        // Model the descriptor a concurrent fork can retain until exec.
        let inherited = first._lock.0.try_clone().unwrap();
        first.cleanup().unwrap();
        drop(first);
        // Recover the stale socket left by a process that died without cleanup.
        // Create it in a child process: a listener created here could be
        // inherited by a concurrent fork and keep answering until exec, which
        // would make the recovery check nondeterministic.
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "transport::tests::stale_socket_fixture",
                "--nocapture",
            ])
            .env("LOCALWEB_TEST_STALE_SOCKET", paths.socket())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "stale socket fixture failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let recovered = DaemonListener::bind(&paths).await.unwrap();
        recovered.cleanup().unwrap();
        drop(recovered);
        drop(inherited);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn unix_socket_path_prefers_runtime_directory() {
        let paths = Paths::discover().unwrap();
        assert!(paths.socket().starts_with(&paths.runtime));
    }

    #[cfg(windows)]
    #[test]
    fn windows_socket_is_a_named_pipe() {
        let paths = Paths::discover().unwrap();
        assert!(paths.socket().to_string_lossy().starts_with(r"\\.\pipe\"));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn windows_listener_recovers_a_missing_pending_instance() {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

        let name = format!(r"\\.\pipe\lw-recover-{}", uuid::Uuid::new_v4());
        let mut listener = DaemonListener {
            path: PathBuf::from(&name),
            pending: Some(crate::windows_ipc::create_server(&name, true).unwrap()),
        };

        let client_name = name.clone();
        let client = tokio::spawn(async move { crate::windows_ipc::connect(&client_name).await });
        let mut server = listener.accept().await.unwrap();
        let mut client = client.await.unwrap().unwrap();
        client.write_all(b"one").await.unwrap();
        let mut buffer = [0; 3];
        server.read_exact(&mut buffer).await.unwrap();
        assert_eq!(&buffer, b"one");
        drop((client, server));

        // Model a failed replacement-instance creation after the first accept.
        listener.pending = None;
        let client_name = name.clone();
        let client = tokio::spawn(async move { crate::windows_ipc::connect(&client_name).await });
        let mut server = listener.accept().await.unwrap();
        let mut client = client.await.unwrap().unwrap();
        client.write_all(b"two").await.unwrap();
        let mut buffer = [0; 3];
        server.read_exact(&mut buffer).await.unwrap();
        assert_eq!(&buffer, b"two");
    }
}
