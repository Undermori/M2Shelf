#[cfg(windows)]
use std::{ffi::OsStr, os::windows::ffi::OsStrExt, ptr};

#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE},
    System::Threading::CreateMutexW,
    UI::WindowsAndMessaging::{FindWindowW, IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE},
};

#[cfg(windows)]
pub(crate) struct SingleInstanceGuard(HANDLE);

#[cfg(windows)]
impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the handle was returned by CreateMutexW and is owned by this guard.
            unsafe { CloseHandle(self.0) };
        }
    }
}

#[cfg(windows)]
pub(crate) fn acquire() -> Result<Option<SingleInstanceGuard>, String> {
    let name = wide("Local\\M2Shelf-app.morimediashelf.desktop-v1");
    // SAFETY: the pointer is NUL terminated and remains valid for this call.
    let handle = unsafe { CreateMutexW(ptr::null(), 0, name.as_ptr()) };
    if handle.is_null() {
        return Err(format!(
            "无法创建 M²Shelf 单实例锁：{}",
            std::io::Error::last_os_error()
        ));
    }
    // SAFETY: GetLastError is read immediately after CreateMutexW as required by Win32.
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        // SAFETY: this process owns the returned handle even when the mutex already exists.
        unsafe { CloseHandle(handle) };
        Ok(None)
    } else {
        Ok(Some(SingleInstanceGuard(handle)))
    }
}

#[cfg(windows)]
pub(crate) fn focus_existing_instance() {
    let title = wide("M²Shelf");
    // SAFETY: the title is NUL terminated. We only pass the returned HWND back to Win32.
    unsafe {
        let window = FindWindowW(ptr::null(), title.as_ptr());
        if !window.is_null() {
            if IsIconic(window) != 0 {
                ShowWindow(window, SW_RESTORE);
            }
            SetForegroundWindow(window);
        }
    }
}

#[cfg(windows)]
fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(Some(0)).collect()
}

#[cfg(not(windows))]
pub(crate) struct SingleInstanceGuard;

#[cfg(not(windows))]
pub(crate) fn acquire() -> Result<Option<SingleInstanceGuard>, String> {
    Ok(Some(SingleInstanceGuard))
}

#[cfg(not(windows))]
pub(crate) fn focus_existing_instance() {}

/// A collection-restore restart waits for its old process before attempting the ordinary
/// mutex. This flag grants no mutex bypass or database authority.
pub(crate) fn wait_for_collection_restart_parent() -> Result<(), String> {
    let arguments: Vec<_> = std::env::args_os().collect();
    if let Some(parent) = collection_parent(&arguments)? {
        #[cfg(windows)]
        {
            use windows_sys::Win32::{
                Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, WAIT_OBJECT_0},
                System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
            };
            let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, parent) };
            if handle.is_null() {
                if std::io::Error::last_os_error().raw_os_error()
                    == Some(ERROR_INVALID_PARAMETER as i32)
                {
                    return Ok(());
                }
                return Err("BACKUP_RESTART_FAILED".into());
            }
            let result = unsafe { WaitForSingleObject(handle, 15000) };
            unsafe { CloseHandle(handle) };
            if result != WAIT_OBJECT_0 {
                return Err("BACKUP_RESTART_FAILED".into());
            }
        }
        #[cfg(not(windows))]
        {
            let _ = parent;
            return Err("BACKUP_RESTART_FAILED".into());
        }
    }
    Ok(())
}
fn collection_parent(arguments: &[std::ffi::OsString]) -> Result<Option<u32>, String> {
    let mut parent = None;
    let mut args = arguments.iter().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--collection-restore-parent" {
            if parent.is_some() {
                return Err("BACKUP_RESTART_FAILED".into());
            }
            let value = args
                .next()
                .and_then(|v| v.to_str())
                .ok_or("BACKUP_RESTART_FAILED")?;
            if !value.bytes().all(|b| b.is_ascii_digit()) {
                return Err("BACKUP_RESTART_FAILED".into());
            }
            let pid = value.parse::<u32>().map_err(|_| "BACKUP_RESTART_FAILED")?;
            if pid == 0 || pid == std::process::id() {
                return Err("BACKUP_RESTART_FAILED".into());
            }
            parent = Some(pid);
        }
    }
    Ok(parent)
}
pub(crate) fn spawn_collection_restart() -> Result<(), String> {
    let mut command =
        std::process::Command::new(std::env::current_exe().map_err(|_| "BACKUP_RESTART_FAILED")?);
    command.args([
        "--collection-restore-parent",
        &std::process::id().to_string(),
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command.spawn().map_err(|_| "BACKUP_RESTART_FAILED")?;
    Ok(())
}
#[cfg(test)]
mod collection_restart_tests {
    use super::*;
    #[test]
    fn restart_parent_is_bounded_and_never_a_mutex_bypass() {
        let args = |s: &str| {
            s.split_whitespace()
                .map(std::ffi::OsString::from)
                .collect::<Vec<_>>()
        };
        assert_eq!(collection_parent(&args("app")), Ok(None));
        assert_eq!(
            collection_parent(&args("app --collection-restore-parent 42")),
            Ok(Some(42))
        );
        for tail in ["0", "-1", "4294967296", "hello", ""] {
            assert!(
                collection_parent(&args(&format!("app --collection-restore-parent {tail}")))
                    .is_err()
            );
        }
        assert!(collection_parent(&args(
            "app --collection-restore-parent 42 --collection-restore-parent 43"
        ))
        .is_err());
    }
}
