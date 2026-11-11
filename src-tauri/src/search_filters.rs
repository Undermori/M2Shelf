use crate::db::{AppResult, Database};
use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SearchFilters {
    pub media_kind: Option<String>,
    pub binding: Option<String>,
    pub cover: Option<String>,
    pub extension: Option<String>,
    pub scan_root_id: Option<i64>,
    pub collection_status: Option<String>,
}

impl SearchFilters {
    pub fn validate(&self) -> AppResult<()> {
        if self.media_kind.as_ref().is_some_and(|s| {
            !matches!(
                s.as_str(),
                "ANIMATION" | "LIVE_ACTION" | "COMIC" | "EBOOK" | "DOUJIN" | "ARTBOOK"
            )
        }) || self
            .binding
            .as_ref()
            .is_some_and(|s| !matches!(s.as_str(), "bound" | "unbound"))
            || self
                .cover
                .as_ref()
                .is_some_and(|s| !matches!(s.as_str(), "available" | "missing"))
            || self.collection_status.as_ref().is_some_and(|s| {
                !matches!(
                    s.as_str(),
                    "UNSET" | "WANT" | "IN_PROGRESS" | "COMPLETED" | "PAUSED" | "DROPPED"
                )
            })
            || self
                .extension
                .as_ref()
                .is_some_and(|s| s.len() > 16 || !s.bytes().all(|b| b.is_ascii_alphanumeric()))
        {
            return Err("SEARCH_FILTER_INVALID".into());
        }
        Ok(())
    }
}

