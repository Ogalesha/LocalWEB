//! Restricts daemon pipes to the native Windows account and verifies peers.

use std::{
    ffi::c_void,
    io,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    path::PathBuf,
    ptr,
};
use tokio::net::windows::named_pipe::{
    ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
};
use windows_sys::Win32::{
    Foundation::{ERROR_PIPE_BUSY, HANDLE, LocalFree},
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            SDDL_REVISION_1,
        },
        GetTokenInformation, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
    },
    System::{
        Pipes::GetNamedPipeServerProcessId,
        Threading::{
            GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
        },
    },
};

struct LocalAllocation(*mut c_void);
impl Drop for LocalAllocation {
    fn drop(&mut self) {
        // SAFETY: the pointer was allocated by a Windows conversion API using
        // LocalAlloc and is released exactly once by this owner.
        unsafe { LocalFree(self.0) };
    }
}

fn user_sid(process: HANDLE) -> io::Result<String> {
    let mut token = ptr::null_mut();
    // SAFETY: process is a live/pseudo process handle and token is a valid out-pointer.
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &raw mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: OpenProcessToken returned a uniquely owned valid handle.
    let token = unsafe { OwnedHandle::from_raw_handle(token) };
    let mut required = 0;
    // SAFETY: this initial call only queries the required buffer length.
    let _ = unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            ptr::null_mut(),
            0,
            &raw mut required,
        )
    };
    let size = usize::try_from(required).map_err(io::Error::other)?;
    if !(std::mem::size_of::<TOKEN_USER>()..=65_536).contains(&size) {
        return Err(io::Error::other("invalid Windows token information size"));
    }
    // A word buffer provides the alignment TOKEN_USER requires.
    let mut words = vec![0usize; size.div_ceil(std::mem::size_of::<usize>())];
    // SAFETY: the aligned buffer has at least the byte capacity reported above.
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            words.as_mut_ptr().cast(),
            required,
            &raw mut required,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: GetTokenInformation initialized the complete TOKEN_USER header.
    let user = unsafe { &*words.as_ptr().cast::<TOKEN_USER>() };
    let mut text = ptr::null_mut();
    // SAFETY: the SID remains valid in words and text is a valid out-pointer.
    if unsafe { ConvertSidToStringSidW(user.User.Sid, &raw mut text) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let _allocation = LocalAllocation(text.cast());
    let mut length = 0;
    // SAFETY: the API returned an allocated, null-terminated UTF-16 SID string.
    while unsafe { *text.add(length) } != 0 {
        length += 1;
    }
    // SAFETY: length identifies the initialized string before its terminator.
    String::from_utf16(unsafe { std::slice::from_raw_parts(text, length) })
        .map_err(io::Error::other)
}

fn current_sid() -> io::Result<String> {
    // SAFETY: GetCurrentProcess returns a valid pseudo-handle, which is not closed.
    user_sid(unsafe { GetCurrentProcess() })
}

pub(crate) fn pipe_name() -> io::Result<PathBuf> {
    Ok(PathBuf::from(format!(
        r"\\.\pipe\localweb-{}",
        current_sid()?
    )))
}

pub(crate) fn create_server(name: &str, first: bool) -> io::Result<NamedPipeServer> {
    let sid = current_sid()?;
    let descriptor = format!("D:P(A;;GA;;;SY)(A;;GA;;;{sid})");
    let wide: Vec<_> = descriptor.encode_utf16().chain(Some(0)).collect();
    let mut security = ptr::null_mut();
    // SAFETY: wide is null-terminated and security is a valid out-pointer.
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            wide.as_ptr(),
            SDDL_REVISION_1,
            &raw mut security,
            ptr::null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let _allocation = LocalAllocation(security);
    let mut attributes = SECURITY_ATTRIBUTES {
        nLength: u32::try_from(std::mem::size_of::<SECURITY_ATTRIBUTES>())
            .map_err(io::Error::other)?,
        lpSecurityDescriptor: security,
        bInheritHandle: 0,
    };
    // SAFETY: attributes and its descriptor stay valid throughout this
    // synchronous call; Windows copies the descriptor when creating the pipe.
    unsafe {
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .create_with_security_attributes_raw(name, (&raw mut attributes).cast())
    }
}

