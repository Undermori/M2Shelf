//! Bounded user-requested Markdown export. No library or screenshot source writes.
use crate::{
    cache,
    collection_backup::{plain, replace},
    db::{AppResult, Database},
};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

pub fn save(db: &Database, app: &Path, destination: &Path, markdown: &str) -> AppResult<()> {
    if markdown.len() > 16 * 1024 * 1024 || markdown.contains('\0') {
        return Err("NOTE_LIMIT".into());
    }
    if destination
        .extension()
        .and_then(|s| s.to_str())
        .is_none_or(|s| !s.eq_ignore_ascii_case("md"))
    {
        return Err("BACKUP_INVALID".into());
    }
    let parent = destination.parent().ok_or("BACKUP_UNSAFE_PATH")?;
    plain(parent, true)?;
    let mut protected = db.library_paths()?;
    protected.push(app.to_path_buf());
    let c = db.connect()?;
    let mut statement = c
        .prepare("SELECT path FROM screenshot_folders")
        .map_err(|_| "BACKUP_DATABASE")?;
    let folders = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|_| "BACKUP_DATABASE")?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|_| "BACKUP_DATABASE")?;
    protected.extend(folders.into_iter().map(PathBuf::from));
    let mut projects = c
        .prepare("SELECT path FROM workspace_folders")
        .map_err(|_| "BACKUP_DATABASE")?;
    let project_roots = projects
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|_| "BACKUP_DATABASE")?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|_| "BACKUP_DATABASE")?;
    protected.extend(project_roots.into_iter().map(PathBuf::from));
    for root in &protected {
        if cache::paths_overlap_checked(destination, root)? {
            return Err("BACKUP_UNSAFE_PATH".into());
        }
    }
    if destination.exists() {
        plain(destination, false)?;
    }
    let temporary = parent.join(format!(".m2shelf-note-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_| "BACKUP_IO")?;
        file.write_all(markdown.as_bytes())
            .map_err(|_| "BACKUP_IO")?;
        file.sync_all().map_err(|_| "BACKUP_IO")?;
        drop(file);
        plain(parent, true)?;
        replace(&temporary, destination)
    })();
    if temporary.exists() && plain(&temporary, false).is_ok() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn markdown_export_is_atomic_and_rejects_protected_roots_and_non_markdown() {
        let temp = tempfile::tempdir().unwrap();
        let app = temp.path().join("app");
        let media = temp.path().join("media");
        let shots = temp.path().join("shots");
        let projects = temp.path().join("projects");
        for path in [&app, &media, &shots, &projects] {
            fs::create_dir(path).unwrap();
        }
        let db = Database::new(app.join("test.db"));
        db.migrate().unwrap();
        db.connect()
            .unwrap()
            .execute(
                "INSERT INTO library_roots(path,display_name) VALUES(?1,'Synthetic')",
                [media.to_str().unwrap()],
            )
            .unwrap();
        db.connect()
            .unwrap()
            .execute(
                "INSERT INTO screenshot_folders(path,alias) VALUES(?1,'Synthetic')",
                [shots.to_str().unwrap()],
            )
            .unwrap();
        let dest = temp.path().join("notes.md");
        db.connect()
            .unwrap()
            .execute(
                "INSERT INTO workspace_folders(path,alias) VALUES(?1,'Synthetic project')",
                [projects.to_str().unwrap()],
            )
            .unwrap();
        save(&db, &app, &dest, "# Title\n\n日本語 한국어 中文").unwrap();
        save(&db, &app, &dest, "# Updated").unwrap();
        assert_eq!(fs::read_to_string(&dest).unwrap(), "# Updated");
        for path in [&app, &media, &shots, &projects] {
            assert!(save(&db, &app, &path.join("notes.md"), "blocked").is_err());
            assert!(!path.join("notes.md").exists());
        }
        assert!(save(&db, &app, &temp.path().join("notes.exe"), "blocked").is_err());
        assert!(save(&db, &app, &dest, "bad\0text").is_err());
        assert_eq!(fs::read_to_string(&dest).unwrap(), "# Updated");
    }
}
