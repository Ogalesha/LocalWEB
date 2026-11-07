//! Bounded command output and cancellation of owned process trees.

use std::{
    io,
    process::{Output, Stdio},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::{Child, Command},
};

struct OwnedChild(Child);

impl Drop for OwnedChild {
    fn drop(&mut self) {
        if let Some(id) = self.0.id() {
            #[cfg(unix)]
            if let Ok(group) = i32::try_from(id) {
                // SAFETY: this unreaped child owns the process group created
                // below. Its ID cannot be reused until its handle is reaped.
                unsafe { libc::kill(-group, libc::SIGKILL) };
            }
            #[cfg(windows)]
            {
                let _ = terminate_tree(id);
            }
        }
    }
}

/// Stops a verified Windows process tree using the system utility, avoiding
/// executable lookup in the current project directory or PATH.
///
/// # Errors
///
/// Returns an error for an invalid PID or failure to launch the system tool.
#[cfg(windows)]
pub fn terminate_tree(pid: u32) -> io::Result<Output> {
    use std::os::windows::process::CommandExt as _;
    if pid <= 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid process ID",
        ));
    }
    let system_root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    std::process::Command::new(std::path::PathBuf::from(system_root).join("System32/taskkill.exe"))
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .creation_flags(0x0800_0000)
        .stdin(Stdio::null())
        .output()
}

/// Runs a command, draining both pipes while retaining at most `cap + 1`
/// bytes per stream. The extra byte lets callers report truncation.
/// Dropping this future cancels its owned command and descendants.
///
/// # Errors
///
/// Returns an error when spawning, reading output, or waiting fails.
pub async fn output(command: &mut Command, cap: usize) -> io::Result<Output> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    let mut child = OwnedChild(command.spawn()?);
    let mut stdout = child
        .0
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("stdout is unavailable"))?;
    let mut stderr = child
        .0
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("stderr is unavailable"))?;
    let (stdout, stderr) =
        tokio::join!(read_stream(&mut stdout, cap), read_stream(&mut stderr, cap));
    // Keep the parent unreaped while descendants may still own pipe handles.
    // Cancellation can then safely identify and stop its process tree.
    let stdout = stdout?;
    let stderr = stderr?;
    let status = child.0.wait().await?;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

async fn read_stream(stream: &mut (impl AsyncRead + Unpin), cap: usize) -> io::Result<Vec<u8>> {
    let mut buffer = Vec::new();
    let mut chunk = [0; 4096];
    loop {
        let count = stream.read(&mut chunk).await?;
        if count == 0 {
            break;
        }
        let keep = count.min(cap.saturating_add(1).saturating_sub(buffer.len()));
        buffer.extend_from_slice(&chunk[..keep]);
    }
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_output_fixture() {
        if std::env::var_os("LOCALWEB_TEST_OUTPUT_FIXTURE").is_some() {
            use std::io::Write as _;
            for _ in 0..128 {
                std::io::stdout().write_all(&[b'o'; 8192]).unwrap();
                std::io::stderr().write_all(&[b'e'; 8192]).unwrap();
            }
        }
    }

    #[tokio::test]
    async fn drains_both_pipes_without_retaining_unbounded_output() {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "process::tests::large_output_fixture",
                "--nocapture",
            ])
            .env("LOCALWEB_TEST_OUTPUT_FIXTURE", "1");
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            output(&mut command, 16384),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(result.status.success());
        assert_eq!(result.stdout.len(), 16385);
        assert_eq!(result.stderr.len(), 16385);
    }

    #[test]
    fn process_tree_fixture() {
        let Some(mode) = std::env::var_os("LOCALWEB_TEST_PROCESS_TREE") else {
            return;
        };
        if mode == "parent" {
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "process::tests::process_tree_fixture",
                    "--nocapture",
                ])
                .env("LOCALWEB_TEST_PROCESS_TREE", "child")
                .spawn()
                .unwrap();
            let _ = child.wait();
            std::thread::sleep(std::time::Duration::from_secs(30));
        } else {
            std::fs::write(
                std::env::var_os("PID_FILE").unwrap(),
                std::process::id().to_string(),
            )
            .unwrap();
            std::thread::sleep(std::time::Duration::from_millis(700));
            std::fs::write(std::env::var_os("WORK_FILE").unwrap(), "work continued").unwrap();
        }
    }

    #[tokio::test]
    async fn cancellation_stops_descendants() {
        let path = std::env::temp_dir().join(format!("lw-command-{}", uuid::Uuid::new_v4()));
        let work = path.with_extension("work");
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "process::tests::process_tree_fixture",
                "--nocapture",
            ])
            .env("LOCALWEB_TEST_PROCESS_TREE", "parent")
            .env("PID_FILE", &path)
            .env("WORK_FILE", &work);
        let task = tokio::spawn(async move { output(&mut command, 1024).await });
        let mut started = false;
        for _ in 0..500 {
            if path.is_file() {
                started = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        task.abort();
        let _ = task.await;
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
        let continued = work.exists();
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(work);
        assert!(started, "test descendant did not start");
        assert!(!continued, "cancelled command left a descendant running");
    }
}