// Shared by all three indexed hit queries, before their LIMIT. No source reads/network.
pub(crate) fn condition(book: bool, file: bool) -> String {
    let extension: String = if book {
        "lower(COALESCE(b.reader_format,b.document_format,CASE WHEN b.source_kind='IMAGE_FOLDER' THEN 'images' ELSE 'cbz' END))=lower(json_extract(?3,'$.extension'))".into()
    } else if file {
        "lower(ltrim(f.extension,'.'))=lower(json_extract(?3,'$.extension'))".into()
    } else {
        "EXISTS(WITH RECURSIVE descendants(id) AS (SELECT n.id UNION SELECT child.id FROM nodes child JOIN descendants d ON child.parent_node_id=d.id WHERE child.node_type<>'IGNORED') SELECT 1 FROM (SELECT node_id,lower(ltrim(extension,'.')) ext FROM media_files UNION ALL SELECT node_id,lower(ltrim(extension,'.')) FROM resource_files UNION ALL SELECT node_id,lower(COALESCE(reader_format,document_format,CASE WHEN source_kind='IMAGE_FOLDER' THEN 'images' ELSE 'cbz' END)) FROM comic_books) indexed_files JOIN descendants d ON indexed_files.node_id=d.id WHERE indexed_files.ext=lower(json_extract(?3,'$.extension')))".into()
    };
    format!("AND NOT EXISTS(WITH RECURSIVE hidden_ancestors(id,parent_node_id,node_type) AS (SELECT id,parent_node_id,node_type FROM nodes WHERE id=n.id UNION SELECT p.id,p.parent_node_id,p.node_type FROM nodes p JOIN hidden_ancestors a ON a.parent_node_id=p.id) SELECT 1 FROM hidden_ancestors WHERE node_type='IGNORED') AND (json_extract(?3,'$.mediaKind') IS NULL OR EXISTS(SELECT 1 FROM library_roots r WHERE r.id=n.library_root_id AND ((CASE WHEN r.artbook_library=1 THEN 'ARTBOOK' WHEN r.doujin_library=1 THEN 'DOUJIN' WHEN r.media_kind='COMIC' THEN r.book_library_kind WHEN r.video_subject_scope<>'MIXED' THEN r.video_subject_scope ELSE r.media_kind END)=json_extract(?3,'$.mediaKind') OR (r.media_kind='VIDEO' AND r.video_subject_scope='MIXED' AND NOT(json_extract(?3,'$.mediaKind')='ANIMATION' AND EXISTS(SELECT 1 FROM tmdb_movie_bindings live WHERE live.node_id=n.id AND live.active=1)) AND json_extract(?3,'$.mediaKind') IN ('ANIMATION','LIVE_ACTION') AND (NOT EXISTS(SELECT 1 FROM metadata_bindings mb WHERE mb.node_id=n.id) OR EXISTS(SELECT 1 FROM metadata_bindings mb WHERE mb.node_id=n.id AND mb.provider_subject_type=CASE json_extract(?3,'$.mediaKind') WHEN 'ANIMATION' THEN 2 ELSE 6 END) OR (json_extract(?3,'$.mediaKind')='LIVE_ACTION' AND EXISTS(SELECT 1 FROM tmdb_movie_bindings tb WHERE tb.node_id=n.id AND tb.active=1)))))))
    AND (json_extract(?3,'$.binding') IS NULL OR ((EXISTS(SELECT 1 FROM metadata_bindings mb WHERE mb.node_id=n.id) OR EXISTS(SELECT 1 FROM tmdb_movie_bindings tb WHERE tb.node_id=n.id AND tb.active=1))=(json_extract(?3,'$.binding')='bound')))
    AND (json_extract(?3,'$.cover') IS NULL OR (EXISTS(SELECT 1 FROM search_available_covers sc WHERE sc.node_id=n.id AND (sc.book_id={}))=(json_extract(?3,'$.cover')='available')))
    AND (json_extract(?3,'$.extension') IS NULL OR {extension})
    AND (json_extract(?3,'$.collectionStatus') IS NULL OR EXISTS(SELECT 1 FROM search_collection_states ss WHERE ss.node_id=n.id AND (ss.book_id=0 OR ss.book_id={}) AND ss.state=json_extract(?3,'$.collectionStatus')))
    AND (json_extract(?3,'$.scanRootId') IS NULL OR (n.library_root_id=json_extract(?3,'$.scanRootId') AND EXISTS(SELECT 1 FROM library_scan_changes ch,json_each(ch.summary_json,'$.addedNodeIds') j WHERE ch.root_id=n.library_root_id AND j.value=n.id)))",if book {"b.id OR (sc.book_id=0 AND n.total_comic_book_count=1)"} else {"sc.book_id"},if book {"b.id"} else {"ss.book_id"})
}

/// Same explicit-node precedence and descendant membership as the collection UI,
/// materialized in this search's existing SQLite snapshot before hit limits.
pub(crate) fn prepare_collection(c: &rusqlite::Connection, enabled: bool) -> AppResult<()> {
    c.execute_batch("CREATE TEMP TABLE search_collection_states(node_id INTEGER,book_id INTEGER,state TEXT);CREATE INDEX search_collection_lookup ON search_collection_states(node_id,book_id,state);").map_err(|e|e.to_string())?;
    if !enabled {
        return Ok(());
    }
    c.execute_batch("INSERT INTO search_collection_states SELECT node_id,0,state FROM collection_states WHERE kind='NODE';
    WITH RECURSIVE members(node_id,book_id) AS (SELECT node_id,id FROM comic_books UNION SELECT n.parent_node_id,m.book_id FROM nodes n JOIN members m ON m.node_id=n.id WHERE n.parent_node_id IS NOT NULL AND n.node_type<>'IGNORED')
    INSERT INTO search_collection_states SELECT m.node_id,m.book_id,COALESCE(cs.state,'UNSET') FROM members m LEFT JOIN collection_states cs ON cs.kind='BOOK' AND cs.book_id=m.book_id WHERE NOT EXISTS(SELECT 1 FROM collection_states own WHERE own.kind='NODE' AND own.node_id=m.node_id);
    WITH RECURSIVE members(node_id,work_id) AS (SELECT id,id FROM nodes WHERE node_type IN ('WORK','AUTO_WORK') AND total_video_count>0 UNION SELECT n.parent_node_id,m.work_id FROM nodes n JOIN members m ON m.node_id=n.id WHERE n.parent_node_id IS NOT NULL AND n.node_type<>'IGNORED')
    INSERT INTO search_collection_states SELECT m.node_id,0,COALESCE(cs.state,'UNSET') FROM members m LEFT JOIN collection_states cs ON cs.kind='NODE' AND cs.node_id=m.work_id WHERE NOT EXISTS(SELECT 1 FROM collection_states own WHERE own.kind='NODE' AND own.node_id=m.node_id);
    INSERT INTO search_collection_states SELECT n.id,0,'UNSET' FROM nodes n WHERE NOT EXISTS(SELECT 1 FROM search_collection_states ss WHERE ss.node_id=n.id);").map_err(|e|e.to_string())
}

pub fn extensions(db: &Database, root: Option<i64>) -> AppResult<Vec<String>> {
    db.read_snapshot(|c| {
        let mut s=c.prepare("SELECT DISTINCT ext FROM (SELECT n.library_root_id root,upper(ltrim(f.extension,'.')) ext FROM media_files f JOIN nodes n ON n.id=f.node_id WHERE n.node_type<>'IGNORED' UNION ALL SELECT n.library_root_id,upper(ltrim(f.extension,'.')) FROM resource_files f JOIN nodes n ON n.id=f.node_id WHERE n.node_type<>'IGNORED' UNION ALL SELECT n.library_root_id,upper(COALESCE(b.reader_format,b.document_format,CASE WHEN b.source_kind='IMAGE_FOLDER' THEN 'images' ELSE 'cbz' END)) FROM comic_books b JOIN nodes n ON n.id=b.node_id WHERE n.node_type<>'IGNORED') WHERE (?1 IS NULL OR root=?1) AND trim(ext,char(9)||char(10)||char(13)||' ')<>'' ORDER BY ext LIMIT 128").map_err(|e|e.to_string())?;
        let rows=s.query_map([root],|r|r.get(0)).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string());rows
    })
}

pub(crate) fn prepare_covers(c: &rusqlite::Connection, db: &Database) -> AppResult<()> {
    c.execute_batch("CREATE TEMP TABLE search_available_covers(node_id INTEGER,book_id INTEGER)")
        .map_err(|e| e.to_string())?;
    let cache_root = db.poster_cache_root(
        &db.path()
            .parent()
            .ok_or("CACHE_PATH_INVALID")?
            .join("cache/covers"),
    )?;
    crate::poster_cache::populate_known_covers(db, c, &cache_root)
}
