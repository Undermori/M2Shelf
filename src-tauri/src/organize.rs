use crate::{
    db::{self, AppResult, Database},
    models::MediaNode,
};
use rusqlite::params;
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub node: MediaNode,
    pub kind: String,
    pub reason: String,
    pub ignored: bool,
}

pub fn list(database: &Database) -> AppResult<Vec<Issue>> {
    database.read_snapshot(|c| {
        let mut reasons:BTreeMap<(i64,String),String>=BTreeMap::new();
        let mut s=c.prepare("SELECT d.node_id,d.outcome FROM match_issue_outcomes d JOIN nodes n ON n.id=d.node_id JOIN library_roots r ON r.id=n.library_root_id WHERE r.auto_bangumi=1 AND NOT EXISTS(SELECT 1 FROM metadata_bindings b WHERE b.node_id=n.id) AND NOT EXISTS(SELECT 1 FROM tmdb_movie_bindings t WHERE t.node_id=n.id AND t.active=1) ORDER BY d.updated_at").map_err(|e|e.to_string())?;
        for row in s.query_map([],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?))).map_err(|e|e.to_string())? {
            let (id,outcome)=row.map_err(|e|e.to_string())?;
            let reason=match outcome.as_str(){"no-results"=>"no-results","multiple-candidates"=>"multiple-candidates","request-failed"=>"network","rate-limited"=>"rate-limited","unauthorized"|"credentials-unavailable"=>"auth","low-confidence"|"title-uncertain"|"year-uncertain"|"no-unique-title-year"|"evidence-conflict"|"detail-conflict"=>"uncertain",_=>continue};
            reasons.insert((id,"match".into()),reason.into());
        }
        let mut s=c.prepare("SELECT n.id FROM nodes n WHERE EXISTS(SELECT 1 FROM poster_cache_failures f WHERE f.node_id=n.id) OR EXISTS(SELECT 1 FROM metadata_bindings b WHERE b.node_id=n.id AND b.cover_download_error IS NOT NULL) OR EXISTS(SELECT 1 FROM tmdb_movie_bindings t WHERE t.node_id=n.id AND t.active=1 AND t.cover_error IS NOT NULL) OR (n.node_type IN ('WORK','AUTO_WORK') AND n.total_video_count>0 AND n.cover_cache_path IS NULL AND NOT EXISTS(SELECT 1 FROM tmdb_movie_bindings t WHERE t.node_id=n.id AND t.active=1 AND t.cover_cache_path IS NOT NULL))").map_err(|e|e.to_string())?;
        for id in s.query_map([],|r|r.get::<_,i64>(0)).map_err(|e|e.to_string())? {reasons.insert((id.map_err(|e|e.to_string())?,"cover".into()),"cover".into());}
        // Lost registered originals are actionable even before another card attempts a read.
        // Inspect application cache metadata only; never open a media source to diagnose it.
        let roots=database.library_paths()?;
        let mut s=c.prepare("SELECT n.id,CASE WHEN n.cover_source='MANUAL' THEN n.cover_cache_path ELSE COALESCE(t.cover_cache_path,n.cover_cache_path) END FROM nodes n LEFT JOIN tmdb_movie_bindings t ON t.node_id=n.id AND t.active=1 WHERE n.cover_cache_path IS NOT NULL OR t.cover_cache_path IS NOT NULL").map_err(|e|e.to_string())?;
        for row in s.query_map([],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?))).map_err(|e|e.to_string())? {
            let (id,path)=row.map_err(|e|e.to_string())?;let path=std::path::PathBuf::from(path);
            if !path.is_absolute()||roots.iter().any(|root|crate::cache::is_equal_or_within_checked(&path,root).unwrap_or(true))||!path.is_file() {
                reasons.insert((id,"cover".into()),"cover".into());
            }
        }
        let mut s=c.prepare("SELECT b.node_id FROM metadata_bindings b JOIN nodes n ON n.id=b.node_id JOIN library_roots r ON r.id=n.library_root_id WHERE (r.media_kind='VIDEO' AND r.video_subject_scope='MIXED' AND b.provider_subject_type NOT IN(2,6)) OR (r.media_kind='COMIC' AND b.provider_subject_type<>1) OR (r.video_subject_scope='ANIMATION' AND b.provider_subject_type<>2) OR (r.video_subject_scope='LIVE_ACTION' AND b.provider_subject_type<>6)").map_err(|e|e.to_string())?;
        for id in s.query_map([],|r|r.get::<_,i64>(0)).map_err(|e|e.to_string())? {reasons.insert((id.map_err(|e|e.to_string())?,"binding".into()),"invalid".into());}
        let mut s=c.prepare(&format!("{} SELECT n.id FROM nodes n WHERE n.parent_node_id IS NOT NULL AND n.node_type<>'IGNORED' AND n.id NOT IN (SELECT id FROM comic_presentation WHERE anchor IS NOT NULL AND id<>anchor)",crate::comics::PRESENTATION_CTE)).map_err(|e|e.to_string())?;
        let eligible=s.query_map([],|r|r.get::<_,i64>(0)).map_err(|e|e.to_string())?.collect::<Result<HashSet<_>,_>>().map_err(|e|e.to_string())?;
        let hidden=db::hidden_node_ids_conn(c)?;
        let graph=crate::logical_works::LogicalWorkIndex::load_for_detail(c)?;
        reasons.retain(|(id,_),_|graph.owners.get(id).is_none_or(|owner|owner==id));
        reasons.retain(|(id,_),_|eligible.contains(id)&&!hidden.contains(id));
        let ids=reasons.keys().map(|(id,_)|*id).collect::<HashSet<_>>().into_iter().collect::<Vec<_>>();
        let mut nodes=db::load_node_rows_by_ids_conn(c,&ids)?;
        db::hydrate_nodes_metadata_conn(c,&mut nodes)?;
        let nodes=nodes.into_iter().map(|n|(n.id,n)).collect::<std::collections::HashMap<_,_>>();
        let mut s=c.prepare("SELECT node_id,kind FROM issue_reminders").map_err(|e|e.to_string())?;
        let ignored=s.query_map([],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?))).map_err(|e|e.to_string())?.collect::<Result<HashSet<_>,_>>().map_err(|e|e.to_string())?;
        Ok(reasons.into_iter().filter_map(|((id,kind),reason)|nodes.get(&id).cloned().map(|node|Issue{node,ignored:ignored.contains(&(id,kind.clone())),kind,reason})).collect())
    })
}

pub fn remind(database: &Database, node_id: i64, kind: &str, ignored: bool) -> AppResult<()> {
    if !matches!(kind, "match" | "cover" | "binding") {
        return Err("ISSUE_KIND_INVALID".into());
    }
    database.read_snapshot(|c| {
        db::ensure_node_visible_conn(c, node_id)?;
        c.execute(
            if ignored {
                "INSERT OR IGNORE INTO issue_reminders(node_id,kind) VALUES(?1,?2)"
            } else {
                "DELETE FROM issue_reminders WHERE node_id=?1 AND kind=?2"
            },
            params![node_id, kind],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
}