fn verify_server(client: &NamedPipeClient, expected_sid: &str) -> io::Result<()> {
    let mut pid = 0;
    // SAFETY: client owns a valid pipe handle and pid is a valid out-pointer.
    if unsafe { GetNamedPipeServerProcessId(client.as_raw_handle(), &raw mut pid) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: this read-only query does not borrow user memory.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: OpenProcess returned a uniquely owned valid process handle.
    let process = unsafe { OwnedHandle::from_raw_handle(process) };
    if user_sid(process.as_raw_handle())? != expected_sid {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "daemon pipe belongs to another Windows account",
        ));
    }
    Ok(())
}

/// Holds the authenticated daemon instance so its PID cannot be reused during
/// a restart. Other executables, even for this account, are never terminated.
pub(crate) fn daemon_process(
    client: &NamedPipeClient,
    expected_executable: &std::path::Path,
) -> io::Result<OwnedHandle> {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt as _};
    use windows_sys::Win32::System::Threading::{
        PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, QueryFullProcessImageNameW,
    };
    let mut pid = 0;
    // SAFETY: client owns the live pipe handle and pid is a valid out-pointer.
    if unsafe { GetNamedPipeServerProcessId(client.as_raw_handle(), &raw mut pid) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: this opens a handle for one PID; no borrowed memory is passed.
    let raw = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE | PROCESS_TERMINATE,
            0,
            pid,
        )
    };
    if raw.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: OpenProcess returned a uniquely owned live process handle.
    let process = unsafe { OwnedHandle::from_raw_handle(raw) };
    if user_sid(process.as_raw_handle())? != current_sid()? {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "daemon belongs to another account",
        ));
    }
    let mut buffer = vec![0u16; 32_768];
    let mut length = u32::try_from(buffer.len()).map_err(io::Error::other)?;
    // SAFETY: process remains owned, and buffer/length are valid writable storage.
    if unsafe {
        QueryFullProcessImageNameW(
            process.as_raw_handle(),
            0,
            buffer.as_mut_ptr(),
            &raw mut length,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let actual = PathBuf::from(OsString::from_wide(
        &buffer[..usize::try_from(length).map_err(io::Error::other)?],
    ));
    if actual.canonicalize()? != expected_executable.canonicalize()? {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "daemon endpoint is owned by another executable",
        ));
    }
    Ok(process)
}

pub(crate) async fn connect(name: &str) -> io::Result<NamedPipeClient> {
    let sid = current_sid()?;
    for attempt in 0..50 {
        // Tokio keeps SECURITY_IDENTIFICATION as its default QoS. Authenticate
        // the server's actual token before sending any request or credentials.
        match ClientOptions::new().open(name) {
            Ok(client) => {
                verify_server(&client, &sid)?;
                return Ok(client);
            }
            Err(error)
                if error.raw_os_error() == i32::try_from(ERROR_PIPE_BUSY).ok() && attempt < 49 =>
            {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("the last attempt returns its result")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_uses_the_native_account_sid() {
        assert!(
            pipe_name()
                .unwrap()
                .to_string_lossy()
                .ends_with(&current_sid().unwrap())
        );
    }

    #[tokio::test]
    async fn first_pipe_instance_excludes_another_owner() {
        let name = format!(r"\\.\pipe\lw-owner-test-{}", uuid::Uuid::new_v4());
        let _owner = create_server(&name, true).unwrap();
        create_server(&name, true).unwrap_err();
    }

    #[tokio::test]
    async fn named_pipe_verifies_peer_and_exchanges_bytes() {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        let name = format!(r"\\.\pipe\lw-security-test-{}", uuid::Uuid::new_v4());
        let server = create_server(&name, true).unwrap();
        let mut client = connect(&name).await.unwrap();
        assert!(verify_server(&client, "S-1-0-0").is_err());
        server.connect().await.unwrap();
        client.write_all(b"ok").await.unwrap();
        let mut server = server;
        let mut result = [0; 2];
        server.read_exact(&mut result).await.unwrap();
        assert_eq!(&result, b"ok");
    }
}
