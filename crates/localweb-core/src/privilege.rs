//! Runs the privileged helper through the native administrative prompt.

use crate::{LocalWebError, Result};
use std::path::Path;

/// Runs `helper operation` with administrator rights, feeding `input` on stdin.
///
/// Uses `pkexec` on Linux, `osascript` on macOS and the UAC `runas` verb on
/// Windows. The caller must have validated that `input` is safe to hand to a
/// root process.
///
/// # Errors
///
/// Returns an error when the prompt is unavailable, cancelled, or the helper
/// exits unsuccessfully.
pub fn run_helper(helper: &Path, operation: &str, input: &[u8]) -> Result<()> {
    if operation.is_empty()
        || !operation
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'-')
    {
        return Err(LocalWebError::Config("invalid helper operation".into()));
    }
    #[cfg(target_os = "macos")]
    {
        macos::run(helper, operation, input)
    }
    #[cfg(windows)]
    {
        windows::run(helper, operation, input)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        linux::run(helper, operation, input)
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod linux {
    use super::{LocalWebError, Result};
    use std::{
        io::Write as _,
        path::Path,
        process::{Command, Stdio},
    };

    pub(super) fn run(helper: &Path, operation: &str, input: &[u8]) -> Result<()> {
        let mut child = Command::new("pkexec")
            .arg(helper)
            .arg(operation)
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|error| LocalWebError::Service(format!("cannot launch pkexec: {error}")))?;
        child
            .stdin
            .take()
            .ok_or_else(|| LocalWebError::Service("helper stdin unavailable".into()))?
            .write_all(input)?;
        if !child.wait()?.success() {
            return Err(LocalWebError::Service(format!(
                "privileged helper `{operation}` failed; a polkit agent must be running and the authorization granted"
            )));
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{LocalWebError, Result, apple_script_escape, shell_quote};
    use crate::Paths;
    use std::{
        fs,
        io::Write as _,
        os::unix::fs::OpenOptionsExt as _,
        path::{Path, PathBuf},
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };

    pub(super) fn run(helper: &Path, operation: &str, input: &[u8]) -> Result<()> {
        let payload = payload_path()?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&payload)?;
        file.write_all(input)?;
        drop(file);

        // SAFETY: getuid has no preconditions and always succeeds.
        let uid = unsafe { libc::getuid() };
        let mut command = format!("LOCALWEB_INVOKING_UID={uid}");
        if let Ok(paths) = Paths::discover() {
            command.push_str(" LOCALWEB_RUNTIME_DIR=");
            command.push_str(&shell_quote(&paths.runtime.to_string_lossy()));
        }
        command.push(' ');
        command.push_str(&shell_quote(&helper.to_string_lossy()));
        command.push(' ');
        command.push_str(&shell_quote(operation));
        command.push_str(" < ");
        command.push_str(&shell_quote(&payload.to_string_lossy()));

        let script = format!(
            "do shell script \"{}\" with administrator privileges",
            apple_script_escape(&command)
        );
        let status = Command::new("/usr/bin/osascript")
            .arg("-e")
            .arg(&script)
            .status();
        let _ = fs::remove_file(&payload);
        if !status
            .map_err(|error| LocalWebError::Service(format!("cannot launch osascript: {error}")))?
            .success()
        {
            return Err(LocalWebError::Service(format!(
                "privileged helper `{operation}` failed; the administrator prompt may have been cancelled"
            )));
        }
        Ok(())
    }

    fn payload_path() -> Result<PathBuf> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| LocalWebError::Service(error.to_string()))?
            .as_nanos();
        Ok(std::env::temp_dir().join(format!(
            "localweb-helper-{}-{nanos}.json",
            std::process::id()
        )))
    }
}

#[cfg(windows)]
mod windows {
    use super::{LocalWebError, Result};
    use std::{
        fs,
        io::Write as _,
        path::{Path, PathBuf},
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };

    pub(super) fn run(helper: &Path, operation: &str, input: &[u8]) -> Result<()> {
        let payload = payload_path()?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&payload)?;
        file.write_all(input)?;
        drop(file);

        // A payload file is used instead of stdin because the UAC `runas`
        // verb launches the helper through ShellExecute and cannot redirect
        // the standard input handle.
        let arguments = format!(
            "{operation} --input \"{}\"",
            payload.display().to_string().replace('"', "\"\"")
        );
        let script = format!(
            "$ErrorActionPreference='Stop'; $p = Start-Process -FilePath '{}' -ArgumentList '{}' -Verb RunAs -Wait -PassThru; exit $p.ExitCode",
            ps_escape(&helper.display().to_string()),
            ps_escape(&arguments)
        );
        let status = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .status();
        let _ = fs::remove_file(&payload);
        match status {
            Ok(status) if status.success() => Ok(()),
            Ok(_) => Err(LocalWebError::Service(format!(
                "privileged helper `{operation}` failed; the administrator prompt may have been cancelled"
            ))),
            Err(error) => Err(LocalWebError::Service(format!(
                "cannot launch powershell.exe: {error}"
            ))),
        }
    }

    fn ps_escape(value: &str) -> String {
        value.replace('\'', "''")
    }

    fn payload_path() -> Result<PathBuf> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| LocalWebError::Service(error.to_string()))?
            .as_nanos();
        Ok(std::env::temp_dir().join(format!(
            "localweb-helper-{}-{nanos}.json",
            std::process::id()
        )))
    }
}

#[cfg_attr(
    not(target_os = "macos"),
    allow(dead_code, reason = "used only by the macOS privilege launcher")
)]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg_attr(
    not(target_os = "macos"),
    allow(dead_code, reason = "used only by the macOS privilege launcher")
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
