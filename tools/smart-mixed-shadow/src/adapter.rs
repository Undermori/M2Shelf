use crate::model::*;
use m2shelf_smart_mixed_lab::{model::*, signals::normalize_path};
use rusqlite::{Connection, OpenFlags, OptionalExtension, Row};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

pub type Result<T> = std::result::Result<T, String>;
#[derive(Default)]
pub struct ReadOptions {
    pub expected_version: Option<String>,
    pub overrides: Vec<ManualOverride>,
    pub cancelled: Arc<AtomicBool>,
    #[cfg(feature = "fixtures")]
    pub sql_step_budget: Option<usize>,
}
pub struct ReadIndex {
    connection: Connection,
}
fn sql_error(_: rusqlite::Error) -> String {
    "SQLITE_READ_FAILED_OR_INTERRUPTED".into()
}
fn check(options: &ReadOptions) -> Result<()> {
    if options.cancelled.load(Ordering::Relaxed) {
        Err("CANCELLED_NO_REPORT".into())
    } else {
        Ok(())
    }
}
struct Budget {
    rows: usize,
    bytes: usize,
}
impl Budget {
    fn row(&mut self) -> Result<()> {
        self.rows += 1;
        if self.rows > 2_000_000 {
            Err("INDEX_ROW_LIMIT".into())
        } else {
            Ok(())
        }
    }
    fn text(&mut self, row: &Row<'_>, column: usize) -> Result<String> {
        let value = row
            .get_ref(column)
            .map_err(sql_error)?
            .as_str()
            .map_err(|_| "INVALID_TEXT")?;
        self.bytes += value.len();
        if value.len() > 32_767 || self.bytes > 128 * 1024 * 1024 {
            return Err("INDEX_TEXT_LIMIT".into());
        }
        Ok(value.into())
    }
    fn optional(&mut self, row: &Row<'_>, column: usize) -> Result<Option<String>> {
        if row.get_ref(column).map_err(sql_error)?.data_type() == rusqlite::types::Type::Null {
            Ok(None)
        } else {
            self.text(row, column).map(Some)
        }
    }
}
fn int(row: &Row<'_>, column: usize) -> Result<i64> {
    row.get(column).map_err(sql_error)
}
fn opt_int(row: &Row<'_>, column: usize) -> Result<Option<i64>> {
    row.get(column).map_err(sql_error)
}
fn bump(map: &mut BTreeMap<String, usize>, key: &str) {
    *map.entry(key.into()).or_default() += 1;
}
struct HashWriter(Sha256);
impl Write for HashWriter {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.update(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub(crate) fn digest(value: &impl serde::Serialize) -> Result<String> {
    let mut w = HashWriter(Sha256::new());
    serde_json::to_writer(&mut w, value).map_err(|_| "DIGEST_FAILED")?;
    Ok(format!("{:x}", w.0.finalize()))
}

/// Lexical containment of trusted INDEXED Windows disk/UNC paths. No disk enumeration,
/// canonicalization or opened-handle guarantee is claimed at this read-only layer.
pub fn relative_index_path(root: &str, source: &str) -> Result<String> {
    fn canonical_shape(s: &str) -> Result<String> {
        let mut p = s.replace('\\', "/");
        if let Some(rest) = p.strip_prefix("//?/UNC/") {
            p = format!("//{rest}");
        } else if let Some(rest) = p.strip_prefix("//?/") {
            p = rest.into();
        }
        let p = if p.len() == 3 && p.as_bytes().get(1) == Some(&b':') {
            p
        } else {
            p.trim_end_matches('/').to_string()
        };
        let disk = p.as_bytes().get(1) == Some(&b':')
            && p.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
            && p.as_bytes().get(2) == Some(&b'/');
        let unc = p.starts_with("//") && p[2..].split('/').count() >= 2;
        if !(disk || unc) || p.len() > 32767 {
            return Err("INDEX_PATH_NOT_CANONICAL".into());
        }
        let tail = if disk { &p[3..] } else { &p[2..] };
        if !tail.is_empty() {
            normalize_path(tail).map_err(|_| "INDEX_PATH_NOT_CANONICAL")?;
        }
        Ok(p)
    }
    let root = canonical_shape(root)?;
    let source = canonical_shape(source)?;
    if source.eq_ignore_ascii_case(&root) {
        return Ok(String::new());
    }
    let prefix = if root.ends_with('/') {
        root.clone()
    } else {
        format!("{root}/")
    };
    if source.len() <= prefix.len()
        || !source
            .get(..prefix.len())
            .is_some_and(|v| v.eq_ignore_ascii_case(&prefix))
    {
        return Err("INDEX_PATH_OUTSIDE_ROOT".into());
    }
    normalize_path(&source[prefix.len()..]).map_err(|_| "INDEX_PATH_NOT_CANONICAL".into())
}
pub(crate) fn suffix_format(path: &str) -> Option<Format> {
    Some(
        match path.rsplit_once('.')?.1.to_ascii_lowercase().as_str() {
            "pdf" => Format::Pdf,
            "epub" => Format::Epub,
            "cbz" => Format::Cbz,
            "txt" => Format::Txt,
            "mobi" => Format::Mobi,
            "azw3" => Format::Azw3,
            "jpg" | "jpeg" => Format::Jpeg,
            "png" => Format::Png,
            "webp" => Format::Webp,
            "gif" => Format::Gif,
            "bmp" => Format::Bmp,
            "avif" => Format::Avif,
            _ => return None,
        },
    )
}
fn document_format(value: Option<&str>) -> Option<Format> {
    match value {
        Some("PDF") => Some(Format::Pdf),
        Some("EPUB") => Some(Format::Epub),
        Some("TXT") => Some(Format::Txt),
        Some("MOBI") => Some(Format::Mobi),
        Some("AZW3") => Some(Format::Azw3),
        _ => None,
    }
}
fn entry(path: String, kind: EntryKind) -> Entry {
    Entry {
        path,
        kind,
        state: EntryState::Available,
        format: None,
        verified: false,
        identity: None,
        hint: None,
        metadata: None,
    }
}

impl ReadIndex {
    pub fn open(path: &Path) -> Result<Self> {
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(sql_error)?;
        connection
            .busy_timeout(Duration::from_secs(3))
            .map_err(sql_error)?;
        connection
            .execute_batch("PRAGMA query_only=ON; PRAGMA trusted_schema=OFF;")
            .map_err(sql_error)?;
        Ok(Self { connection })
    }
    pub fn read(&mut self, root_id: i64, options: &ReadOptions) -> Result<IndexSnapshot> {
        self.read_with_pin_hook(root_id, options, || Ok(()))
    }
    /// Isolated harness seam: called after first SELECT pins the deferred transaction.
    /// No desktop command imports this crate or this seam.
    pub fn read_with_pin_hook(
        &mut self,
        root_id: i64,
        options: &ReadOptions,
        pinned: impl FnOnce() -> Result<()>,
    ) -> Result<IndexSnapshot> {
        check(options)?;
        if options.overrides.len() > 200_000 {
            return Err("OVERRIDE_LIMIT".into());
        }
        let cancel = options.cancelled.clone();
        #[cfg(feature = "fixtures")]
        let mut steps = options.sql_step_budget;
        self.connection.progress_handler(
            1000,
            Some(move || {
                #[cfg(feature = "fixtures")]
                if let Some(left) = &mut steps {
                    if *left <= 1000 {
                        return true;
                    }
                    *left -= 1000;
                }
                cancel.load(Ordering::Relaxed)
            }),
        );
        let result = (|| {
            let tx = self.connection.transaction().map_err(sql_error)?;
            let version: i64 = tx.query_row("SELECT CASE WHEN COUNT(*)=MAX(version) AND MIN(version)=1 THEN MAX(version) ELSE -1 END FROM mediashelf_schema_migrations",[],|r|r.get(0)).map_err(sql_error)?;
            if version != 24 && !(cfg!(feature = "production") && (25..=26).contains(&version)) {
                return Err("UNSUPPORTED_SCHEMA_VERSION".into());
            }
            let mut budget = Budget { rows: 0, bytes: 0 };
            let (root_path, media, mode, policy): (String, LibraryKind, String, i64) = {
                let mut stmt = tx.prepare("SELECT path,media_kind,book_library_kind,doujin_library,artbook_library,recognition_mode,auto_bangumi FROM library_roots WHERE id=?1").map_err(sql_error)?;
                let mut rows = stmt.query([root_id]).map_err(sql_error)?;
                let row = rows.next().map_err(sql_error)?.ok_or("ROOT_NOT_FOUND")?;
                if budget.text(row, 1)? != "COMIC" {
                    return Err("OUT_OF_SCOPE_VIDEO_LIBRARY".into());
                }
                let media = if int(row, 4)? == 1 {
                    LibraryKind::Artbook
                } else if int(row, 3)? == 1 {
                    LibraryKind::Doujin
                } else if budget.text(row, 2)? == "EBOOK" {
                    LibraryKind::Ebook
                } else {
                    LibraryKind::Comic
                };
                (
                    budget.text(row, 0)?,
                    media,
                    budget.text(row, 5)?,
                    int(row, 6)?,
                )
            };
            relative_index_path(&root_path, &root_path)?;
            if !matches!(mode.as_str(), "FOLDER" | "VIDEO_FILE") {
                return Err("UNKNOWN_RECOGNITION_MODE".into());
            }
            pinned()?;
            check(options)?;
            let mut scan = tx.query_row("SELECT outcome,error_count,last_success_at,last_auto_attempt_at FROM library_scan_health WHERE library_root_id=?1",[root_id],|r|Ok(ScanHealth {outcome:r.get(0)?,error_count:r.get(1)?,last_success:r.get(2)?,last_attempt:r.get(3)?,latest_run:None,complete:false,freshness:"INDEX_ONLY_CURRENT_DISK_UNKNOWN".into()})).optional().map_err(sql_error)?.unwrap_or(ScanHealth {outcome:"UNKNOWN".into(),error_count:0,last_success:None,last_attempt:None,latest_run:None,complete:false,freshness:"INDEX_ONLY_CURRENT_DISK_UNKNOWN".into()});
            scan.latest_run = tx.query_row("SELECT status,started_at,finished_at,errors FROM scan_runs WHERE root_id=?1 ORDER BY started_at DESC,id DESC LIMIT 1",[root_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(sql_error)?;
            scan.complete = scan.outcome == "SUCCESS"
                && scan.error_count == 0
                && scan
                    .latest_run
                    .as_ref()
                    .is_none_or(|r| r.0 == "COMPLETED" && r.3 == 0);
            let mut diagnostics = BTreeMap::new();
            if !scan.complete {
                bump(&mut diagnostics, "INCOMPLETE_OR_STALE_INDEX_RETAIN_ALL");
            }
            let mut nodes = BTreeMap::new();
            {
                let mut stmt=tx.prepare("SELECT n.id,n.parent_node_id,n.absolute_path,n.node_type,n.manual_type_override,b.provider_subject_id,b.provider_subject_type FROM nodes n LEFT JOIN metadata_bindings b ON b.node_id=n.id AND b.provider='BANGUMI' WHERE n.library_root_id=?1 ORDER BY n.id").map_err(sql_error)?;
                let mut rows = stmt.query([root_id]).map_err(sql_error)?;
                while let Some(r) = rows.next().map_err(sql_error)? {
                    check(options)?;
                    budget.row()?;
                    let id = int(r, 0)?;
                    let absolute = budget.text(r, 2)?;
                    nodes.insert(
                        id,
                        Node {
                            id,
                            parent: opt_int(r, 1)?,
                            path: relative_index_path(&root_path, &absolute)?,
                            node_type: budget.text(r, 3)?,
                            manual: int(r, 4)? != 0,
                            binding: opt_int(r, 5)?.zip(opt_int(r, 6)?),
                        },
                    );
                }
            }
            for n in nodes.values() {
                if n.parent.is_none() && !n.path.is_empty() {
                    return Err("ORPHAN_NODE_NO_REPORT".into());
                }
                let mut next = n.parent;
                let mut seen = BTreeSet::from([n.id]);
                while let Some(id) = next {
                    if !seen.insert(id) || seen.len() > 65 {
                        return Err("NODE_CYCLE_OR_DEPTH_LIMIT".into());
                    }
                    let parent = nodes.get(&id).ok_or("CROSS_ROOT_OR_MISSING_PARENT")?;
                    // Flat mode intentionally has physical grandchildren attached to the hidden Root.
                    if mode == "FOLDER"
                        && !(parent.path.is_empty()
                            || n.path.starts_with(&format!("{}/", parent.path)))
                    {
                        return Err("NODE_PATH_PARENT_CONFLICT".into());
                    }
                    next = parent.parent;
                }
            }
            let mut sources = BTreeMap::new();
            {
                let mut stmt=tx.prepare("SELECT b.id,b.node_id,b.source_resource_id,b.source_path,b.revision,b.source_kind,b.reader_format,b.document_format,b.file_size,b.modified_at,b.source_resource_stamp,b.page_count,b.index_error,b.text_encoding FROM comic_books b JOIN nodes n ON n.id=b.node_id WHERE n.library_root_id=?1 ORDER BY b.id").map_err(sql_error)?;
                let mut rows = stmt.query([root_id]).map_err(sql_error)?;
                while let Some(r) = rows.next().map_err(sql_error)? {
                    check(options)?;
                    budget.row()?;
                    let id = int(r, 0)?;
                    let absolute = budget.text(r, 3)?;
                    sources.insert(
                        id,
                        Source {
                            book_id: id,
                            node_id: int(r, 1)?,
                            resource_id: opt_int(r, 2)?,
                            path: relative_index_path(&root_path, &absolute)?,
                            revision: budget.text(r, 4)?,
                            indexed_source_path: absolute,
                            source_kind: budget.text(r, 5)?,
                            reader_format: budget.optional(r, 6)?,
                            document_format: budget.optional(r, 7)?,
                            text_encoding: budget.optional(r, 13)?,
                            file_size: int(r, 8)?,
                            modified_at: budget.text(r, 9)?,
                            source_resource_stamp: budget.optional(r, 10)?,
                            page_count: int(r, 11)?,
                            index_error: budget.optional(r, 12)?,
                            pages: vec![],
                            status: "UNCLASSIFIED".into(),
                        },
                    );
                }
            }
            {
                let mut stmt=tx.prepare("SELECT p.comic_book_id,p.id,p.page_index,p.page_name,p.source_locator,p.file_size,p.crc32,p.modified_at FROM comic_pages p JOIN comic_books b ON b.id=p.comic_book_id JOIN nodes n ON n.id=b.node_id WHERE n.library_root_id=?1 ORDER BY p.comic_book_id,p.page_index,p.id").map_err(sql_error)?;
                let mut rows = stmt.query([root_id]).map_err(sql_error)?;
                while let Some(r) = rows.next().map_err(sql_error)? {
                    check(options)?;
                    budget.row()?;
                    let source = sources.get_mut(&int(r, 0)?).ok_or("PAGE_WITHOUT_SOURCE")?;
                    if source.pages.len() >= 10000 {
                        return Err("PAGE_LIMIT".into());
                    }
                    source.pages.push(Page {
                        id: int(r, 1)?,
                        page_index: int(r, 2)?,
                        name: budget.text(r, 3)?,
                        source_locator: budget.text(r, 4)?,
                        file_size: int(r, 5)?,
                        crc32: opt_int(r, 6)?,
                        modified_at: budget.text(r, 7)?,
                    });
                }
            }
            let mut resources = Vec::new();
            {
                let mut stmt=tx.prepare("SELECT f.id,f.node_id,f.absolute_path,f.file_size,f.modified_at, EXISTS(SELECT 1 FROM comic_books b WHERE b.source_resource_id IS NULL AND b.node_id=f.node_id AND b.source_path=f.absolute_path COLLATE NOCASE AND lower(b.source_path) NOT LIKE '%.zip') OR EXISTS(SELECT 1 FROM comic_books b JOIN comic_pages p ON p.comic_book_id=b.id WHERE b.source_resource_id IS NULL AND b.node_id=f.node_id AND b.source_kind='IMAGE_FOLDER' AND p.source_locator=f.absolute_path COLLATE NOCASE),f.extension,f.resource_type FROM resource_files f JOIN nodes n ON n.id=f.node_id WHERE n.library_root_id=?1 ORDER BY f.id").map_err(sql_error)?;
                let mut rows = stmt.query([root_id]).map_err(sql_error)?;
                while let Some(r) = rows.next().map_err(sql_error)? {
                    check(options)?;
                    budget.row()?;
                    let indexed_source_path = budget.text(r, 2)?;
                    let path = relative_index_path(&root_path, &indexed_source_path)?;
                    let extension = budget.text(r, 6)?;
                    resources.push(Resource {
                        id: int(r, 0)?,
                        node_id: int(r, 1)?,
                        readable_suffix: matches!(
                            extension
                                .trim_start_matches('.')
                                .to_ascii_lowercase()
                                .as_str(),
                            "cbz" | "pdf" | "epub" | "txt" | "mobi" | "azw3"
                        ),
                        extension,
                        resource_type: budget.text(r, 7)?,
                        path,
                        indexed_source_path,
                        file_size: int(r, 3)?,
                        modified_at: budget.text(r, 4)?,
                        core_duplicate: int(r, 5)? != 0,
                    });
                }
            }
            let resource_by_id: BTreeMap<_, _> = resources.iter().map(|r| (r.id, r)).collect();
            for s in sources.values() {
                if let Some(id) = s.resource_id {
                    let r = resource_by_id
                        .get(&id)
                        .ok_or("CROSS_ROOT_OR_MISSING_RESOURCE")?;
                    if s.node_id != r.node_id || !s.path.eq_ignore_ascii_case(&r.path) {
                        return Err("RESOURCE_SOURCE_CONFLICT".into());
                    }
                }
            }
            let mut entries =
                BTreeMap::from([(String::new(), entry(String::new(), EntryKind::Directory))]);
            let mut hard = BTreeMap::new();
            let file_nodes: BTreeSet<_> = sources
                .values()
                .filter(|s| s.source_kind != "IMAGE_FOLDER" || s.resource_id.is_some())
                .map(|s| (s.node_id, s.path.as_str()))
                .collect();
            for n in nodes.values() {
                let mut e = entry(
                    n.path.clone(),
                    if file_nodes.contains(&(n.id, n.path.as_str())) {
                        EntryKind::File
                    } else {
                        EntryKind::Directory
                    },
                );
                let mut cursor = Some(n.id);
                let mut excluded = false;
                while let Some(id) = cursor {
                    let a = &nodes[&id];
                    excluded |= a.node_type == "IGNORED";
                    cursor = a.parent;
                }
                if excluded {
                    e.state = EntryState::Excluded;
                }
                if n.manual || n.binding.is_some() {
                    // Conservative hard fence: even equal bound descendants remain separately reviewable.
                    if !n.path.is_empty() {
                        hard.insert(
                            n.path.clone(),
                            ManualOverride {
                                path: n.path.clone(),
                                role: None,
                                series_directory: None,
                                volume: None,
                                chapter: None,
                                no_merge: true,
                            },
                        );
                    }
                    bump(
                        &mut diagnostics,
                        if n.manual {
                            "MANUAL_CLASSIFICATION_FENCE"
                        } else {
                            "BINDING_FENCE"
                        },
                    );
                }
                entries.insert(n.path.clone(), e);
            }
            let mut prior = Vec::new();
            let mut page_orders = Vec::new();
            for s in sources.values_mut() {
                let mut e = entry(
                    s.path.clone(),
                    if s.source_kind == "IMAGE_FOLDER" && s.resource_id.is_none() {
                        EntryKind::Directory
                    } else {
                        EntryKind::File
                    },
                );
                if let Some(old) = entries.get(&s.path) {
                    e.state = old.state;
                }
                let n = &nodes[&s.node_id];
                if !(n.path.is_empty()
                    || s.path.eq_ignore_ascii_case(&n.path)
                    || s.path
                        .get(..n.path.len() + 1)
                        .is_some_and(|p| p.eq_ignore_ascii_case(&format!("{}/", n.path))))
                {
                    return Err("BOOK_PATH_OWNER_CONFLICT".into());
                }
                let mut cursor = Some(n.id);
                while let Some(id) = cursor {
                    if nodes[&id].node_type == "IGNORED" {
                        e.state = EntryState::Excluded;
                    }
                    cursor = nodes[&id].parent;
                }
                e.identity = Some(format!("book:{}", s.book_id));
                let valid = s.index_error.is_none()
                    && !s.revision.is_empty()
                    && s.page_count > 0
                    && s.page_count == s.pages.len() as i64
                    && s.pages
                        .iter()
                        .enumerate()
                        .all(|(i, p)| p.page_index == i as i64);
                let format =
                    document_format(s.reader_format.as_deref().or(if s.text_encoding.is_some() {
                        Some("TXT")
                    } else {
                        s.document_format.as_deref()
                    }))
                    .or_else(|| {
                        (s.source_kind == "ZIP_ARCHIVE"
                            && s.path.to_ascii_lowercase().ends_with(".cbz"))
                        .then_some(Format::Cbz)
                    });
                if s.resource_id.is_some() {
                    if s.source_kind == "IMAGE_FOLDER" {
                        for page in &s.pages {
                            if !relative_index_path(&root_path, &page.source_locator)?
                                .eq_ignore_ascii_case(&s.path)
                            {
                                return Err("ATTACHMENT_IMAGE_LOCATOR_CONFLICT".into());
                            }
                        }
                    }
                    s.status = "ATTACHMENT_RETAINED_NOT_A_WORK".into();
                } else if s.source_kind == "IMAGE_FOLDER" {
                    let mut page_paths = Vec::new();
                    for p in &s.pages {
                        let path = relative_index_path(&root_path, &p.source_locator)?;
                        if path.rsplit_once('/').map_or("", |(d, _)| d) != s.path {
                            return Err("NON_DIRECT_IMAGE_PAGE".into());
                        }
                        let mut page = entry(path.clone(), EntryKind::File);
                        page.format = suffix_format(&path);
                        page.state = e.state;
                        if entries.insert(path.clone(), page).is_some() {
                            return Err("DUPLICATE_IMAGE_PAGE_PATH".into());
                        }
                        page_paths.push(path);
                    }
                    page_orders.push(PageOrder {
                        directory: s.path.clone(),
                        pages: page_paths,
                        basis: OrderBasis::ExistingIndexedNaturalOrder,
                        include_covers: false,
                    });
                    s.status = if valid {
                        "INDEXED_IMAGE_BINARY_VALIDATION_UNKNOWN"
                    } else {
                        "INVALID_BOOK_INDEX_RETAINED"
                    }
                    .into();
                    // Scanner stores suffix/size/order, not binary validation. Do NOT set verified.
                } else if valid
                    && format.is_some()
                    && format == suffix_format(&s.path)
                    && s.document_format
                        .as_ref()
                        .is_none_or(|d| s.reader_format.as_ref().is_none_or(|r| r == d))
                    && e.state != EntryState::Excluded
                {
                    e.verified = true;
                    e.format = format;
                    s.status = "INDEXED_FORMAT_EVIDENCE".into();
                } else {
                    s.status = "UNAVAILABLE_OR_UNVERIFIED_INDEX".into();
                    e.format = format;
                }
                if s.status != "INDEXED_FORMAT_EVIDENCE" {
                    bump(&mut diagnostics, &s.status);
                }
                if let Some(old) = entries.get(&s.path) {
                    if old.kind != e.kind {
                        return Err("SOURCE_KIND_PATH_CONFLICT".into());
                    }
                }
                if entries.get(&s.path).is_some_and(|v| v.identity.is_some()) {
                    return Err("DUPLICATE_BOOK_PATH_REVIEW_REQUIRED".into());
                }
                entries.insert(s.path.clone(), e);
                prior.push(PriorUnit {
                    source_ref: serde_json::to_string(&(
                        format!("root:{root_id}"),
                        if s.source_kind == "IMAGE_FOLDER" && s.resource_id.is_none() {
                            "DIRECT_PAGES"
                        } else {
                            "FILE"
                        },
                        format!("native:book:{}", s.book_id),
                    ))
                    .map_err(|_| "SOURCE_REF_FAILED")?,
                    path: s.path.clone(),
                });
            }
            for r in &resources {
                if !r.core_duplicate && !entries.contains_key(&r.path) {
                    entries.insert(r.path.clone(), entry(r.path.clone(), EntryKind::File));
                }
                if r.readable_suffix && !r.core_duplicate {
                    bump(&mut diagnostics, "READABLE_ATTACHMENT_NOT_PROMOTED");
                }
            }
            if entries.len() > 200_000 {
                return Err("PHYSICAL_ENTRY_LIMIT".into());
            }
            let index_version = digest(&(
                version, root_id, &root_path, media, &mode, policy, &scan, &nodes, &sources, &resources,
            ))?;
            if options
                .expected_version
                .as_ref()
                .is_some_and(|v| v != &index_version)
            {
                return Err("STALE_INDEX_NO_REPORT".into());
            }
            // Shadow overrides never weaken production manual/binding/ignored boundaries.
            let override_revision = digest(&options.overrides)?;
            let mut overrides = options.overrides.clone();
            for o in &mut overrides {
                o.path = normalize_path(&o.path)?;
                if hard.contains_key(&o.path) {
                    o.no_merge = true;
                }
            }
            let override_paths: BTreeSet<_> = overrides.iter().map(|v| v.path.clone()).collect();
            for (path, o) in hard {
                if !override_paths.contains(&path) {
                    overrides.push(o);
                }
            }
            overrides.sort_by(|a, b| a.path.cmp(&b.path));
            let current_browse_node_ids = nodes
                .values()
                .filter(|n| {
                    n.node_type != "IGNORED" && n.parent.is_none_or(|id| nodes[&id].path.is_empty())
                })
                .filter(|n| !n.path.is_empty())
                .map(|n| n.id)
                .collect::<Vec<_>>();
            let hidden: BTreeSet<_> = nodes
                .values()
                .filter(|n| n.path.is_empty())
                .map(|n| n.id)
                .collect();
            let current_browse_book_ids = sources
                .values()
                .filter(|s| {
                    hidden.contains(&s.node_id) && !s.path.to_ascii_lowercase().ends_with(".zip")
                })
                .map(|s| s.book_id)
                .collect::<Vec<_>>();
            let indexed: BTreeSet<_> = current_browse_book_ids
                .iter()
                .map(|id| sources[id].path.to_lowercase())
                .collect();
            let visible_resources = resources
                .iter()
                .filter(|r| {
                    hidden.contains(&r.node_id)
                        && !r.core_duplicate
                        && !indexed.contains(&r.path.to_lowercase())
                })
                .collect::<Vec<_>>();
            let current_browse_readable_resource_ids = visible_resources
                .iter()
                .filter(|r| r.readable_suffix)
                .map(|r| r.id)
                .collect();
            let current_browse_other_resource_ids = visible_resources
                .iter()
                .filter(|r| !r.readable_suffix)
                .map(|r| r.id)
                .collect();
            let mut anchors = nodes
                .values()
                .filter(|n| n.path.is_empty())
                .map(|n| n.id)
                .chain(current_browse_node_ids.iter().take(64).copied())
                .collect::<Vec<_>>();
            anchors.sort_unstable();
            anchors.dedup();
            anchors.truncate(65);
            if current_browse_node_ids.len() > 64 {
                bump(&mut diagnostics, "DETAIL_COMPARISON_SAMPLED_65_ANCHORS");
            }
            let current_details = anchors
                .into_iter()
                .map(|id| detail_view(&tx, id, &nodes, &sources, &resources))
                .collect::<Result<Vec<_>>>()?;
            check(options)?;
            tx.rollback().map_err(sql_error)?;
            Ok(IndexSnapshot {
                root_identity: root_id,
                indexed_root_path: root_path,
                media_type: media,
                recognition_mode: mode,
                index_snapshot_version: index_version,
                override_revision,
                scan_health: scan.clone(),
                nodes,
                source_id_map: sources,
                resources,
                recognition_input: Snapshot {
                    root_id: format!("root:{root_id}"),
                    media_kind: media,
                    complete: scan.complete,
                    entries: entries.into_values().collect(),
                    page_orders,
                    overrides,
                    prior_units: prior,
                },
                current_browse_node_ids,
                current_browse_book_ids,
                current_browse_readable_resource_ids,
                current_browse_other_resource_ids,
                current_details,
                diagnostics,
            })
        })();
        self.connection.progress_handler(0, None::<fn() -> bool>);
        result
    }
}

/// Mirrors comics::owned_nodes / populate_detail and db::list_resources_for_nodes_conn.
/// Kept independent of Tauri; parity tests and source references document this read-model copy.
fn detail_view(
    c: &Connection,
    anchor: i64,
    nodes: &BTreeMap<i64, Node>,
    sources: &BTreeMap<i64, Source>,
    resources: &[Resource],
) -> Result<DetailView> {
    let mut stmt=c.prepare("WITH RECURSIVE owned(id) AS (SELECT id FROM nodes WHERE id=?1 UNION ALL SELECT n.id FROM nodes n JOIN owned o ON n.parent_node_id=o.id WHERE n.node_type<>'IGNORED' AND n.manual_type_override=0 AND (NOT EXISTS(SELECT 1 FROM metadata_bindings b WHERE b.node_id=n.id) OR EXISTS(SELECT 1 FROM metadata_bindings b JOIN metadata_bindings p ON p.node_id=?1 WHERE b.node_id=n.id AND b.provider_subject_id=p.provider_subject_id AND b.provider_subject_type=p.provider_subject_type))) SELECT id FROM owned").map_err(sql_error)?;
    let owned = stmt
        .query_map([anchor], |r| r.get::<_, i64>(0))
        .map_err(sql_error)?
        .collect::<std::result::Result<BTreeSet<_>, _>>()
        .map_err(sql_error)?;
    let mut expanded = BTreeSet::from([anchor]);
    let mut book_ids = Vec::new();
    for s in sources
        .values()
        .filter(|s| owned.contains(&s.node_id) && !s.path.to_ascii_lowercase().ends_with(".zip"))
    {
        book_ids.push(s.book_id);
        let mut next = Some(s.node_id);
        while let Some(id) = next {
            if !owned.contains(&id) || !expanded.insert(id) {
                break;
            }
            next = nodes[&id].parent;
        }
    }
    let remaining_child_ids = nodes
        .values()
        .filter(|n| {
            n.node_type != "IGNORED"
                && n.parent.is_some_and(|p| expanded.contains(&p))
                && !expanded.contains(&n.id)
        })
        .map(|n| n.id)
        .collect();
    let attachment_resource_ids = resources
        .iter()
        .filter(|r| expanded.contains(&r.node_id) && !r.core_duplicate)
        .map(|r| r.id)
        .collect();
    let indexed_paths: BTreeSet<_> = book_ids
        .iter()
        .map(|id| sources[id].path.to_lowercase())
        .collect();
    let presented = resources
        .iter()
        .filter(|r| {
            expanded.contains(&r.node_id)
                && !r.core_duplicate
                && !indexed_paths.contains(&r.path.to_lowercase())
        })
        .collect::<Vec<_>>();
    let unopened_readable_resource_ids = presented
        .iter()
        .filter(|r| r.readable_suffix)
        .map(|r| r.id)
        .collect::<Vec<_>>();
    let other_resource_ids = presented
        .iter()
        .filter(|r| !r.readable_suffix)
        .map(|r| r.id)
        .collect();
    let display_readable_count = book_ids.len() + unopened_readable_resource_ids.len();
    Ok(DetailView {
        anchor_node_id: anchor,
        book_ids,
        remaining_child_ids,
        attachment_resource_ids,
        unopened_readable_resource_ids,
        other_resource_ids,
        display_readable_count,
    })
}

#[cfg(all(test, feature = "fixtures"))]
mod tests {
    use super::*;
    #[test]
    fn actual_adapter_connection_is_query_only_and_read_only() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.tmp/smart-mixed-phase2/tests");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(format!(
            "guard-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _f = crate::fixture::Factory::create(&p, LibraryKind::Comic, "FOLDER").unwrap();
        let index = ReadIndex::open(&p).unwrap();
        assert_eq!(
            index
                .connection
                .query_row("PRAGMA query_only", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert!(index
            .connection
            .execute("DELETE FROM library_roots", [])
            .is_err());
        index
            .connection
            .execute_batch("PRAGMA query_only=OFF;")
            .unwrap();
        assert!(index
            .connection
            .execute("DELETE FROM library_roots", [])
            .is_err());
    }
}
