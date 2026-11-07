//! Process identity for native backends that cannot rely on systemd units.

use localweb_core::{LocalWebError, Result, atomic_write};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ProcessIdentity {
    pub pid: u32,
    started: u64,
}

impl ProcessIdentity {
    pub fn capture(pid: u32) -> Option<Self> {
        if pid <= 1 {
            return None;
        }
        Some(Self {
            pid,
            started: process_started(pid)?,
        })
    }

    pub fn parse(text: &str) -> Option<Self> {
        let identity: Self = serde_json::from_str(text).ok()?;
        (identity.pid > 1 && identity.started > 0).then_some(identity)
    }

    pub fn is_current(self) -> bool {
        Self::capture(self.pid) == Some(self)
    }

    pub fn save(path: &Path, pid: u32) -> Result<()> {
        let identity = Self::capture(pid).ok_or_else(|| {
            LocalWebError::Service("service exited before its identity could be recorded".into())
        })?;
        let data = serde_json::to_vec(&identity)
            .map_err(|error| LocalWebError::Config(error.to_string()))?;
        atomic_write(path, &data, 0o600)
    }

    pub fn load(path: &Path) -> Option<Self> {
        Self::parse(&fs::read_to_string(path).ok()?)
    }

    #[cfg(windows)]
    pub fn hold(self) -> Option<std::os::windows::io::OwnedHandle> {
        let handle = open_process(self.pid)?;
        (started_for_handle(&handle) == Some(self.started)).then_some(handle)
    }
}

#[cfg(windows)]
fn open_process(pid: u32) -> Option<std::os::windows::io::OwnedHandle> {
    use std::os::windows::io::FromRawHandle as _;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    };
    // SAFETY: querying a PID requires no borrowed memory. A successful handle
    // is transferred exactly once to OwnedHandle so it is always closed.
    let handle = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            0,
            pid,
        )
    };
    if handle.is_null() {
        return None;
    }
    // SAFETY: OpenProcess returned a valid, uniquely owned process handle.
    Some(unsafe { std::os::windows::io::OwnedHandle::from_raw_handle(handle) })
}

#[cfg(windows)]
fn started_for_handle(handle: &std::os::windows::io::OwnedHandle) -> Option<u64> {
    use std::os::windows::io::AsRawHandle as _;
    use windows_sys::Win32::{
        Foundation::{FILETIME, WAIT_TIMEOUT},
        System::Threading::{GetProcessTimes, WaitForSingleObject},
    };
    let raw = handle.as_raw_handle();
    // SAFETY: the owned process handle remains valid throughout these calls.
    if unsafe { WaitForSingleObject(raw, 0) } != WAIT_TIMEOUT {
        return None;
    }
    let mut created = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut exited = created;
    let mut kernel = created;
    let mut user = created;
    // SAFETY: all four output pointers address initialized FILETIME values.
    if unsafe {
        GetProcessTimes(
            raw,
            &raw mut created,
            &raw mut exited,
            &raw mut kernel,
            &raw mut user,
        )
    } == 0
    {
        return None;
    }
    Some((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
}

#[cfg(windows)]
fn process_started(pid: u32) -> Option<u64> {
    started_for_handle(&open_process(pid)?)
}

#[cfg(target_os = "macos")]
fn process_started(pid: u32) -> Option<u64> {
    #[link(name = "proc")]
    unsafe extern "C" {}
    let pid = i32::try_from(pid).ok()?;
    let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
    let size = i32::try_from(std::mem::size_of::<libc::proc_bsdinfo>()).ok()?;
    // SAFETY: the buffer has exactly the size required by PROC_PIDTBSDINFO.
    let read = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    if read != size {
        return None;
    }
    // SAFETY: proc_pidinfo reported that it initialized the complete buffer.
    let info = unsafe { info.assume_init() };
    // Darwin's SZOMB process status is 5; zombies are already stopped.
    if info.pbi_status == 5 {
        return None;
    }
    info.pbi_start_tvsec
        .checked_mul(1_000_000)?
        .checked_add(info.pbi_start_tvusec)
}

#[cfg(all(test, target_os = "linux"))]
fn process_started(pid: u32) -> Option<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let (_, fields) = stat.rsplit_once(") ")?;
    if fields.starts_with('Z') {
        return None;
    }
    fields.split_whitespace().nth(19)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_legacy_and_process_group_ids_are_rejected() {
        for text in [
            "1",
            "12345",
            "garbage",
            r#"{"pid":0,"started":1}"#,
            r#"{"pid":-1,"started":1}"#,
            r#"{"pid":42,"started":0}"#,
        ] {
            assert!(ProcessIdentity::parse(text).is_none(), "{text}");
        }
    }

    #[test]
    fn pid_reuse_does_not_match_a_different_creation_time() {
        let identity = ProcessIdentity::capture(std::process::id()).unwrap();
        assert!(identity.is_current());
        let stale = ProcessIdentity {
            started: identity.started + 1,
            ..identity
        };
        assert!(!stale.is_current());
        let path = std::env::temp_dir().join(format!("lw-identity-{}", uuid::Uuid::new_v4()));
        ProcessIdentity::save(&path, identity.pid).unwrap();
        assert_eq!(ProcessIdentity::load(&path), Some(identity));
        fs::remove_file(path).unwrap();
        #[cfg(windows)]
        {
            assert!(identity.hold().is_some());
            assert!(stale.hold().is_none());
        }
    }
}
