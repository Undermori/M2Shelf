//! Synthetic current-schema factory. Feature-gated; never used by the desktop app.
use crate::adapter::Result;
use m2shelf_smart_mixed_lab::model::*;
use rusqlite::{params, Connection};
use std::{collections::BTreeMap, path::Path};
pub const ROOT_PATH: &str = "R:\\SyntheticLibrary";
pub const MIGRATIONS: &[&str] = &[
    include_str!("../../../src-tauri/migrations/0001_initial.sql"),
    include_str!("../../../src-tauri/migrations/0002_mvp.sql"),
    include_str!("../../../src-tauri/migrations/0003_resources_and_cover_status.sql"),
    include_str!("../../../src-tauri/migrations/0004_multilingual_metadata.sql"),
    include_str!("../../../src-tauri/migrations/0005_user_tags.sql"),
    include_str!("../../../src-tauri/migrations/0006_watch_history.sql"),
    include_str!("../../../src-tauri/migrations/0007_favorite_folders.sql"),
    include_str!("../../../src-tauri/migrations/0008_bangumi_subject_type.sql"),
    include_str!("../../../src-tauri/migrations/0009_library_recognition_mode.sql"),
    include_str!("../../../src-tauri/migrations/0010_confirmed_title_aliases.sql"),
    include_str!("../../../src-tauri/migrations/0011_incremental_scan.sql"),
    include_str!("../../../src-tauri/migrations/0012_provider_aliases.sql"),
    include_str!("../../../src-tauri/migrations/0013_alias_sync.sql"),
    include_str!("../../../src-tauri/migrations/0014_scan_health.sql"),
    include_str!("../../../src-tauri/migrations/0015_comic_library_kind.sql"),
    include_str!("../../../src-tauri/migrations/0016_comics.sql"),
    include_str!("../../../src-tauri/migrations/0017_comic_binding_types.sql"),
    include_str!("../../../src-tauri/migrations/0018_document_books.sql"),
    include_str!("../../../src-tauri/migrations/0019_ebook_library.sql"),
    include_str!("../../../src-tauri/migrations/0020_book_file_recognition.sql"),
    include_str!("../../../src-tauri/migrations/0021_doujin_and_text_books.sql"),
    include_str!("../../../src-tauri/migrations/0022_poster_cache_failures.sql"),
    include_str!("../../../src-tauri/migrations/0023_readable_resources.sql"),
    include_str!("../../../src-tauri/migrations/0024_artbook_matching_policy.sql"),
];

