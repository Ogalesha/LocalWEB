//! Per-user service lifecycle for systemd (Linux), launchd (macOS) and Task
//! Scheduler (Windows).

#[cfg(not(windows))]
use crate::atomic_write;
use crate::{LocalWebError, Paths, Result};
#[cfg(not(windows))]
use std::fs;
use std::{path::PathBuf, process::Command};

/// systemd unit that runs the daemon on Linux.
#[cfg(all(unix, not(target_os = "macos")))]
pub const DAEMON_UNIT: &str = "localwebd.service";
/// launchd label that runs the daemon on macOS.
#[cfg(target_os = "macos")]
pub const DAEMON_LABEL: &str = "dev.localweb.daemon";
/// launchd label that owns the HTTPS loopback socket on macOS.
#[cfg(target_os = "macos")]
pub const GATEWAY_LABEL: &str = "dev.localweb.gateway";
/// Scheduled task that runs the daemon on Windows.
#[cfg(windows)]
pub const DAEMON_TASK: &str = "LocalWeb Daemon";

/// Resolves the installed daemon executable.
///
/// # Errors
///
/// Returns an error when neither the packaged nor the sibling binary exists.
pub fn daemon_binary() -> Result<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let installed = PathBuf::from("/usr/local/libexec/localweb/localwebd");
        if installed.is_file() {
            return Ok(installed);
        }
    }
    let candidate = std::env::current_exe()?
        .with_file_name(format!("localwebd{}", std::env::consts::EXE_SUFFIX));
    if !candidate.is_file() {
        return Err(LocalWebError::Service(format!(
            "the LocalWeb daemon is missing at {}",
            candidate.display()
        )));
    }
    Ok(candidate)
}

/// Restarts the daemon, replacing a stale process left by an upgrade.
///
/// # Errors
///
/// Returns an error when the native service manager rejects the request.
pub fn restart_daemon() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        macos::restart_daemon()
    }
    #[cfg(windows)]
    {
        windows::restart_daemon()
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        linux::run(
            "systemctl",
            &["--user", "restart", DAEMON_UNIT],
            "systemctl could not start localwebd.service",
        )
    }
}

/// Enables or disables starting the daemon at login.
///
/// # Errors
///
/// Returns an error when the native service manager rejects the request.
pub fn set_autostart(enable: bool) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        macos::set_autostart(enable)
    }
    #[cfg(windows)]
    {
        windows::set_autostart(enable)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let action = if enable { "enable" } else { "disable" };
        linux::run(
            "systemctl",
            &["--user", action, DAEMON_UNIT],
            &format!("systemctl --user {action} {DAEMON_UNIT} failed"),
        )
    }
}

