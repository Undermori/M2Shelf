//! Credentials never cross IPC. Windows owns persistence and supplies the masked input dialog.
use crate::db::AppResult;
use rusqlite::OptionalExtension;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Mutex,
};
include!(concat!(env!("OUT_DIR"), "/tmdb_default.rs"));
static GENERATION: AtomicU64 = AtomicU64::new(0);
pub fn generation() -> u64 {
    GENERATION.load(Ordering::SeqCst)
}
static PERSONAL: AtomicBool = AtomicBool::new(false);
static VERIFIED: Mutex<Option<(bool, &'static str)>> = Mutex::new(None);

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialStatus {
    pub source: &'static str,
    pub default_available: bool,
    pub personal_configured: bool,
    pub check: &'static str,
}
pub fn initialize(db: &crate::db::Database) -> AppResult<()> {
    let saved: Option<String> = db
        .connect()?
        .query_row(
            "SELECT value FROM settings WHERE key='tmdb_credential_source'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(|_| "TMDB_SETTINGS_FAILED")?;
    PERSONAL.store(
        saved
            .as_deref()
            .map_or_else(|| read().is_ok(), |v| v == "PERSONAL"),
        Ordering::SeqCst,
    );
    Ok(())
}
pub fn select(db: &crate::db::Database, source: &str) -> AppResult<()> {
    if !matches!(source, "DEFAULT" | "PERSONAL") {
        return Err("TMDB_SOURCE_INVALID".into());
    }
    db.connect()?.execute("INSERT INTO settings(key,value) VALUES('tmdb_credential_source',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [source]).map_err(|_| "TMDB_SETTINGS_FAILED")?;
    GENERATION.fetch_add(1, Ordering::SeqCst);
    crate::tmdb::cancel_mutation();
    PERSONAL.store(source == "PERSONAL", Ordering::SeqCst);
    *VERIFIED.lock().map_err(|_| "TMDB_SETTINGS_FAILED")? = None;
    crate::tmdb::invalidate_search_cache();
    Ok(())
}
pub fn selected() -> AppResult<zeroize::Zeroizing<String>> {
    if PERSONAL.load(Ordering::SeqCst) {
        read()
    } else if DEFAULT_TOKEN.is_empty() {
        Err("TMDB_DEFAULT_UNAVAILABLE".into())
    } else {
        Ok(zeroize::Zeroizing::new(DEFAULT_TOKEN.into()))
    }
}
pub fn status() -> CredentialStatus {
    let personal = PERSONAL.load(Ordering::SeqCst);
    CredentialStatus {
        source: if personal { "PERSONAL" } else { "DEFAULT" },
        default_available: !DEFAULT_TOKEN.is_empty(),
        personal_configured: read().is_ok(),
        check: VERIFIED
            .lock()
            .ok()
            .and_then(|v| *v)
            .filter(|v| v.0 == personal)
            .map_or("UNCHECKED", |v| v.1),
    }
}
pub fn verify() -> AppResult<CredentialStatus> {
    let personal = PERSONAL.load(Ordering::SeqCst);
    let generation = generation();
    let result = crate::tmdb::check_credentials();
    let check = match &result {
        Ok(()) => "VERIFIED",
        Err(e) if e.contains("401") || e.contains("403") => "AUTH_ERROR",
        Err(e) if e.contains("429") => "RATE_LIMITED",
        Err(_) => "NETWORK_ERROR",
    };
    if self::generation() == generation && PERSONAL.load(Ordering::SeqCst) == personal {
        *VERIFIED.lock().map_err(|_| "TMDB_SETTINGS_FAILED")? = Some((personal, check));
    }
    Ok(status())
}

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
    pub fn delete(parent: usize, title: &str, message: &str) -> AppResult<bool> {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            MessageBoxW, IDYES, MB_ICONQUESTION, MB_YESNO,
        };
        let title = wide(title);
        let message = wide(message);
        if unsafe {
            MessageBoxW(
                parent as _,
                message.as_ptr(),
                title.as_ptr(),
                MB_YESNO | MB_ICONQUESTION,
            )
        } != IDYES
        {
            return Ok(false);
        }
        let target = wide(TARGET);
        if unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) } == 0
            && unsafe { windows_sys::Win32::Foundation::GetLastError() } != 1168
        {
            return Err("TMDB_CREDENTIAL_DELETE_FAILED".into());
        }
        Ok(true)
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
pub use windows::{configure, delete, read};
#[cfg(not(windows))]
pub fn delete(_: usize, _: &str, _: &str) -> AppResult<bool> {
    Err("TMDB_NATIVE_CREDENTIALS_UNAVAILABLE".into())
}
#[cfg(not(windows))]
pub fn read() -> AppResult<zeroize::Zeroizing<String>> {
    Err("TMDB_NATIVE_CREDENTIALS_UNAVAILABLE".into())
}
#[cfg(not(windows))]
pub fn configure(_: usize, _: &str, _: &str) -> AppResult<bool> {
    Err("TMDB_NATIVE_CREDENTIALS_UNAVAILABLE".into())
}
