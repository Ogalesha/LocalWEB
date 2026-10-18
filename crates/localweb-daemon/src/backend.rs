//! Starts and stops managed services with the native process supervisor.
//!
//! Linux uses transient systemd user units; macOS uses launchd for the HTTPS
//! gateway (so it can own port 443) and daemon-owned child processes for the
//! remaining services.

use localweb_core::{LocalWebError, Result, runtime::RuntimeLayout};
use std::path::Path;

/// Restarts (or starts) one managed service.
///
/// # Errors
///
/// Returns an error when the executable is missing or the service manager fails.
pub fn start_or_restart(unit: &str, executable: &Path, args: &[&str]) -> Result<()> {
    if !executable.is_file() {
        return Err(LocalWebError::RuntimeNotInstalled(
            executable.display().to_string(),
        ));
    }
    #[cfg(target_os = "macos")]
    {
        macos::start_or_restart(unit, executable, args)
    }
    #[cfg(not(target_os = "macos"))]
    {
        linux::start_or_restart(unit, executable, args)
    }
}

/// Starts a restart-on-exit worker inside the project directory.
///
/// # Errors
///
/// Returns an error when the worker cannot be supervised.
pub fn start_supervised(
    unit: &str,
    description: &str,
    working_directory: &Path,
    executable: &Path,
    worker_args: &[String],
    library_path: &str,
) -> Result<()> {
    if !executable.is_file() {
        return Err(LocalWebError::RuntimeNotInstalled(
            executable.display().to_string(),
        ));
    }
    #[cfg(target_os = "macos")]
    {
        let _ = (description, library_path);
        macos::start_supervised(unit, working_directory, executable, worker_args)
    }
    #[cfg(not(target_os = "macos"))]
    {
        linux::start_supervised(
            unit,
            description,
            working_directory,
            executable,
            worker_args,
            library_path,
        )
    }
}

/// Stops one managed service if it is running.
///
/// # Errors
///
/// Returns an error when the service manager fails.
pub fn stop(unit: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        macos::stop(unit)
    }
    #[cfg(not(target_os = "macos"))]
    {
        linux::stop(unit)
    }
}

/// Reports whether a managed service is currently running.
#[must_use]
pub fn is_active(unit: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        macos::is_active(unit)
    }
    #[cfg(not(target_os = "macos"))]
    {
        linux::is_active(unit)
    }
}

/// Waits until a just-started service is stably active.
///
/// # Errors
///
/// Returns an error when the service exits during the settling window.
pub fn wait_active(unit: &str, service: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        macos::wait_active(unit, service)
    }
    #[cfg(not(target_os = "macos"))]
    {
        linux::wait_active(unit, service)
    }
}

#[cfg(not(target_os = "macos"))]
mod linux {
    use super::{LocalWebError, Path, Result, RuntimeLayout};
    use std::{
        ffi::OsString,
        process::{Command, Stdio},
        thread,
        time::Duration,
    };