/// Installs the native per-user service definition (systemd unit, `LaunchAgents`
/// or a Task Scheduler entry).
///
/// # Errors
///
/// Returns an error when a definition cannot be written or reloaded.
pub fn write_user_units(paths: &Paths) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        macos::write_user_units(paths)
    }
    #[cfg(windows)]
    {
        windows::write_user_units(paths)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = paths;
        linux::write_user_units()
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod linux {
    use super::{
        Command, DAEMON_UNIT, LocalWebError, PathBuf, Result, atomic_write, daemon_binary, fs,
    };

    pub(super) fn run(program: &str, args: &[&str], message: &str) -> Result<()> {
        let output = Command::new(program).args(args).output()?;
        if !output.status.success() {
            return Err(LocalWebError::Service(format!(
                "{message}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(())
    }

    pub(super) fn write_user_units() -> Result<()> {
        if !std::path::Path::new("/usr/lib/systemd/user/localwebd.service").is_file() {
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .ok_or_else(|| LocalWebError::Config("HOME is unset".into()))?;
            let unit_dir = home.join(".config/systemd/user");
            fs::create_dir_all(&unit_dir)?;
            let daemon = daemon_binary()?;
            let unit = format!(
                "[Unit]\nDescription=LocalWeb user daemon\nAfter=default.target\n\n[Service]\nType=simple\nExecStart={}\nRestart=on-failure\nNoNewPrivileges=yes\nPrivateTmp=yes\n\n[Install]\nWantedBy=default.target\n",
                daemon.display()
            );
            atomic_write(&unit_dir.join(DAEMON_UNIT), unit.as_bytes(), 0o600)?;
        }
        run(
            "systemctl",
            &["--user", "daemon-reload"],
            "systemctl --user daemon-reload failed",
        )
    }
}

#[cfg(windows)]
mod windows {
    use super::{Command, DAEMON_TASK, LocalWebError, Paths, Result, daemon_binary};
    use std::process::Stdio;

    pub(super) fn restart_daemon() -> Result<()> {
        ensure_task()?;
        let _ = Command::new("taskkill.exe")
            .args(["/IM", "localwebd.exe", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        run(
            &["/Run", "/TN", DAEMON_TASK],
            "schtasks could not run the LocalWeb daemon task",
        )
    }

    pub(super) fn set_autostart(enable: bool) -> Result<()> {
        ensure_task()?;
        let flag = if enable { "/ENABLE" } else { "/DISABLE" };
        run(
            &["/Change", "/TN", DAEMON_TASK, flag],
            "schtasks could not change the LocalWeb daemon task",
        )
    }

    pub(super) fn write_user_units(_paths: &Paths) -> Result<()> {
        ensure_task()?;
        // Match the Linux default: installed, but login autostart stays off
        // until `localweb autostart enable` is requested.
        run(
            &["/Change", "/TN", DAEMON_TASK, "/DISABLE"],
            "schtasks could not disable the LocalWeb daemon task",
        )
    }

    fn ensure_task() -> Result<()> {
        if task_exists() { Ok(()) } else { create_task() }
    }

    fn task_exists() -> bool {
        Command::new("schtasks.exe")
            .args(["/Query", "/TN", DAEMON_TASK])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    fn create_task() -> Result<()> {
        let daemon = daemon_binary()?;
        // Launch through hidden PowerShell so the console daemon does not
        // flash a terminal window at logon.
        let script = format!(
            "Start-Process -FilePath '{}' -WindowStyle Hidden",
            daemon.display().to_string().replace('\'', "''")
        );
        let launch = format!("powershell.exe -NoProfile -WindowStyle Hidden -Command \"{script}\"");
        run(
            &[
                "/Create",
                "/F",
                "/TN",
                DAEMON_TASK,
                "/SC",
                "ONLOGON",
                "/RL",
                "LIMITED",
                "/TR",
                &launch,
            ],
            "schtasks could not install the LocalWeb daemon task",
        )
    }

    fn run(args: &[&str], message: &str) -> Result<()> {
        let output = Command::new("schtasks.exe").args(args).output()?;
        if !output.status.success() {
            return Err(LocalWebError::Service(format!(
                "{message}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{
        Command, DAEMON_LABEL, GATEWAY_LABEL, LocalWebError, PathBuf, Paths, Result, atomic_write,
        daemon_binary, fs,
    };
    use std::{os::unix::fs::OpenOptionsExt as _, process::Stdio};

    pub(super) fn restart_daemon() -> Result<()> {
        let domain = format!("gui/{}", uid());
        let target = format!("{domain}/{DAEMON_LABEL}");
        if !loaded(&target) {
            let plist = plist_dir()?.join(format!("{DAEMON_LABEL}.plist"));
            if !plist.is_file() {
                return Err(LocalWebError::Config(
                    "the launchd agent is missing; run `localweb setup` first".into(),
                ));
            }
            let plist = plist.to_string_lossy().into_owned();
            return run(
                "/bin/launchctl",
                &["bootstrap", &domain, &plist],
                "launchctl could not bootstrap the LocalWeb daemon",
            );
        }
        run(
            "/bin/launchctl",
            &["kickstart", "-k", &target],
            "launchctl could not restart the LocalWeb daemon",
        )
    }

    pub(super) fn set_autostart(enable: bool) -> Result<()> {
        let target = format!("gui/{}/{DAEMON_LABEL}", uid());
        let action = if enable { "enable" } else { "disable" };
        run(
            "/bin/launchctl",
            &[action, &target],
            &format!("launchctl could not {action} the LocalWeb daemon"),
        )
    }

    pub(super) fn write_user_units(paths: &Paths) -> Result<()> {
        let daemon = daemon_binary()?;
        let gateway = daemon.with_file_name("localweb-gateway");
        if !gateway.is_file() {
            return Err(LocalWebError::Service(format!(
                "the LocalWeb gateway is missing at {}",
                gateway.display()
            )));
        }
        let logs = paths.state.join("logs");
        fs::create_dir_all(&logs)?;
        for file in ["daemon.log", "gateway.log"] {
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .mode(0o600)
                .open(logs.join(file))?;
        }
        let directory = plist_dir()?;
        fs::create_dir_all(&directory)?;
        let daemon_plist = daemon_plist(&daemon, &logs.join("daemon.log"));
        atomic_write(
            &directory.join(format!("{DAEMON_LABEL}.plist")),
            daemon_plist.as_bytes(),
            0o600,
        )?;
        let gateway_plist = gateway_plist(&gateway, &logs.join("gateway.log"));
        atomic_write(
            &directory.join(format!("{GATEWAY_LABEL}.plist")),
            gateway_plist.as_bytes(),
            0o600,
        )?;
        Ok(())
    }

    fn daemon_plist(daemon: &std::path::Path, log: &std::path::Path) -> String {
        format!(
            concat!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
                "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n",
                "<plist version=\"1.0\">\n<dict>\n",
                "  <key>Label</key><string>{label}</string>\n",
                "  <key>ProgramArguments</key><array><string>{program}</string></array>\n",
                "  <key>RunAtLoad</key><true/>\n",
                "  <key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>\n",
                "  <key>EnvironmentVariables</key><dict><key>PATH</key><string>{path}</string></dict>\n",
                "  <key>StandardOutPath</key><string>{log}</string>\n",
                "  <key>StandardErrorPath</key><string>{log}</string>\n",
                "</dict>\n</plist>\n"
            ),
            label = DAEMON_LABEL,
            program = xml_escape(&daemon.to_string_lossy()),
            path = runtime_path_value(),
            log = xml_escape(&log.to_string_lossy()),
        )
    }

    fn gateway_plist(gateway: &std::path::Path, log: &std::path::Path) -> String {
        format!(
            concat!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
                "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n",
                "<plist version=\"1.0\">\n<dict>\n",
                "  <key>Label</key><string>{label}</string>\n",
                "  <key>ProgramArguments</key><array><string>{program}</string></array>\n",
                "  <key>RunAtLoad</key><false/>\n",
                "  <key>KeepAlive</key><true/>\n",
                "  <key>EnvironmentVariables</key><dict><key>PATH</key><string>{path}</string></dict>\n",
                "  <key>Sockets</key><dict><key>LocalWebHTTPS</key><dict>\n",
                "    <key>SockNodeName</key><string>127.0.0.1</string>\n",
                "    <key>SockServiceName</key><string>443</string>\n",
                "    <key>SockType</key><string>stream</string>\n",
                "  </dict></dict>\n",
                "  <key>StandardOutPath</key><string>{log}</string>\n",
                "  <key>StandardErrorPath</key><string>{log}</string>\n",
                "</dict>\n</plist>\n"
            ),
            label = GATEWAY_LABEL,
            program = xml_escape(&gateway.to_string_lossy()),
            path = runtime_path_value(),
            log = xml_escape(&log.to_string_lossy()),
        )
    }

    fn runtime_path_value() -> &'static str {
        "/opt/homebrew/bin:/opt/homebrew/sbin:/usr/local/bin:/usr/local/sbin:/usr/bin:/bin:/usr/sbin:/sbin"
    }

    fn plist_dir() -> Result<PathBuf> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| LocalWebError::Config("HOME is unset".into()))?;
        Ok(home.join("Library/LaunchAgents"))
    }

    fn loaded(target: &str) -> bool {
        Command::new("/bin/launchctl")
            .args(["print", target])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    pub(super) fn uid() -> u32 {
        // SAFETY: getuid has no preconditions and always succeeds.
        unsafe { libc::getuid() }
    }

    pub(super) fn run(program: &str, args: &[&str], message: &str) -> Result<()> {
        let output = Command::new(program).args(args).output()?;
        if !output.status.success() {
            return Err(LocalWebError::Service(format!(
                "{message}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(())
    }

    fn xml_escape(value: &str) -> String {
        value
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    }
}
