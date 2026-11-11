use crate::db::{AppResult, Database};
use std::{fs, path::Path};
pub(crate) fn plain_directory(path: &Path) -> AppResult<std::path::PathBuf> {
    if !path.is_absolute() {
        return Err("LIBRARY_PATH_INVALID".into());
    }
    for part in path.ancestors() {
        let m = fs::symlink_metadata(part).map_err(|_| "LIBRARY_PATH_INVALID")?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if m.file_attributes() & 0x400 != 0 {
                return Err("LIBRARY_PATH_INVALID".into());
            }
        }
        if m.file_type().is_symlink() {
            return Err("LIBRARY_PATH_INVALID".into());
        }
    }
    let canonical = fs::canonicalize(path).map_err(|_| "LIBRARY_PATH_INVALID")?;
    if !canonical.is_dir() {
        return Err("LIBRARY_PATH_INVALID".into());
    }
    #[cfg(windows)]
    for name in ["WINDIR", "ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(p) = std::env::var_os(name) {
            if let Ok(blocked) = fs::canonicalize(p) {
                if canonical.starts_with(blocked) {
                    return Err("LIBRARY_PATH_INVALID".into());
                }
            }
        }
    }
    fs::read_dir(&canonical)
        .map_err(|_| "LIBRARY_PATH_INVALID")?
        .next()
        .transpose()
        .map_err(|_| "LIBRARY_PATH_INVALID")?;
    Ok(canonical)
}
pub fn validate(db: &Database, path: &Path) -> AppResult<String> {
    let p = plain_directory(path)?;
    db.validate_new_root_path(&p)
        .map(|p| crate::db::display_path(&p))
}