pub struct Factory {
    pub connection: Connection,
    pub nodes: BTreeMap<String, i64>,
    pub mode: String,
}
impl Factory {
    /// Refuses to overwrite any existing file. Only CLI-chosen private synthetic paths are used.
    pub fn create(path: &Path, kind: LibraryKind, mode: &str) -> Result<Self> { Self::create_internal(path,kind,mode,false) }
    #[cfg(feature="production")]
    pub fn create_smart(path:&Path,kind:LibraryKind)->Result<Self>{Self::create_internal(path,kind,"FOLDER",true)}
    fn create_internal(path:&Path,kind:LibraryKind,mode:&str,smart:bool)->Result<Self>{
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|_| "FIXTURE_DESTINATION_EXISTS_OR_UNWRITABLE")?;
        let c = Connection::open(path).map_err(|_| "FIXTURE_OPEN_FAILED")?;
        c.execute_batch("CREATE TABLE mediashelf_schema_migrations(version INTEGER PRIMARY KEY,applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);").map_err(|e|e.to_string())?;
        for (i, sql) in MIGRATIONS.iter().enumerate() {
            c.execute_batch(sql)
                .map_err(|e| format!("fixture migration {}: {e}", i + 1))?;
            c.execute(
                "INSERT INTO mediashelf_schema_migrations(version) VALUES(?1)",
                [i + 1],
            )
            .map_err(|e| e.to_string())?;
        }
        if smart {c.execute_batch(include_str!("../../../src-tauri/migrations/0025_smart_mixed.sql")).map_err(|e|e.to_string())?;c.execute("INSERT INTO mediashelf_schema_migrations(version) VALUES(25)",[]).map_err(|e|e.to_string())?;c.execute_batch(include_str!("../../../src-tauri/migrations/0026_text_reader_positions.sql")).map_err(|e|e.to_string())?;c.execute("INSERT INTO mediashelf_schema_migrations(version) VALUES(26)",[]).map_err(|e|e.to_string())?;}
        let video = matches!(
            kind,
            LibraryKind::Video | LibraryKind::Animation | LibraryKind::LiveAction
        );
        let scope = match kind {
            LibraryKind::Animation => "ANIMATION",
            LibraryKind::LiveAction => "LIVE_ACTION",
            _ => "MIXED",
        };
        c.execute(&format!("INSERT INTO library_roots(id,path,display_name,media_kind,book_library_kind,doujin_library,artbook_library,recognition_mode,auto_bangumi,video_subject_scope{}) VALUES(1,?1,'Synthetic',?2,?3,?4,?5,?6,0,?7{})",if smart{",book_organization_strategy"}else{""},if smart{",'SMART_MIXED'"}else{""}),params![ROOT_PATH,if video{"VIDEO"}else{"COMIC"},if kind==LibraryKind::Ebook{"EBOOK"}else{"COMIC"},kind==LibraryKind::Doujin,kind==LibraryKind::Artbook,mode,scope]).map_err(|e|e.to_string())?;
        c.execute("INSERT INTO library_scan_health(library_root_id,outcome,error_count,last_success_at) VALUES(1,'SUCCESS',0,'2026-01-01T00:00:00Z')",[]).map_err(|e|e.to_string())?;
        let mut result = Self {
            connection: c,
            nodes: BTreeMap::new(),
            mode: mode.into(),
        };
        result.node("")?;
        Ok(result)
    }
    pub fn absolute(path: &str) -> String {
        if path.is_empty() {
            ROOT_PATH.into()
        } else {
            format!("{}\\{}", ROOT_PATH, path.replace('/', "\\"))
        }
    }
    pub fn node(&mut self, path: &str) -> Result<i64> {
        if let Some(id) = self.nodes.get(path) {
            return Ok(*id);
        }
        let parent = if path.is_empty() {
            None
        } else {
            Some(self.node(path.rsplit_once('/').map_or("", |(p, _)| p))?)
        };
        let name = path.rsplit('/').next().unwrap_or("Synthetic");
        self.connection.execute("INSERT INTO nodes(library_root_id,parent_node_id,absolute_path,folder_name,display_name) VALUES(1,?1,?2,?3,?3)",params![parent,Self::absolute(path),name]).map_err(|e|e.to_string())?;
        let id = self.connection.last_insert_rowid();
        self.nodes.insert(path.into(), id);
        Ok(id)
    }
    pub fn file_book(&mut self, path: &str) -> Result<i64> {
        let format = path
            .rsplit_once('.')
            .map_or("", |(_, s)| s)
            .to_ascii_uppercase();
        if !["PDF", "EPUB", "TXT", "MOBI", "AZW3", "CBZ"].contains(&format.as_str()) {
            return Err("UNSUPPORTED_SYNTHETIC_BOOK".into());
        }
        let id = if self.mode == "VIDEO_FILE" {
            let root = self.nodes[""];
            self.connection.execute("INSERT INTO nodes(library_root_id,parent_node_id,absolute_path,folder_name,display_name) VALUES(1,?1,?2,?3,?3)",params![root,Self::absolute(path),path]).map_err(|e|e.to_string())?;
            let id = self.connection.last_insert_rowid();
            self.nodes.insert(path.into(), id);
            id
        } else {
            self.node(path.rsplit_once('/').map_or("", |(p, _)| p))?
        };
        self.connection.execute("INSERT INTO comic_books(node_id,source_path,source_kind,display_name,revision,modified_at,page_count,file_size,reader_format) VALUES(?1,?2,'ZIP_ARCHIVE',?3,?4,'2026-01-01',1,100,?5)",params![id,Self::absolute(path),path,format!("synthetic-revision:{path}"),if format=="CBZ"{None}else{Some(format.as_str())}]).map_err(|e|e.to_string())?;
        let book = self.connection.last_insert_rowid();
        self.connection.execute("INSERT INTO comic_pages(comic_book_id,page_index,page_name,source_locator,file_size,modified_at) VALUES(?1,0,'synthetic chapter','source-chapter:0',100,'2026-01-01')",[book]).map_err(|e|e.to_string())?;
        Ok(book)
    }
    pub fn image_book(&mut self, path: &str, pages: &[&str]) -> Result<i64> {
        let node = self.node(path)?;
        self.connection.execute("INSERT INTO comic_books(node_id,source_path,source_kind,display_name,revision,modified_at,page_count,file_size) VALUES(?1,?2,'IMAGE_FOLDER',?3,?4,'2026-01-01',?5,100)",params![node,Self::absolute(path),path,format!("synthetic-image-revision:{path}"),pages.len()]).map_err(|e|e.to_string())?;
        let book = self.connection.last_insert_rowid();
        for (i, name) in pages.iter().enumerate() {
            let p = if path.is_empty() {
                name.to_string()
            } else {
                format!("{path}/{name}")
            };
            self.connection.execute("INSERT INTO comic_pages(comic_book_id,page_index,page_name,source_locator,file_size,modified_at) VALUES(?1,?2,?3,?4,100,'2026-01-01')",params![book,i,name,Self::absolute(&p)]).map_err(|e|e.to_string())?;
        }
        Ok(book)
    }
    pub fn resource(&mut self, path: &str) -> Result<i64> {
        let node = self.node(path.rsplit_once('/').map_or("", |(p, _)| p))?;
        let extension = path.rsplit_once('.').map_or("", |(_, s)| s);
        self.connection.execute("INSERT INTO resource_files(node_id,absolute_path,file_name,extension,file_size,modified_at) VALUES(?1,?2,?3,?4,100,'2026-01-01')",params![node,Self::absolute(path),path,extension]).map_err(|e|e.to_string())?;
        Ok(self.connection.last_insert_rowid())
    }
    pub fn bind(&self, node: i64, subject: i64) -> Result<()> {
        self.connection.execute("INSERT INTO metadata_bindings(node_id,provider,provider_subject_id,provider_subject_type,provider_title) VALUES(?1,'BANGUMI',?2,1,'Synthetic title')",params![node,subject]).map_err(|e|e.to_string())?;
        Ok(())
    }
    pub fn manual(&self, node: i64, kind: &str) -> Result<()> {
        self.connection
            .execute(
                "UPDATE nodes SET node_type=?1,manual_type_override=1 WHERE id=?2",
                params![kind, node],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    /// Translate authored Phase 1 evidence to real schema rows. Binary-only evidence and
    /// trusted author/volume hints have no production columns: do not fabricate them.
    pub fn import(&mut self, input: &Snapshot) -> Result<()> {
        for e in input
            .entries
            .iter()
            .filter(|e| e.kind == EntryKind::Directory)
        {
            let n = self.node(&e.path)?;
            if e.state == EntryState::Excluded {
                self.manual(n, "IGNORED")?;
            }
        }
        let ordered: std::collections::BTreeSet<_> = input
            .page_orders
            .iter()
            .flat_map(|o| o.pages.iter())
            .collect();
        for e in input.entries.iter().filter(|e| e.kind == EntryKind::File) {
            if ordered.contains(&e.path) {
                continue;
            }
            if e.format.is_some_and(|f| !f.image())
                && crate::adapter::suffix_format(&e.path).is_some()
                && e.verified
            {
                let id = self.file_book(&e.path)?;
                if e.state != EntryState::Available {
                    self.connection.execute("UPDATE comic_books SET index_error='SYNTHETIC_UNAVAILABLE' WHERE id=?1",[id]).map_err(|e|e.to_string())?;
                }
            } else {
                self.resource(&e.path)?;
            }
        }
        for order in &input.page_orders {
            let names = order
                .pages
                .iter()
                .map(|p| p.rsplit('/').next().unwrap())
                .collect::<Vec<_>>();
            self.image_book(&order.directory, &names)?;
        }
        if !input.complete {
            self.connection
                .execute(
                    "UPDATE library_scan_health SET outcome='PARTIAL',error_count=1",
                    [],
                )
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    pub fn seed_metadata(&self, book: i64) -> Result<()> {
        let node: i64 = self
            .connection
            .query_row("SELECT node_id FROM comic_books WHERE id=?1", [book], |r| {
                r.get(0)
            })
            .map_err(|e| e.to_string())?;
        self.connection
            .execute(
                "INSERT INTO comic_reading_progress(comic_book_id,last_page_index) VALUES(?1,0)",
                [book],
            )
            .map_err(|e| e.to_string())?;
        self.connection
            .execute(
                "INSERT INTO comic_bookmarks(comic_book_id,page_index) VALUES(?1,0)",
                [book],
            )
            .map_err(|e| e.to_string())?;
        self.connection
            .execute("INSERT INTO watch_history(node_id) VALUES(?1)", [node])
            .map_err(|e| e.to_string())?;
        self.connection.execute_batch("INSERT INTO tags(id,name,normalized_name) VALUES(1,'Synthetic tag','synthetic tag'); INSERT INTO favorite_folders(id,name,normalized_name) VALUES(1,'Synthetic favorite','synthetic favorite'); INSERT INTO settings(key,value) VALUES('reader','synthetic');").map_err(|e|e.to_string())?;
        self.connection
            .execute("INSERT INTO node_tags(node_id,tag_id) VALUES(?1,1)", [node])
            .map_err(|e| e.to_string())?;
        self.connection
            .execute(
                "INSERT INTO node_favorite_folders(folder_id,node_id) VALUES(1,?1)",
                [node],
            )
            .map_err(|e| e.to_string())?;
        self.connection.execute("UPDATE nodes SET cover_source='MANUAL',cover_cache_path='synthetic-cache/cover',display_name='Synthetic custom title' WHERE id=?1",[node]).map_err(|e|e.to_string())?;
        self.bind(node, 123)?;
        Ok(())
    }
}
