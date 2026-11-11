//! Read-only card summaries embedded in collection snapshots; bounded legacy refresh remains.
use crate::{
    db::{AppResult, Database},
    logical_works::LogicalWorkIndex,
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub node_id: i64,
    #[serde(default)]
    pub work_view: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub node_id: i64,
    pub work_view: bool,
    pub watched: usize,
    pub total: usize,
    pub started: bool,
}
/// Scoped collection projection, shared by every card returned in the same SQLite snapshot.
/// No separate first-paint queue, persistent summary table or media reads.
pub(crate) struct Hydrator<'a> {
    connection: &'a rusqlite::Connection,
    graph: Option<LogicalWorkIndex>,
    summaries: std::collections::HashMap<(i64, bool), Summary>,
    hidden: Option<std::collections::HashSet<i64>>,
    video: crate::episode_watch::PresentationReader,
    memberships: Option<std::collections::BTreeMap<String, Vec<i64>>>,
}
impl<'a> Hydrator<'a> {
    pub(crate) fn new(
        connection: &'a rusqlite::Connection,
        graph: Option<LogicalWorkIndex>,
    ) -> Self {
        Self {
            connection,
            graph,
            summaries: Default::default(),
            hidden: None,
            video: Default::default(),
            memberships: None,
        }
    }
    pub(crate) fn node(
        &mut self,
        node: &mut crate::models::MediaNode,
        work_view: bool,
    ) -> AppResult<()> {
        let key = (node.id, work_view);
        if let Some(value) = self.summaries.get(&key) {
            node.card_progress = Some(value.clone());
            return Ok(());
        }
        if self.hidden.is_none() {
            self.hidden = Some(crate::db::hidden_node_ids_conn(self.connection)?);
        }
        if self.hidden.as_ref().expect("loaded").contains(&node.id) {
            node.card_progress = None;
            return Ok(());
        }
        let (watched, total, started) = if node.media_kind.is_book() {
            crate::comics::progress_counts_conn(self.connection, node.id)?
        } else {
            if self.graph.is_none() {
                self.graph = Some(LogicalWorkIndex::load_for_detail(self.connection)?);
            }
            let graph = self.graph.as_mut().expect("loaded");
            if work_view && self.memberships.is_none() {
                self.memberships = Some(crate::works::progress_membership_index(graph));
            }
            let sources = crate::works::progress_sources_indexed(
                graph,
                node,
                work_view,
                self.memberships.as_ref(),
            )?;
            let detail = crate::works::progress_detail_for_sources_conn(
                self.connection,
                graph,
                node.clone(),
                work_view,
                sources,
            )?;
            let (watched, total) = self.video.counts(self.connection, &detail, work_view)?;
            (watched, total, watched > 0)
        };
        let value = Summary {
            node_id: node.id,
            work_view,
            watched,
            total,
            started,
        };
        self.summaries.insert(key, value.clone());
        node.card_progress = Some(value);
        Ok(())
    }
    pub(crate) fn nodes(
        &mut self,
        nodes: &mut [crate::models::MediaNode],
        work_view: bool,
    ) -> AppResult<()> {
        if nodes.is_empty() {
            return Ok(());
        }
        if self.graph.is_none()
            && !work_view
            && nodes.iter().any(|n| {
                !n.media_kind.is_book() && !self.summaries.contains_key(&(n.id, work_view))
            })
        {
            let roots = nodes
                .iter()
                .filter(|n| !n.media_kind.is_book())
                .map(|n| n.library_root_id)
                .collect::<std::collections::HashSet<_>>();
            if roots.len() == 1 {
                self.graph = Some(LogicalWorkIndex::load_for_root_detail(
                    self.connection,
                    *roots.iter().next().expect("one root"),
                )?);
            }
        }
        if self.hidden.is_none() {
            self.hidden = Some(crate::db::hidden_node_ids_conn(self.connection)?);
        }
        let ids = nodes
            .iter()
            .filter(|node| {
                node.media_kind.is_book()
                    && !self.summaries.contains_key(&(node.id, work_view))
                    && !self.hidden.as_ref().expect("loaded").contains(&node.id)
            })
            .map(|node| node.id)
            .collect::<Vec<_>>();
        for (node_id, (watched, total, started)) in
            crate::comics::progress_counts_for_nodes_conn(self.connection, &ids)?
        {
            self.summaries.insert(
                (node_id, work_view),
                Summary {
                    node_id,
                    work_view,
                    watched,
                    total,
                    started,
                },
            );
        }
        let video_nodes = nodes
            .iter()
            .filter(|n| {
                !n.media_kind.is_book()
                    && !self.summaries.contains_key(&(n.id, work_view))
                    && !self.hidden.as_ref().expect("loaded").contains(&n.id)
            })
            .collect::<Vec<_>>();
        if !video_nodes.is_empty() {
            if self.graph.is_none() {
                self.graph = Some(LogicalWorkIndex::load_for_detail(self.connection)?);
            }
            let graph = self.graph.as_mut().expect("loaded");
            if work_view && self.memberships.is_none() {
                self.memberships = Some(crate::works::progress_membership_index(graph));
            }
            // Bound temporary file/identity DTOs even for very large catalogues.
            for video_nodes in video_nodes.chunks(32) {
                let plans = video_nodes
                    .iter()
                    .map(|node| {
                        crate::works::progress_sources_indexed(
                            graph,
                            node,
                            work_view,
                            self.memberships.as_ref(),
                        )
                    })
                    .collect::<AppResult<Vec<_>>>()?;
                let mut source_ids = std::collections::HashSet::new();
                for sources in &plans {
                    source_ids.extend(sources.iter().map(|n| n.id));
                }
                let ids = source_ids.into_iter().collect::<Vec<_>>();
                graph.load_detail_videos(self.connection, &ids, false)?;
                let missing = ids
                    .into_iter()
                    .filter(|id| !graph.videos.contains_key(id))
                    .collect::<Vec<_>>();
                for file in crate::db::list_media_for_nodes_conn(self.connection, &missing)? {
                    graph.videos.entry(file.node_id).or_default().push(file);
                }
                for id in missing {
                    graph.videos.entry(id).or_default();
                }
                let details = video_nodes
                    .iter()
                    .zip(plans)
                    .map(|(n, sources)| {
                        crate::works::progress_detail_for_sources_conn(
                            self.connection,
                            graph,
                            (*n).clone(),
                            work_view,
                            sources,
                        )
                    })
                    .collect::<AppResult<Vec<_>>>()?;
                let counts = self
                    .video
                    .counts_many(self.connection, &details, work_view)?;
                for (node, (watched, total)) in video_nodes.iter().zip(counts) {
                    self.summaries.insert(
                        (node.id, work_view),
                        Summary {
                            node_id: node.id,
                            work_view,
                            watched,
                            total,
                            started: watched > 0,
                        },
                    );
                }
            }
        }
        for node in nodes {
            self.node(node, work_view)?;
        }
        Ok(())
    }
    pub(crate) fn episode_watch(
        &mut self,
        detail: &crate::models::NodeDetail,
        work_view: bool,
    ) -> AppResult<crate::episode_watch::EpisodeWatchSnapshot> {
        let owned;
        let projection = if work_view {
            detail
        } else {
            let graph = self.graph.as_mut().expect("video detail graph");
            let node = graph
                .nodes
                .get(&detail.node.id)
                .cloned()
                .ok_or("NODE_NOT_VISIBLE")?;
            owned =
                crate::works::progress_detail_for_node_conn(self.connection, graph, node, false)?;
            &owned
        };
        self.video.snapshot(self.connection, projection, work_view)
    }
    pub(crate) fn library_nodes(
        &mut self,
        root: i64,
        nodes: &mut [crate::models::MediaNode],
    ) -> AppResult<()> {
        if nodes.iter().any(|n| !n.media_kind.is_book())
            && nodes.iter().all(|n| n.library_root_id == root)
        {
            let counts = self.video.library_counts(self.connection, root, nodes)?;
            for (node, count) in nodes.iter().zip(counts) {
                if let Some((watched, total)) = count {
                    self.summaries.insert(
                        (node.id, false),
                        Summary {
                            node_id: node.id,
                            work_view: false,
                            watched,
                            total,
                            started: watched > 0,
                        },
                    );
                }
            }
        }
        self.nodes(nodes, false)
    }
    pub(crate) fn catalogue(
        &mut self,
        catalogue: &mut crate::smart_mixed::Catalogue,
    ) -> AppResult<()> {
        self.nodes(&mut catalogue.directory_nodes, false)?;
        for group in &mut catalogue.groups {
            if let Some(node) = &mut group.cover_node {
                self.node(node, false)?;
            }
        }
        Ok(())
    }
    pub(crate) fn work_groups(&mut self, groups: &mut [crate::models::WorkGroup]) -> AppResult<()> {
        // Share the snapshot's membership index, while retaining the previous
        // single-aggregate memory peak, source order and cache access order.
        for group in groups {
            self.node(&mut group.node, true)?;
            self.nodes(&mut group.sources, false)?;
        }
        Ok(())
    }
}
pub(crate) fn hydrate_nodes_conn(
    c: &rusqlite::Connection,
    nodes: &mut [crate::models::MediaNode],
) -> AppResult<()> {
    Hydrator::new(c, None).nodes(nodes, false)
}
pub(crate) fn hydrate_library_nodes_conn(
    c: &rusqlite::Connection,
    root: i64,
    nodes: &mut [crate::models::MediaNode],
) -> AppResult<()> {
    Hydrator::new(c, None).library_nodes(root, nodes)
}
pub fn read(db: &Database, requests: &[Request]) -> AppResult<Vec<Summary>> {
    if requests.len() > 16 || requests.iter().any(|r| r.node_id <= 0) {
        return Err("CARD_PROGRESS_SELECTION_INVALID".into());
    }
    db.read_snapshot(|c| {
        let mut hydrator = Hydrator::new(c, None);
        let mut result = Vec::new();
        for request in requests {
            let Ok(mut node) = crate::db::get_node_conn(c, request.node_id) else {
                continue;
            };
            if crate::db::ensure_node_visible_conn(c, node.id).is_err() {
                continue;
            }
            hydrator.node(&mut node, request.work_view)?;
            result.push(node.card_progress.expect("hydrated"));
        }
        Ok(result)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn assert_batched_groups_match_reference(db: &Database) {
        let full = db.list_all_resources().unwrap();
        db.read_snapshot(|c| {
            let mut graph = LogicalWorkIndex::load_for_catalogue(c)?;
            let membership = crate::works::progress_membership_index(&graph);
            let mut reader = crate::episode_watch::PresentationReader::default();
            for group in &full.works {
                let original = crate::works::progress_sources(&graph, &group.node, true)?;
                let indexed = crate::works::progress_sources_indexed(
                    &graph,
                    &group.node,
                    true,
                    Some(&membership),
                )?;
                assert_eq!(
                    serde_json::to_value(original).unwrap(),
                    serde_json::to_value(indexed).unwrap()
                );
                let detail = crate::works::progress_detail_for_node_conn(
                    c,
                    &mut graph,
                    group.node.clone(),
                    true,
                )?;
                let expected = reader.counts(c, &detail, true)?;
                let actual = group
                    .node
                    .card_progress
                    .as_ref()
                    .expect("inline aggregate progress");
                assert_eq!((actual.watched, actual.total), expected);
            }
            Ok(())
        })
        .unwrap();
    }
    #[test]
    fn collection_snapshots_include_progress_before_returning_every_video_card() {
        let t = crate::db::test_temp_dir();
        let db = Database::new(t.path().join("inline-video.db"));
        db.migrate().unwrap();
        let c = db.connect().unwrap();
        c.execute("INSERT INTO library_roots(id,path,display_name,media_kind,video_subject_scope) VALUES(1,?1,'Synthetic','VIDEO','ANIMATION')",[t.path().join("Synthetic").to_string_lossy()]).unwrap();
        c.execute("INSERT INTO nodes(id,library_root_id,absolute_path,folder_name,display_name,node_type) VALUES(1,1,?1,'Synthetic','Synthetic','CONTAINER')",[t.path().join("Synthetic").to_string_lossy()]).unwrap();
        for id in 2..=97 {
            let path = t.path().join(format!("Synthetic/Show {id}"));
            c.execute("INSERT INTO nodes(id,library_root_id,parent_node_id,absolute_path,folder_name,display_name,node_type,manual_type_override,direct_video_count,total_video_count) VALUES(?1,1,1,?2,?3,?3,'WORK',1,2,2)",rusqlite::params![id,path.to_string_lossy(),format!("Show {id}")]).unwrap();
            c.execute("INSERT INTO metadata_bindings(node_id,provider,provider_subject_id,provider_subject_type,provider_title) VALUES(?1,'BANGUMI',?1,2,?2)",rusqlite::params![id,format!("Show {id}")]).unwrap();
            for episode in 1..=2 {
                let name = format!("Show {id} S01E0{episode}.mkv");
                c.execute("INSERT INTO media_files(node_id,absolute_path,file_name,extension,file_size,modified_at) VALUES(?1,?2,?3,'mkv',123,'stamp')",rusqlite::params![id,path.join(&name).to_string_lossy(),name]).unwrap();
            }
        }
        let start = std::time::Instant::now();
        let all = db.list_all_resources().unwrap();
        assert_batched_groups_match_reference(&db);
        eprintln!(
            "96-card snapshot with both projections: {:?}",
            start.elapsed()
        );
        assert_eq!((all.nodes.len(), all.works.len()), (96, 96));
        for node in &all.nodes {
            let p = node.card_progress.as_ref().unwrap();
            assert_eq!(
                (p.node_id, p.work_view, p.watched, p.total, p.started),
                (node.id, false, 0, 2, false)
            );
        }
        for group in &all.works {
            assert!(group.node.card_progress.as_ref().unwrap().work_view);
        }
        let watch = db.episode_watch_snapshot(2, false).unwrap();
        db.set_episode_watched(2, false, 1, true, &watch.snapshot)
            .unwrap();
        let all = db.list_all_resources().unwrap();
        let p = all
            .nodes
            .iter()
            .find(|n| n.id == 2)
            .unwrap()
            .card_progress
            .as_ref()
            .unwrap();
        assert_eq!((p.watched, p.total, p.started), (1, 2, true));
    }
    #[test]
    fn book_cards_use_current_pages_and_unread_books_without_writing() {
        let t = crate::db::test_temp_dir();
        let db = Database::new(t.path().join("book-cards.db"));
        db.migrate().unwrap();
        let c = db.connect().unwrap();
        c.execute("INSERT INTO library_roots(id,path,display_name,media_kind,book_library_kind) VALUES(1,?1,'Books','COMIC','EBOOK')",[t.path().join("Synthetic").to_string_lossy()]).unwrap();
        c.execute("INSERT INTO nodes(id,library_root_id,absolute_path,folder_name,display_name,node_type,total_comic_book_count) VALUES(1,1,?1,'Books','Books','CONTAINER',1)",[t.path().join("Synthetic").to_string_lossy()]).unwrap();
        c.execute("INSERT INTO nodes(id,library_root_id,parent_node_id,absolute_path,folder_name,display_name,node_type,direct_comic_book_count,total_comic_book_count) VALUES(2,1,1,?1,'Novel','Novel','WORK',1,1)",[t.path().join("Synthetic/Novel").to_string_lossy()]).unwrap();
        c.execute("INSERT INTO comic_books(id,node_id,source_path,source_kind,display_name,modified_at,page_count) VALUES(1,2,?1,'IMAGE_FOLDER','Novel','2026-10-26',10)",[t.path().join("Synthetic/Novel").to_string_lossy()]).unwrap();
        let request = [Request {
            node_id: 2,
            work_view: false,
        }];
        assert!(!read(&db, &request).unwrap()[0].started);
        c.execute(
            "INSERT INTO comic_reading_progress(comic_book_id,last_page_index) VALUES(1,3)",
            [],
        )
        .unwrap();
        let p = read(&db, &request).unwrap().remove(0);
        assert_batched_groups_match_reference(&db);
        assert_eq!((p.watched, p.total, p.started), (4, 10, true));
        let all = db.list_all_resources().unwrap();
        let p = all
            .nodes
            .iter()
            .find(|node| node.id == 2)
            .unwrap()
            .card_progress
            .as_ref()
            .unwrap();
        assert_eq!((p.watched, p.total, p.started), (4, 10, true));
        c.execute("UPDATE comic_reading_progress SET last_page_index=9", [])
            .unwrap();
        assert_eq!(read(&db, &request).unwrap()[0].watched, 10);
    }
}