    pub(super) fn start_or_restart(unit: &str, executable: &Path, args: &[&str]) -> Result<()> {
        if is_active(unit) {
            run_command("systemctl", &["--user", "restart", unit])?;
            return Ok(());
        }
        let name = format!("--unit={unit}");
        let executable = executable
            .to_str()
            .ok_or_else(|| LocalWebError::Config("non-UTF-8 executable path".into()))?;
        let mut command = Command::new("systemd-run");
        let library_path = format!(
            "--setenv=LD_LIBRARY_PATH={}",
            RuntimeLayout::discover().library_path()
        );
        command.args([
            "--user",
            &name,
            "--collect",
            "--property=StartLimitIntervalSec=0",
        ]);
        // The gateway keeps its file capability: both NoNewPrivileges
        // and PrivateTmp (via a user namespace) would neutralise it.
        if unit != "localweb-gateway" {
            command.arg("--property=NoNewPrivileges=yes");
            command.arg("--property=PrivateTmp=yes");
        }
        command.arg(&library_path);
        for key in [
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_STATE_HOME",
            "XDG_RUNTIME_DIR",
            "LOCALWEB_RUNTIME_ROOT",
        ] {
            if let Some(value) = std::env::var_os(key) {
                let mut argument = OsString::from(format!("--setenv={key}="));
                argument.push(value);
                command.arg(argument);
            }
        }
        command.args(["--", executable]);
        command.args(args);
        let output = command.output()?;
        if !output.status.success() {
            return Err(LocalWebError::Service(format!(
                "systemd-run {unit}: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(())
    }

    pub(super) fn start_supervised(
        unit: &str,
        description: &str,
        working_directory: &Path,
        executable: &Path,
        worker_args: &[String],
        library_path: &str,
    ) -> Result<()> {
        let arguments = supervisor_systemd_args(
            unit,
            description,
            working_directory,
            executable,
            worker_args,
            library_path,
        );
        let output = Command::new("systemd-run").args(arguments).output()?;
        if !output.status.success() {
            return Err(LocalWebError::Service(format!(
                "systemd-run {unit}: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        wait_active(unit, "project supervisor")
    }

    pub(super) fn stop(unit: &str) -> Result<()> {
        if is_active(unit) {
            run_command("systemctl", &["--user", "stop", unit])?;
        }
        Ok(())
    }

    pub(super) fn is_active(unit: &str) -> bool {
        Command::new("systemctl")
            .args(["--user", "is-active", "--quiet", unit])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    pub(super) fn wait_active(unit: &str, service: &str) -> Result<()> {
        for _ in 0..20 {
            if is_active(unit) {
                thread::sleep(Duration::from_millis(200));
                if is_active(unit) {
                    return Ok(());
                }
            }
            thread::sleep(Duration::from_millis(100));
        }
        Err(LocalWebError::Service(format!(
            "{service} did not remain active in {unit}; inspect it with `journalctl --user -u {unit}`"
        )))
    }

    fn run_command(program: &str, args: &[&str]) -> Result<()> {
        let output = Command::new(program).args(args).output()?;
        if !output.status.success() {
            return Err(LocalWebError::Service(format!(
                "{program} {}: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(())
    }

    fn supervisor_systemd_args(
        unit: &str,
        description: &str,
        working_directory: &Path,
        executable: &Path,
        worker_args: &[String],
        library_path: &str,
    ) -> Vec<OsString> {
        let mut arguments = vec![
            OsString::from("--user"),
            OsString::from(format!("--unit={unit}")),
            OsString::from("--collect"),
            OsString::from(format!("--description=LocalWeb worker for {description}")),
            OsString::from(format!(
                "--working-directory={}",
                working_directory.display()
            )),
            OsString::from("--property=NoNewPrivileges=yes"),
            OsString::from("--property=PrivateTmp=yes"),
            OsString::from("--property=Restart=always"),
            OsString::from("--property=RestartSec=2s"),
            OsString::from("--property=KillMode=control-group"),
            OsString::from(format!("--setenv=LD_LIBRARY_PATH={library_path}")),
            OsString::from("--"),
            executable.as_os_str().to_owned(),
        ];
        arguments.extend(worker_args.iter().map(OsString::from));
        arguments
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn supervisor_arguments_never_pass_through_a_shell() {
            let arguments = supervisor_systemd_args(
                "localweb-project-demo-supervisor",
                "demo",
                Path::new("/tmp/demo project"),
                Path::new("/usr/bin/php8.5"),
                &["artisan".into(), "queue:work; touch /tmp/unsafe".into()],
                "/usr/lib/localweb/runtime/lib",
            );
            assert!(!arguments.iter().any(|argument| argument == "sh"));
            assert!(!arguments.iter().any(|argument| argument == "-c"));
            assert!(
                arguments
                    .iter()
                    .any(|argument| argument == "queue:work; touch /tmp/unsafe")
            );
            assert!(
                arguments
                    .iter()
                    .any(|argument| argument == "--working-directory=/tmp/demo project")
            );
        }
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{LocalWebError, Path, Result, RuntimeLayout};
    use localweb_core::{Paths, service::GATEWAY_LABEL};
    use std::{
        ffi::OsString,
        fs,
        os::unix::fs::OpenOptionsExt as _,
        path::PathBuf,
        process::{Command, Stdio},
        thread,
        time::Duration,
    };

    /// Restart loop for supervised workers. Worker arguments never pass through
    /// the shell: they are positional parameters referenced as `"$@"`.
    const SUPERVISOR_LOOP: &str = r#"child=; shutdown() { trap - TERM INT HUP; if [ -n "$child" ]; then kill "$child" 2>/dev/null; sleep 1; kill -9 "$child" 2>/dev/null; fi; exit 0; }; trap shutdown TERM INT HUP; while :; do "$@" & child=$!; wait "$child"; sleep 2; done"#;

    pub(super) fn start_or_restart(unit: &str, executable: &Path, args: &[&str]) -> Result<()> {
        if unit == "localweb-gateway" {
            return start_gateway();
        }
        stop(unit)?;
        let arguments: Vec<OsString> = args.iter().map(OsString::from).collect();
        spawn(unit, executable, &arguments, None, false)
    }

    pub(super) fn start_supervised(
        unit: &str,
        working_directory: &Path,
        executable: &Path,
        worker_args: &[String],
    ) -> Result<()> {
        stop(unit)?;
        let arguments: Vec<OsString> = worker_args.iter().map(OsString::from).collect();
        spawn(unit, executable, &arguments, Some(working_directory), true)?;
        wait_active(unit, "project supervisor")
    }

    pub(super) fn stop(unit: &str) -> Result<()> {
        if unit == "localweb-gateway" {
            return stop_gateway();
        }
        let path = pid_path(unit)?;
        let Ok(contents) = fs::read_to_string(&path) else {
            return Ok(());
        };
        let Ok(pid) = contents.trim().parse::<i32>() else {
            let _ = fs::remove_file(&path);
            return Ok(());
        };
        signal(pid, libc::SIGTERM);
        for _ in 0..200 {
            if !alive(pid) {
                let _ = fs::remove_file(&path);
                return Ok(());
            }
            thread::sleep(Duration::from_millis(50));
        }
        signal(pid, libc::SIGKILL);
        let _ = fs::remove_file(&path);
        Ok(())
    }

    pub(super) fn is_active(unit: &str) -> bool {
        if unit == "localweb-gateway" {
            return gateway_loaded();
        }
        let Ok(path) = pid_path(unit) else {
            return false;
        };
        fs::read_to_string(path)
            .ok()
            .and_then(|contents| contents.trim().parse::<i32>().ok())
            .is_some_and(alive)
    }

    pub(super) fn wait_active(unit: &str, service: &str) -> Result<()> {
        for _ in 0..20 {
            if is_active(unit) {
                thread::sleep(Duration::from_millis(200));
                if is_active(unit) {
                    return Ok(());
                }
            }
            thread::sleep(Duration::from_millis(100));
        }
        let logs = Paths::discover()
            .map(|paths| paths.state.join("logs").display().to_string())
            .unwrap_or_else(|_| "the LocalWeb log directory".into());
        Err(LocalWebError::Service(format!(
            "{service} did not remain active in {unit}; inspect {logs}"
        )))
    }

    fn spawn(
        unit: &str,
        executable: &Path,
        args: &[OsString],
        working_directory: Option<&Path>,
        supervised: bool,
    ) -> Result<()> {
        let paths = Paths::discover()?;
        let services = paths.runtime.join("services");
        fs::create_dir_all(&services)?;
        let logs = paths.state.join("logs");
        fs::create_dir_all(&logs)?;
        let log_path = logs.join(format!("{unit}.log"));
        let log = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(&log_path)?;
        let error_log = log.try_clone()?;
        let mut command = if supervised {
            let mut command = Command::new("/bin/sh");
            command
                .arg("-c")
                .arg(SUPERVISOR_LOOP)
                .arg("localweb-supervisor")
                .arg(executable)
                .args(args);
            command
        } else {
            let mut command = Command::new(executable);
            command.args(args);
            command
        };
        command
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(error_log))
            .env("PATH", macos_path())
            .env("LD_LIBRARY_PATH", RuntimeLayout::discover().library_path());
        if let Some(working_directory) = working_directory {
            command.current_dir(working_directory);
        }
        for key in [
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_STATE_HOME",
            "XDG_RUNTIME_DIR",
            "LOCALWEB_RUNTIME_ROOT",
        ] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        let mut child = command.spawn().map_err(|error| {
            LocalWebError::Service(format!(
                "cannot start {unit} ({}): {error}",
                executable.display()
            ))
        })?;
        let pid = child.id();
        fs::write(services.join(format!("{unit}.pid")), pid.to_string())?;
        thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }

    fn start_gateway() -> Result<()> {
        let plist = launch_agent("dev.localweb.gateway")?;
        if !plist.is_file() {
            return Err(LocalWebError::Config(
                "the gateway launch agent is missing; run `localweb setup` first".into(),
            ));
        }
        let domain = gui_domain();
        let target = format!("{domain}/{GATEWAY_LABEL}");
        if !gateway_loaded() {
            let plist = plist.to_string_lossy().into_owned();
            let output = Command::new("/bin/launchctl")
                .args(["bootstrap", &domain, &plist])
                .output()?;
            if !output.status.success() {
                return Err(LocalWebError::Service(format!(
                    "launchctl could not bootstrap the gateway: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                )));
            }
        }
        let output = Command::new("/bin/launchctl")
            .args(["kickstart", "-k", &target])
            .output()?;
        if !output.status.success() {
            return Err(LocalWebError::Service(format!(
                "launchctl could not start the gateway: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(())
    }

    fn stop_gateway() -> Result<()> {
        if !gateway_loaded() {
            return Ok(());
        }
        let target = format!("{}/{}", gui_domain(), GATEWAY_LABEL);
        let output = Command::new("/bin/launchctl")
            .args(["bootout", &target])
            .output()?;
        if !output.status.success() {
            return Err(LocalWebError::Service(format!(
                "launchctl could not stop the gateway: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(())
    }

    fn gateway_loaded() -> bool {
        let target = format!("{}/{}", gui_domain(), GATEWAY_LABEL);
        Command::new("/bin/launchctl")
            .args(["print", &target])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    fn pid_path(unit: &str) -> Result<PathBuf> {
        Ok(Paths::discover()?
            .runtime
            .join("services")
            .join(format!("{unit}.pid")))
    }

    fn launch_agent(label: &str) -> Result<PathBuf> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| LocalWebError::Config("HOME is unset".into()))?;
        Ok(home
            .join("Library/LaunchAgents")
            .join(format!("{label}.plist")))
    }

    fn gui_domain() -> String {
        format!("gui/{}", uid())
    }

    fn uid() -> u32 {
        // SAFETY: getuid has no preconditions and always succeeds.
        unsafe { libc::getuid() }
    }

    fn alive(pid: i32) -> bool {
        // SAFETY: signal 0 performs existence checking without delivering a signal.
        unsafe { libc::kill(pid, 0) == 0 }
    }

    fn signal(pid: i32, signal: i32) {
        // SAFETY: the pid comes from a pidfile written by this daemon; sending a
        // signal to a stale pid is a no-op that returns an error we ignore.
        unsafe {
            libc::kill(pid, signal);
        }
    }

    fn macos_path() -> String {
        let inherited = std::env::var("PATH").unwrap_or_default();
        format!("/opt/homebrew/bin:/opt/homebrew/sbin:/usr/local/bin:/usr/local/sbin:{inherited}")
    }
}
