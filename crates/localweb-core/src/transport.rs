//! Cross-platform transport for the daemon protocol.
//!
//! Unix systems use a filesystem socket inside the per-user runtime
//! directory; Windows uses a per-user named pipe instead.

use crate::{LocalWebError, Paths, Result};
use std::path::{Path, PathBuf};

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
    #[cfg(windows)]
    pending: tokio::net::windows::named_pipe::NamedPipeServer,
}

impl DaemonListener {
    /// Binds the endpoint, replacing a stale socket file on Unix.
    ///
    /// # Errors
    ///
    /// Returns an error when another daemon already answers or the endpoint
    /// cannot be created.
    pub async fn bind(paths: &Paths) -> Result<Self> {
        let path = paths.socket();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            if path.exists() {
                match tokio::net::UnixStream::connect(&path).await {
                    Ok(_) => {
                        return Err(LocalWebError::Service(format!(
                            "localwebd is already running at {}",
                            path.display()
                        )));
                    }
                    Err(_) => std::fs::remove_file(&path)?,
                }
            }
            let listener = tokio::net::UnixListener::bind(&path)?;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
            Ok(Self { path, listener })
        }
        #[cfg(windows)]
        {
            use tokio::net::windows::named_pipe::{ClientOptions, ServerOptions};
            let name = path
                .to_str()
                .ok_or_else(|| LocalWebError::Config("non-UTF-8 pipe name".into()))?;
            if ClientOptions::new().open(name).is_ok() {
                return Err(LocalWebError::Service(format!(
                    "localwebd is already running at {name}"
                )));
            }
            let pending = ServerOptions::new()
                .first_pipe_instance(true)
                .create(name)?;
            Ok(Self { path, pending })
        }
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
            use tokio::net::windows::named_pipe::ServerOptions;
            self.pending.connect().await?;
            let name = self
                .path
                .to_str()
                .ok_or_else(|| LocalWebError::Config("non-UTF-8 pipe name".into()))?;
            let next = ServerOptions::new().create(name)?;
            let connected = std::mem::replace(&mut self.pending, next);
            Ok(connected)
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
        use tokio::net::windows::named_pipe::ClientOptions;
        let name = path
            .to_str()
            .ok_or_else(|| LocalWebError::Config("non-UTF-8 pipe name".into()))?;
        Ok(ClientOptions::new().open(name)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
