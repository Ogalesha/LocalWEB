//! Starts and stops managed services with the native process supervisor.
//!
//! Linux uses transient systemd user units; macOS uses launchd for the HTTPS
//! gateway (so it can own port 443) and daemon-owned child processes for the
//! remaining services.

use localweb_core::{LocalWebError, Result, runtime::RuntimeLayout};
use std::{collections::BTreeMap, path::Path};

#[cfg(any(test, windows, target_os = "macos"))]
#[path = "backend_identity.rs"]
mod identity;

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
    #[cfg(windows)]
    {
        windows::start_or_restart(unit, executable, args)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
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
    environment: &BTreeMap<String, String>,
) -> Result<()> {
    if !executable.is_file() {
        return Err(LocalWebError::RuntimeNotInstalled(
            executable.display().to_string(),
        ));
    }
    #[cfg(target_os = "macos")]
    {
        let _ = (description, library_path);
        macos::start_supervised(
            unit,
            working_directory,
            executable,
            worker_args,
            environment,
        )
    }
    #[cfg(windows)]
    {
        let _ = description;
        windows::start_supervised(
            unit,
            working_directory,
            executable,
            worker_args,
            library_path,
            environment,
        )
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        linux::start_supervised(
            unit,
            description,
            working_directory,
            executable,
            worker_args,
            library_path,
            environment,
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
    #[cfg(windows)]
    {
        windows::stop(unit)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
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
    #[cfg(windows)]
    {
        windows::is_active(unit)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
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
    #[cfg(windows)]
    {
        windows::wait_active(unit, service)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        linux::wait_active(unit, service)
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod linux {
    use super::{BTreeMap, LocalWebError, Path, Result, RuntimeLayout};
    use std::{
        ffi::OsString,
        process::{Command, Stdio},
        thread,
        time::Duration,
    };

    pub(super) fn start_or_restart(unit: &str, executable: &Path, args: &[&str]) -> Result<()> {
        // A restart keeps the old transient ExecStart and environment. Recreate
        // the unit so changed executable paths and gateway arguments apply.
        stop(unit)?;
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
            "--expand-environment=no",
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
        environment: &BTreeMap<String, String>,
    ) -> Result<()> {
        let arguments = supervisor_systemd_args(
            unit,
            description,
            working_directory,
            executable,
            worker_args,
            library_path,
            environment,
        );
        let output = Command::new("systemd-run").args(arguments).output()?;
        if !output.status.success() {
            return Err(LocalWebError::Service(format!(
                "systemd-run {unit}: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        wait_active(unit, "background worker")
    }

    pub(super) fn stop(unit: &str) -> Result<()> {
        let output = Command::new("systemctl")
            .args(["--user", "show", "--property=LoadState", "--value", unit])
            .output()?;
        if String::from_utf8_lossy(&output.stdout).trim() == "not-found" {
            return Ok(());
        }
        if !output.status.success() {
            return Err(LocalWebError::Service(format!(
                "cannot inspect systemd unit {unit}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        // is-active is false during RestartSec, when a worker still has a
        // pending restart. Stop every loaded unit to cancel that job as well.
        run_command("systemctl", &["--user", "stop", unit])
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
        environment: &BTreeMap<String, String>,
    ) -> Vec<OsString> {
        let mut arguments = vec![
            OsString::from("--user"),
            OsString::from("--expand-environment=no"),
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
            OsString::from("--property=StartLimitIntervalSec=30s"),
            OsString::from("--property=StartLimitBurst=5"),
            OsString::from("--property=KillMode=control-group"),
        ];
        arguments.extend(
            environment
                .iter()
                .map(|(key, value)| OsString::from(format!("--setenv={key}={value}"))),
        );
        arguments.push(OsString::from(format!(
            "--setenv=LD_LIBRARY_PATH={library_path}"
        )));
        arguments.push(OsString::from(format!(
            "--setenv=LOCALWEB_PROJECT={description}"
        )));
        arguments.push(OsString::from("--"));
        arguments.push(executable.as_os_str().to_owned());
        arguments.extend(worker_args.iter().map(OsString::from));
        arguments
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        struct TestUnit(String);

        impl Drop for TestUnit {
            fn drop(&mut self) {
                let _ = stop(&self.0);
            }
        }

        #[test]
        #[ignore = "requires a systemd user session; creates only a temporary audit unit"]
        fn native_systemd_replaces_arguments_and_preserves_dollar_literals() {
            let unit = TestUnit(format!("localweb-audit-{}", uuid::Uuid::new_v4().simple()));
            let root = std::env::current_dir()
                .unwrap()
                .join("target")
                .join(&unit.0);
            std::fs::create_dir_all(&root).unwrap();
            let output = root.join("argv");
            let script = "import pathlib,sys,time; pathlib.Path(sys.argv[1]).write_text(sys.argv[2]); time.sleep(30)";
            for value in ["$HOME ${HOME} spaces; %literal", "updated arguments"] {
                start_or_restart(
                    &unit.0,
                    Path::new("/usr/bin/python3"),
                    &["-c", script, output.to_str().unwrap(), value],
                )
                .unwrap();
                for _ in 0..100 {
                    if std::fs::read_to_string(&output).is_ok_and(|text| text == value) {
                        break;
                    }
                    thread::sleep(Duration::from_millis(20));
                }
                assert_eq!(std::fs::read_to_string(&output).unwrap(), value);
            }
            stop(&unit.0).unwrap();
            std::fs::remove_dir_all(root).unwrap();
        }

        #[test]
        #[ignore = "requires a systemd user session; creates only a temporary audit unit"]
        fn native_systemd_stop_cancels_a_pending_worker_restart() {
            let unit = TestUnit(format!("localweb-audit-{}", uuid::Uuid::new_v4().simple()));
            let output = Command::new("systemd-run")
                .args([
                    "--user",
                    "--collect",
                    &format!("--unit={}", unit.0),
                    "--property=Restart=always",
                    "--property=RestartSec=60s",
                    "/bin/false",
                ])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let substate = || {
                let output = Command::new("systemctl")
                    .args(["--user", "show", "--property=SubState", "--value", &unit.0])
                    .output()
                    .unwrap();
                String::from_utf8_lossy(&output.stdout).trim().to_owned()
            };
            for _ in 0..100 {
                if substate() == "auto-restart" {
                    break;
                }
                thread::sleep(Duration::from_millis(20));
            }
            assert_eq!(substate(), "auto-restart");
            assert!(!is_active(&unit.0));
            stop(&unit.0).unwrap();
            assert_ne!(substate(), "auto-restart");
            assert!(!is_active(&unit.0));
        }

        #[test]
        #[ignore = "requires a systemd user session; creates only a temporary audit worker"]
        fn native_worker_preserves_environment_and_restarts_clean_exits() {
            let unit = TestUnit(format!("localweb-audit-{}", uuid::Uuid::new_v4().simple()));
            let root = std::env::temp_dir().join(format!("lw worker {}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&root).unwrap();
            let script = "import os,time; open('marker','a').write(os.environ['AUDIT_MARKER']+'\\n'); time.sleep(0.6)";
            let marker = "$HOME; literal value";
            start_supervised(
                &unit.0,
                "demo",
                &root,
                Path::new("/usr/bin/python3"),
                &["-c".into(), script.into()],
                "",
                &BTreeMap::from([("AUDIT_MARKER".into(), marker.into())]),
            )
            .unwrap();
            let file = root.join("marker");
            for _ in 0..200 {
                if std::fs::read_to_string(&file).is_ok_and(|value| value.lines().count() >= 2) {
                    break;
                }
                thread::sleep(Duration::from_millis(20));
            }
            stop(&unit.0).unwrap();
            let value = std::fs::read_to_string(file).unwrap();
            assert!(
                value.lines().count() >= 2,
                "worker did not restart after exit 0"
            );
            assert!(value.lines().all(|line| line == marker));
            std::fs::remove_dir_all(root).unwrap();
        }

        #[test]
        fn supervisor_arguments_never_pass_through_a_shell() {
            let arguments = supervisor_systemd_args(
                "localweb-project-demo-supervisor",
                "demo",
                Path::new("/tmp/demo project"),
                Path::new("/usr/bin/php8.5"),
                &["artisan".into(), "queue:work; touch /tmp/unsafe".into()],
                "/usr/lib/localweb/runtime/lib",
                &BTreeMap::from([("AUDIT_MARKER".into(), "$HOME; literal".into())]),
            );
            assert!(!arguments.iter().any(|argument| argument == "sh"));
            assert!(!arguments.iter().any(|argument| argument == "-c"));
            assert!(
                arguments
                    .iter()
                    .any(|argument| argument == "--setenv=AUDIT_MARKER=$HOME; literal")
            );
            assert!(
                arguments
                    .iter()
                    .any(|argument| argument == "--expand-environment=no")
            );
            assert!(
                arguments
                    .iter()
                    .any(|argument| argument == "queue:work; touch /tmp/unsafe")
            );
            assert!(
                arguments
                    .iter()
                    .any(|argument| argument == "--property=Restart=always")
            );
            assert!(
                arguments
                    .iter()
                    .any(|argument| argument == "--property=StartLimitBurst=5")
            );
            assert!(
                arguments
                    .iter()
                    .any(|argument| argument == "--working-directory=/tmp/demo project")
            );
        }
    }
}

#[cfg(windows)]
mod windows {
    use super::{BTreeMap, LocalWebError, Path, Result, RuntimeLayout, identity::ProcessIdentity};
    use localweb_core::Paths;
    use std::{
        ffi::OsString,
        fs,
        os::windows::process::CommandExt as _,
        path::PathBuf,
        process::{Command, Stdio},
        sync::{
            Arc, Mutex, OnceLock,
            atomic::{AtomicBool, Ordering},
        },
        thread,
        time::Duration,
    };

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    fn supervisors() -> &'static Mutex<BTreeMap<String, Arc<AtomicBool>>> {
        static SUPERVISORS: OnceLock<Mutex<BTreeMap<String, Arc<AtomicBool>>>> = OnceLock::new();
        SUPERVISORS.get_or_init(|| Mutex::new(BTreeMap::new()))
    }

    fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
        mutex
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(super) fn start_or_restart(unit: &str, executable: &Path, args: &[&str]) -> Result<()> {
        stop(unit)?;
        let arguments: Vec<OsString> = args.iter().map(OsString::from).collect();
        let log = log_path(unit)?;
        let mut child = spawn_child(executable, &arguments, None, &log, &BTreeMap::new())?;
        if let Err(error) = write_pid(unit, child.id()) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }

    pub(super) fn start_supervised(
        unit: &str,
        working_directory: &Path,
        executable: &Path,
        worker_args: &[String],
        _library_path: &str,
        environment: &BTreeMap<String, String>,
    ) -> Result<()> {
        stop(unit)?;
        let environment = environment.clone();
        let executable = executable.to_path_buf();
        let arguments: Vec<OsString> = worker_args.iter().map(OsString::from).collect();
        let working = working_directory.to_path_buf();
        let cancel = Arc::new(AtomicBool::new(false));
        let unit_owned = unit.to_owned();
        let log = log_path(unit)?;
        lock(supervisors()).insert(unit.to_owned(), Arc::clone(&cancel));
        thread::spawn(move || {
            while !cancel.load(Ordering::SeqCst) {
                let child = {
                    // Stop cannot cancel the worker between the ownership check
                    // and recording the newly spawned child.
                    let owners = lock(supervisors());
                    if !owners
                        .get(&unit_owned)
                        .is_some_and(|owner| Arc::ptr_eq(owner, &cancel))
                        || cancel.load(Ordering::SeqCst)
                    {
                        break;
                    }
                    match spawn_child(&executable, &arguments, Some(&working), &log, &environment) {
                        Ok(mut child) => {
                            if write_pid(&unit_owned, child.id()).is_err() {
                                let _ = child.kill();
                                let _ = child.wait();
                                None
                            } else {
                                Some(child)
                            }
                        }
                        Err(_) => None,
                    }
                };
                if let Some(mut child) = child {
                    let _ = child.wait();
                }
                if cancel.load(Ordering::SeqCst) {
                    break;
                }
                thread::sleep(Duration::from_secs(2));
            }
        });
        wait_active(unit, "background worker")
    }

    pub(super) fn stop(unit: &str) -> Result<()> {
        if let Some(cancel) = lock(supervisors()).remove(unit) {
            cancel.store(true, Ordering::SeqCst);
        }
        let path = pid_path(unit)?;
        let Ok(contents) = fs::read_to_string(&path) else {
            return Ok(());
        };
        let identity = ProcessIdentity::parse(&contents).ok_or_else(|| LocalWebError::Service(
            format!("Cannot verify service ownership in {}. Stop older LocalWeb services before restarting.", path.display())
        ))?;
        let Some(_owned_process) = identity.hold() else {
            let _ = fs::remove_file(&path);
            return Ok(());
        };
        let output = localweb_core::process::terminate_tree(identity.pid)?;
        for _ in 0..100 {
            if !identity.is_current() {
                let _ = fs::remove_file(&path);
                return Ok(());
            }
            thread::sleep(Duration::from_millis(20));
        }
        Err(LocalWebError::Service(format!(
            "{unit} did not stop: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }

    pub(super) fn is_active(unit: &str) -> bool {
        pid_path(unit)
            .ok()
            .and_then(|path| ProcessIdentity::load(&path))
            .is_some_and(ProcessIdentity::is_current)
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

    fn spawn_child(
        executable: &Path,
        args: &[OsString],
        working_directory: Option<&Path>,
        log_path: &Path,
        environment: &BTreeMap<String, String>,
    ) -> Result<std::process::Child> {
        let log = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)?;
        let error_log = log.try_clone()?;
        let mut command = Command::new(executable);
        command
            .args(args)
            .envs(environment)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(error_log))
            .env("PATH", windows_path())
            .creation_flags(CREATE_NO_WINDOW);
        if let Some(working_directory) = working_directory {
            command.current_dir(working_directory);
        }
        if let Some(value) = std::env::var_os("LOCALWEB_RUNTIME_ROOT") {
            command.env("LOCALWEB_RUNTIME_ROOT", value);
        }
        command.spawn().map_err(|error| {
            LocalWebError::Service(format!("cannot start {}: {error}", executable.display()))
        })
    }

    fn write_pid(unit: &str, pid: u32) -> Result<()> {
        let path = pid_path(unit)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        ProcessIdentity::save(&path, pid)
    }

    fn pid_path(unit: &str) -> Result<PathBuf> {
        Ok(Paths::discover()?
            .runtime
            .join("services")
            .join(format!("{unit}.pid")))
    }

    fn log_path(unit: &str) -> Result<PathBuf> {
        let logs = Paths::discover()?.state.join("logs");
        fs::create_dir_all(&logs)?;
        Ok(logs.join(format!("{unit}.log")))
    }

    fn windows_path() -> String {
        let inherited = std::env::var("PATH").unwrap_or_default();
        let libraries = RuntimeLayout::discover().library_path();
        if libraries.is_empty() {
            inherited
        } else {
            format!("{libraries};{inherited}")
        }
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{BTreeMap, LocalWebError, Path, Result, RuntimeLayout, identity::ProcessIdentity};
    use localweb_core::{Paths, service::GATEWAY_LABEL};
    use std::{
        ffi::OsString,
        fs,
        os::unix::{fs::OpenOptionsExt as _, process::CommandExt as _},
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
        spawn(unit, executable, &arguments, None, false, &BTreeMap::new())
    }

    pub(super) fn start_supervised(
        unit: &str,
        working_directory: &Path,
        executable: &Path,
        worker_args: &[String],
        environment: &BTreeMap<String, String>,
    ) -> Result<()> {
        stop(unit)?;
        let arguments: Vec<OsString> = worker_args.iter().map(OsString::from).collect();
        spawn(
            unit,
            executable,
            &arguments,
            Some(working_directory),
            true,
            environment,
        )?;
        wait_active(unit, "background worker")
    }

    pub(super) fn stop(unit: &str) -> Result<()> {
        if unit == "localweb-gateway" {
            return stop_gateway();
        }
        let path = pid_path(unit)?;
        let Ok(contents) = fs::read_to_string(&path) else {
            return Ok(());
        };
        let identity = ProcessIdentity::parse(&contents).ok_or_else(|| LocalWebError::Service(
            format!("Cannot verify service ownership in {}. Stop older LocalWeb services before restarting.", path.display())
        ))?;
        signal(identity, libc::SIGTERM)?;
        for _ in 0..200 {
            if !identity.is_current() {
                let _ = fs::remove_file(&path);
                return Ok(());
            }
            thread::sleep(Duration::from_millis(50));
        }
        signal(identity, libc::SIGKILL)?;
        for _ in 0..100 {
            if !identity.is_current() {
                let _ = fs::remove_file(&path);
                return Ok(());
            }
            thread::sleep(Duration::from_millis(20));
        }
        Err(LocalWebError::Service(format!("{unit} did not stop")))
    }

    pub(super) fn is_active(unit: &str) -> bool {
        if unit == "localweb-gateway" {
            return gateway_loaded();
        }
        pid_path(unit)
            .ok()
            .and_then(|path| ProcessIdentity::load(&path))
            .is_some_and(ProcessIdentity::is_current)
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
        environment: &BTreeMap<String, String>,
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
            .process_group(0)
            .envs(environment)
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
        if let Err(error) = ProcessIdentity::save(&services.join(format!("{unit}.pid")), pid) {
            if let Ok(group) = i32::try_from(pid) {
                // SAFETY: this unreaped child owns the group created above.
                unsafe { libc::kill(-group, libc::SIGKILL) };
            }
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
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

    fn signal(identity: ProcessIdentity, signal: i32) -> Result<()> {
        if !identity.is_current() {
            return Ok(());
        }
        let pid = i32::try_from(identity.pid)
            .map_err(|_| LocalWebError::Service("Invalid service process ID".into()))?;
        // SAFETY: getpgid only queries the positive, creation-time-checked PID.
        if unsafe { libc::getpgid(pid) } != pid {
            if !identity.is_current() {
                return Ok(());
            }
            return Err(LocalWebError::Service(
                "Cannot verify the service process group".into(),
            ));
        }
        // SAFETY: this verified process owns the group created by spawn.
        if unsafe { libc::kill(-pid, signal) } != 0 {
            let error = std::io::Error::last_os_error();
            if identity.is_current() {
                return Err(error.into());
            }
        }
        Ok(())
    }

    fn macos_path() -> String {
        let inherited = std::env::var("PATH").unwrap_or_default();
        format!("/opt/homebrew/bin:/opt/homebrew/sbin:/usr/local/bin:/usr/local/sbin:{inherited}")
    }
}
