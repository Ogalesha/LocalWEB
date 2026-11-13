//! Runs the privileged helper through the native administrative prompt.

use crate::{LocalWebError, Result};
use std::{
    io::Write as _,
    path::Path,
    process::{Command, Stdio},
};

/// Runs `helper operation` with administrator rights and validated input.
///
/// # Errors
///
/// Returns an error when authorization or the helper fails.
pub fn run_helper(helper: &Path, operation: &str, input: &[u8]) -> Result<()> {
    let (mut command, payload) = helper_command(helper, operation, input)?;
    command.stdin(if payload.is_none() {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    let mut child = command.spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input)?;
    }
    check_status(child.wait()?.success(), operation)
}

/// Runs a helper with bounded output and a five-minute authorization deadline.
///
/// Cancellation stops the owned launcher. An elevated transaction may already
/// have committed; callers must refresh before retrying after interruption.
///
/// # Errors
///
/// Returns an error on denial, timeout, or helper failure.
pub async fn run_helper_async(helper: &Path, operation: &str, input: &[u8]) -> Result<()> {
    let (command, payload) = helper_command(helper, operation, input)?;
    let mut command = tokio::process::Command::from(command);
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(300),
        crate::process::output_with_input(&mut command, 65536, payload.is_none().then_some(input)),
    )
    .await
    .map_err(|_elapsed| {
        LocalWebError::Service(
            "Administrator approval timed out. Refresh the current state before retrying.".into(),
        )
    })??;
    check_status(result.status.success(), operation)
}

fn check_status(success: bool, operation: &str) -> Result<()> {
    if !success {
        return Err(LocalWebError::Service(format!(
            "The administrator action `{operation}` did not complete. Check authorization and refresh before retrying."
        )));
    }
    Ok(())
}

// Retaining this guard across the command also removes payloads on cancellation.
struct Payload(std::path::PathBuf);

impl Drop for Payload {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[cfg(any(windows, target_os = "macos"))]
impl Payload {
    fn create(input: &[u8]) -> Result<Self> {
        let payload = Self(
            std::env::temp_dir().join(format!("localweb-helper-{}.json", uuid::Uuid::new_v4())),
        );
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600)
        };
        options.open(&payload.0)?.write_all(input)?;
        Ok(payload)
    }
}

fn helper_command(
    helper: &Path,
    operation: &str,
    input: &[u8],
) -> Result<(Command, Option<Payload>)> {
    if operation.is_empty()
        || !operation
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'-')
    {
        return Err(LocalWebError::Config("invalid helper operation".into()));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = input;
        let mut command = Command::new("pkexec");
        command.arg(helper).arg(operation);
        Ok((command, None))
    }
    #[cfg(target_os = "macos")]
    {
        let payload = Payload::create(input)?;
        // SAFETY: getuid has no preconditions.
        let uid = unsafe { libc::getuid() };
        let mut shell = format!("LOCALWEB_INVOKING_UID={uid}");
        if let Ok(paths) = crate::Paths::discover() {
            shell.push_str(" LOCALWEB_RUNTIME_DIR=");
            shell.push_str(&shell_quote(&paths.runtime.to_string_lossy()));
        }
        for argument in [&*helper.to_string_lossy(), operation] {
            shell.push(' ');
            shell.push_str(&shell_quote(argument));
        }
        shell.push_str(" < ");
        shell.push_str(&shell_quote(&payload.0.to_string_lossy()));
        let script = format!(
            "do shell script \"{}\" with administrator privileges",
            apple_script_escape(&shell)
        );
        let mut command = Command::new("/usr/bin/osascript");
        command.args(["-e", &script]);
        Ok((command, Some(payload)))
    }
    #[cfg(windows)]
    {
        let payload = Payload::create(input)?;
        let arguments = format!(
            "{operation} --input \"{}\"",
            payload.0.display().to_string().replace('"', "\"\"")
        );
        let script = format!(
            "$ErrorActionPreference='Stop'; $p = Start-Process -FilePath '{}' -ArgumentList '{}' -Verb RunAs -Wait -PassThru; exit $p.ExitCode",
            helper.display().to_string().replace('\'', "''"),
            arguments.replace('\'', "''")
        );
        let mut command = Command::new(
            crate::platform::windows_system_directory()?
                .join("WindowsPowerShell/v1.0/powershell.exe"),
        );
        command.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
        Ok((command, Some(payload)))
    }
}

#[cfg_attr(
    not(target_os = "macos"),
    allow(dead_code, reason = "macOS privilege launcher")
)]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg_attr(
    not(target_os = "macos"),
    allow(dead_code, reason = "macOS privilege launcher")
)]
fn apple_script_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quoting_neutralizes_single_quotes() {
        assert_eq!(shell_quote("plain"), "'plain'");
        assert_eq!(shell_quote("a'b"), "'a'\\''b'");
        assert_eq!(shell_quote(""), "''");
    }

    #[test]
    fn applescript_escaping_protects_quotes_and_backslashes() {
        assert_eq!(apple_script_escape("plain"), "plain");
        assert_eq!(apple_script_escape("say \"hi\""), "say \\\"hi\\\"");
        assert_eq!(apple_script_escape("a\\b"), "a\\\\b");
    }
}
