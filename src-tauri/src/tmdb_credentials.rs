//! Credentials never cross IPC. Windows owns persistence and supplies the masked input dialog.
use crate::db::AppResult;

#[cfg(windows)]
mod windows {
    use super::*;
    use windows_sys::Win32::Security::Credentials::*;
    use zeroize::{Zeroize, Zeroizing};
    const TARGET: &str = "M2Shelf/TMDB/movie-api";
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }
    pub fn read() -> AppResult<Zeroizing<String>> {
        let target = wide(TARGET);
        let mut pointer = std::ptr::null_mut();
        if unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut pointer) } == 0 {
            return Err("TMDB_CREDENTIALS_REQUIRED".into());
        }
        let result = unsafe {
            let credential = &mut *pointer;
            if credential.CredentialBlobSize > 2048 || credential.CredentialBlob.is_null() {
                Err("TMDB_CREDENTIALS_INVALID".into())
            } else {
                let blob = std::slice::from_raw_parts_mut(
                    credential.CredentialBlob,
                    credential.CredentialBlobSize as usize,
                );
                let result = std::str::from_utf8(blob)
                    .map(|s| Zeroizing::new(s.to_owned()))
                    .map_err(|_| "TMDB_CREDENTIALS_INVALID".into());
                blob.zeroize();
                result
            }
        };
        unsafe { CredFree(pointer.cast()) };
        result
    }
    pub fn configure(parent: usize, title: &str, message: &str) -> AppResult<bool> {
        if title.chars().count() > 120 || message.chars().count() > 1000 {
            return Err("TMDB_DIALOG_INVALID".into());
        }
        let target = wide(TARGET);
        let title = wide(title);
        let message = wide(message);
        let info = CREDUI_INFOW {
            cbSize: std::mem::size_of::<CREDUI_INFOW>() as u32,
            hwndParent: parent as _,
            pszCaptionText: title.as_ptr(),
            pszMessageText: message.as_ptr(),
            ..Default::default()
        };
        let mut username = [0_u16; 514];
        username[..4].copy_from_slice(&[84, 77, 68, 66]);
        let mut password = Zeroizing::new([0_u16; 1025]);
        let mut save = 0;
        let code = unsafe {
            CredUIPromptForCredentialsW(
                &info,
                target.as_ptr(),
                std::ptr::null(),
                0,
                username.as_mut_ptr(),
                username.len() as u32,
                password.as_mut_ptr(),
                password.len() as u32,
                &mut save,
                CREDUI_FLAGS_GENERIC_CREDENTIALS
                    | CREDUI_FLAGS_ALWAYS_SHOW_UI
                    | CREDUI_FLAGS_DO_NOT_PERSIST
                    | CREDUI_FLAGS_KEEP_USERNAME,
            )
        };
        if code == 1223 {
            return Ok(false);
        }
        if code != 0 {
            return Err("TMDB_CREDENTIAL_DIALOG_FAILED".into());
        }
        let end = password
            .iter()
            .position(|c| *c == 0)
            .unwrap_or(password.len());
        let mut token = Zeroizing::new(
            String::from_utf16(&password[..end]).map_err(|_| "TMDB_CREDENTIALS_INVALID")?,
        );
        let value = token.trim();
        if value.is_empty() || value.len() > 2048 || !value.bytes().all(|b| b.is_ascii_graphic()) {
            return Err("TMDB_CREDENTIALS_INVALID".into());
        }
        let mut bytes = Zeroizing::new(value.as_bytes().to_vec());
        let credential = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: target.as_ptr() as _,
            CredentialBlobSize: bytes.len() as u32,
            CredentialBlob: bytes.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            UserName: username.as_mut_ptr(),
            ..Default::default()
        };
        let ok = unsafe { CredWriteW(&credential, 0) } != 0;
        token.zeroize();
        if ok {
            Ok(true)
        } else {
            Err("TMDB_CREDENTIAL_SAVE_FAILED".into())
        }
    }
}
#[cfg(windows)]
pub use windows::{configure, read};
#[cfg(not(windows))]
pub fn read() -> AppResult<zeroize::Zeroizing<String>> {
    Err("TMDB_NATIVE_CREDENTIALS_UNAVAILABLE".into())
}
#[cfg(not(windows))]
pub fn configure(_: usize, _: &str, _: &str) -> AppResult<bool> {
    Err("TMDB_NATIVE_CREDENTIALS_UNAVAILABLE".into())
}
