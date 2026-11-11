//! A read-only catalogue over the existing path-owned index. Grouping never merges database
//! rows, so rescans, manual bindings, favorites and playback retain their original identities.
use rusqlite::Connection;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
};

use crate::{
    db::{self, AppResult, Database},
    logical_works::LogicalWorkIndex,
    models::{MediaNode, NestedMediaFile, NodeDetail, NodeType, WorkGroup, WorkTarget},
    title_extractor::{extract_title_signals, is_generic_title, normalize_title_for_match},
};

fn is_file_node(node: &MediaNode) -> bool {
    let path = Path::new(&node.absolute_path);
    path.file_stem().and_then(|name| name.to_str()) == Some(node.folder_name.as_str())
        && path.file_name().and_then(|name| name.to_str()) != Some(node.folder_name.as_str())
}

/// Shared parser strips contextual episodes while retaining the season in the catalogue key.
fn episode_title(value: &str) -> String {
    let parsed = m2shelf_media_parser::parse(value, m2shelf_media_parser::Kind::Animation);
    if !parsed.episodes.is_empty() {
        if let Some(season) = parsed.season {
            format!("{} S{season:02}", parsed.display_title)
        } else {
            parsed.display_title
        }
    } else {
        value.into()
    }
}

fn title_signals(node: &MediaNode) -> crate::title_extractor::TitleSignals {
    extract_title_signals(&if is_file_node(node) {
        episode_title(&node.display_name)
    } else {
        node.display_name.clone()
    })
}

fn local_title(node: &MediaNode) -> String {
    let signals = title_signals(node);
    if signals.cleaned_title.is_empty() || is_generic_title(&signals.cleaned_title) {
        node.display_name.clone()
    } else {
        signals.cleaned_title
    }
}

fn group_key(node: &MediaNode) -> String {
    if let Some(binding) = node.tmdb_binding.as_ref().filter(|v| v.active) {
        let season = crate::media_scraper::parse(node, &[]).season;
        return format!(
            "tmdb:{:?}:{}:{season:?}",
            binding.movie.media_type, binding.movie.id
        );
    }
    if let Some(binding) = &node.binding {
        return format!(
            "bangumi:{}:{}",
            binding.provider_subject_type, binding.provider_subject_id
        );
    }
    // Only flatten unbound episode file nodes within the same physical directory. Folder
    // releases and cross-library aliases need an explicit/shared Bangumi identity to merge.
    let path = Path::new(&node.absolute_path);
    if !is_file_node(node) {
        return format!("node:{}", node.id);
    }
    let signals = title_signals(node);
    let title = normalize_title_for_match(&signals.cleaned_title);
    if title.chars().count() < 3
        || title.chars().all(|c| c.is_numeric())
        || is_generic_title(&signals.cleaned_title)
    {
        return format!("node:{}", node.id);
    }
    format!(
        "local:{}:{:?}:{}:{:?}:{:?}:{:?}",
        node.library_root_id,
        path.parent(),
        title,
        signals.season_number,
        signals.year,
        signals.edition_kind
    )
}

pub fn group_works(mut nodes: Vec<MediaNode>) -> Vec<WorkGroup> {
    nodes.sort_by_key(|node| node.id);
    let mut groups: BTreeMap<String, Vec<MediaNode>> = BTreeMap::new();
    for node in nodes {
        groups.entry(group_key(&node)).or_default().push(node);
    }
    groups
        .into_values()
        .map(|mut sources| {
            // Prefer a source with artwork; ties retain deterministic source identity.
            sources.sort_by_key(|source| (source.cover_cache_path.is_none(), source.id));
            let mut node = sources[0].clone();
            node.node_type = NodeType::Work;
            if node.binding.is_none() && !node.manual_display_name {
                node.display_name = local_title(&node);
            }
            node.direct_video_count = sources.iter().map(|source| source.direct_video_count).sum();
            node.total_video_count = sources.iter().map(|source| source.total_video_count).sum();
            node.child_media_branch_count = 0;
            node.created_at = sources.iter().map(|n| &n.created_at).min().unwrap().clone();
            node.last_watched_at = sources
                .iter()
                .filter_map(|source| source.last_watched_at.as_ref())
                .max()
                .cloned();
            node.latest_file_modified_at = sources
                .iter()
                .filter_map(|source| source.latest_file_modified_at.as_ref())
                .max()
                .cloned();
            if let Some(binding) = node.binding.as_mut() {
                binding.cover_download_error = sources
                    .iter()
                    .filter_map(|source| {
                        source
                            .binding
                            .as_ref()
                            .and_then(|binding| binding.cover_download_error.clone())
                    })
                    .next();
            }
            let mut tags = BTreeMap::new();
            for source in &sources {
                for tag in &source.user_tags {
                    tags.insert(tag.id, tag.clone());
                }
            }
            node.user_tags = tags.into_values().collect();
            let target = target_for_sources(&sources);
            WorkGroup {
                node,
                sources,
                target,
            }
        })
        .collect()
}

pub fn target_for_sources(sources: &[MediaNode]) -> WorkTarget {
    let mut sources = sources.iter().collect::<Vec<_>>();
    sources.sort_by_key(|source| source.id);
    let source_node_ids = sources.iter().map(|source| source.id).collect();
    // Playback, tags and timestamps are presentation state, not operation dependencies.
    let states = sources
        .iter()
        .map(|node| operation_node_state(node))
        .collect::<Vec<_>>();
    let bytes = serde_json::to_vec(&states).expect("Node state serialization is infallible");
    WorkTarget {
        source_node_ids,
        snapshot: format!("{:x}", Sha256::digest(bytes)),
    }
}

fn operation_node_state(node: &MediaNode) -> serde_json::Value {
    serde_json::json!([
        node.id,
        node.library_root_id,
        node.parent_node_id,
        node.absolute_path,
        node.folder_name,
        node.display_name,
        node.node_type,
        node.manual_type_override,
        node.cover_source,
        node.cover_cache_path,
        node.binding.as_ref().map(|binding| (
            &binding.provider,
            binding.provider_subject_type,
            binding.provider_subject_id,
            &binding.provider_image_url,
            &binding.cover_download_error
        ))
    ])
}

/// Include ownership and independent boundaries even when counts and activity times are equal.
pub fn target_for_index(sources: &[MediaNode], index: &LogicalWorkIndex) -> WorkTarget {
    let memberships = membership_index(index);
    target_with_memberships(sources, index, &memberships)
}

fn membership_index(index: &LogicalWorkIndex) -> BTreeMap<String, Vec<i64>> {
    let mut memberships: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    for node in index.nodes.values().filter(|node| {
        node.node_type.is_work()
            && node.total_video_count > 0
            && index.owners.get(&node.id) == Some(&node.id)
    }) {
        memberships
            .entry(group_key(node))
            .or_default()
            .push(node.id);
    }
    for ids in memberships.values_mut() {
        ids.sort_unstable();
    }
    memberships
}

fn target_with_memberships(
    sources: &[MediaNode],
    index: &LogicalWorkIndex,
    all_memberships: &BTreeMap<String, Vec<i64>>,
) -> WorkTarget {
    let mut target = target_for_sources(sources);
    let mut pending = target.source_node_ids.clone();
    let mut descendants = std::collections::BTreeSet::new();
    while let Some(id) = pending.pop() {
        if descendants.insert(id) {
            pending.extend(index.children.get(&id).into_iter().flatten().copied());
        }
    }
    // Cover completion may address original Nodes which became owned after binding. Include
    // their new owner and its tree, so a concurrent split/merge cannot escape validation.
    for id in &target.source_node_ids {
        if let Some(owner) = index.owners.get(id) {
            descendants.insert(*owner);
        }
    }
    let states = descendants
        .iter()
        .filter_map(|id| index.nodes.get(id))
        .map(|node| {
            let mut files = index
                .videos
                .get(&node.id)
                .into_iter()
                .flatten()
                .map(|file| (file.id, file.node_id, &file.absolute_path, &file.file_name))
                .collect::<Vec<_>>();
            files.sort_by_key(|file| file.0);
            (
                operation_node_state(node),
                index.owners.get(&node.id),
                files,
            )
        })
        .collect::<Vec<_>>();
    let keys = sources
        .iter()
        .map(group_key)
        .collect::<std::collections::BTreeSet<_>>();
    let memberships = keys
        .into_iter()
        .map(|key| {
            let members = all_memberships.get(&key).cloned().unwrap_or_default();
            (key, members)
        })
        .collect::<Vec<_>>();
    let bytes = serde_json::to_vec(&(target.snapshot, states, memberships))
        .expect("Work dependencies serialization is infallible");
    target.snapshot = format!("{:x}", Sha256::digest(bytes));
    target
}

pub fn groups_from_index(
    connection: &Connection,
    index: &LogicalWorkIndex,
) -> AppResult<Vec<WorkGroup>> {
    let mut groups = group_works(index.sources(connection)?);
    let memberships = membership_index(index);
    for group in &mut groups {
        group.target = target_with_memberships(&group.sources, index, &memberships);
    }
    Ok(groups)
}

// The preference controls only All Resources presentation. Keep the complete mutation
// snapshot so a projected cross-library card opens the same logical work detail.
pub fn groups_visible_in_all(
    connection: &Connection,
    index: &LogicalWorkIndex,
    hidden: &HashSet<i64>,
) -> AppResult<Vec<WorkGroup>> {
    if hidden.is_empty() {
        return groups_from_index(connection, index);
    }
    Ok(groups_from_index(connection, index)?
        .into_iter()
        .filter_map(|group| {
            if group
                .sources
                .iter()
                .all(|node| !hidden.contains(&node.library_root_id))
            {
                return Some(group);
            }
            let visible = group
                .sources
                .iter()
                .filter(|node| !hidden.contains(&node.library_root_id))
                .cloned()
                .collect::<Vec<_>>();
            let mut projected = group_works(visible).pop()?;
            projected.target = group.target;
            Some(projected)
        })
        .collect())
}

#[cfg(test)]
pub fn populate_owned_content(
    database: &Database,
    detail: &mut NodeDetail,
    sources: &[MediaNode],
) -> AppResult<()> {
    database.read_snapshot(|connection| {
        let index = LogicalWorkIndex::load(connection)?;
        populate_owned_content_conn(connection, &index, detail, sources)
    })
}

/// Shared indexed-video ownership only, without children/resources/activity presentation.
fn populate_owned_video_files(
    index: &LogicalWorkIndex,
    detail: &mut NodeDetail,
    sources: &[MediaNode],
) {
    let mut seen = detail
        .media_files
        .iter()
        .map(|file| file.id)
        .collect::<HashSet<_>>();
    for source in sources.iter().filter(|source| source.node_type.is_work()) {
        for id in index
            .owned_nodes(source.id)
            .into_iter()
            .filter(|id| *id != source.id)
        {
            let node = &index.nodes[&id];
            let relative = Path::new(&node.absolute_path)
                .strip_prefix(&source.absolute_path)
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_else(|_| node.folder_name.clone());
            for file in index.videos.get(&id).into_iter().flatten() {
                if seen.insert(file.id) {
                    detail.nested_media_files.push(NestedMediaFile {
                        file: file.clone(),
                        source_node_id: source.id,
                        source_name: source.display_name.clone(),
                        relative_directory: relative.clone(),
                    });
                }
            }
        }
    }
    detail.nested_media_files.sort_by(|a, b| {
        crate::db::natural_cmp(&a.relative_directory, &b.relative_directory)
            .then_with(|| crate::db::natural_cmp(&a.file.file_name, &b.file.file_name))
            .then(a.file.id.cmp(&b.file.id))
    });
}

fn populate_owned_content_conn(
    connection: &Connection,
    index: &LogicalWorkIndex,
    detail: &mut NodeDetail,
    sources: &[MediaNode],
) -> AppResult<()> {
    let hydrated_children = detail.children.len();
    // Folder-mode details use the same owned statistics as catalogue details. A manual
    // independent descendant must not inflate its parent work's count or activity times.
    if detail.work_target.is_none() && detail.node.node_type.is_work() {
        if let Some(source) = index
            .sources_for_ids(connection, &[detail.node.id])?
            .into_iter()
            .find(|source| source.id == detail.node.id)
        {
            detail.node.total_video_count = source.total_video_count;
            detail.node.last_watched_at = source.last_watched_at;
            detail.node.latest_file_modified_at = source.latest_file_modified_at;
        }
    }
    populate_owned_video_files(index, detail, sources);
    for source in sources.iter().filter(|source| source.node_type.is_work()) {
        for owned in index.owned_nodes(source.id) {
            for child in index
                .children
                .get(&owned)
                .into_iter()
                .flatten()
                .filter_map(|id| index.nodes.get(id))
            {
                if child.node_type != NodeType::Ignored
                    && index.owners.get(&child.id) != Some(&source.id)
                    && !sources.iter().any(|source| source.id == child.id)
                    && !detail.children.iter().any(|node| node.id == child.id)
                {
                    detail.children.push(child.clone());
                }
            }
        }
        detail.expanded_folder_ids.extend(
            detail
                .children
                .iter()
                .filter(|child| {
                    index.owners.get(&child.id) == Some(&source.id)
                        && detail.nested_media_files.iter().any(|entry| {
                            entry.source_node_id == source.id
                                && Path::new(&entry.file.absolute_path)
                                    .starts_with(&child.absolute_path)
                        })
                })
                .map(|child| child.id),
        );
    }
    detail.recognition_warnings = index
        .warnings
        .iter()
        .filter_map(|id| index.nodes.get(id))
        .filter(|node| {
            sources.iter().any(|source| {
                node.library_root_id == source.library_root_id
                    && Path::new(&node.absolute_path).starts_with(&source.absolute_path)
            })
        })
        .cloned()
        .collect();
    // The lightweight ownership graph does not calculate recursive activity for unrelated
    // nodes. Preserve full presentation metadata on the small set returned by this detail.
    db::hydrate_file_modified_times_conn(connection, &mut detail.children[hydrated_children..])?;
    db::hydrate_file_modified_times_conn(connection, &mut detail.recognition_warnings)?;

    detail.expanded_folder_ids.sort_unstable();
    detail.expanded_folder_ids.dedup();
    detail.comic_books = Some(crate::comics::books_for_resources(
        connection,
        &detail.resource_files,
    )?);
    Ok(())
}

#[cfg(test)]
pub fn work_detail(database: &Database, node_id: i64) -> AppResult<NodeDetail> {
    database.read_snapshot(|connection| {
        let mut detail = work_detail_conn(connection, node_id)?;
        crate::card_progress::hydrate_nodes_conn(connection, &mut detail.children)?;
        Ok(detail)
    })
}

pub(crate) fn work_detail_conn(connection: &Connection, node_id: i64) -> AppResult<NodeDetail> {
    work_detail_index_conn(connection, node_id).map(|(detail, _)| detail)
}

fn work_detail_index_conn(
    connection: &Connection,
    node_id: i64,
) -> AppResult<(NodeDetail, LogicalWorkIndex)> {
    let mut index = LogicalWorkIndex::load_for_detail(connection)?;
    let missing = || "NODE_NOT_VISIBLE：作品已不在当前索引中，请刷新资源库。".to_string();
    let node = index
        .nodes
        .get(&node_id)
        .filter(|_| index.is_source(node_id))
        .ok_or_else(missing)?;
    let key = group_key(node);
    let ids = index
        .nodes
        .values()
        .filter(|node| index.is_source(node.id) && group_key(node) == key)
        .map(|node| node.id)
        .collect::<Vec<_>>();
    let sources = index.sources_for_ids(connection, &ids)?;
    index.load_detail_videos(connection, &ids, true)?;
    let mut group = group_works(sources)
        .into_iter()
        .next()
        .ok_or_else(missing)?;
    group.target = target_for_index(&group.sources, &index);
    let detail = detail_for_group_conn(connection, &index, group)?;
    Ok((detail, index))
}

fn detail_for_group_conn(
    connection: &Connection,
    index: &LogicalWorkIndex,
    group: WorkGroup,
) -> AppResult<NodeDetail> {
    let source_ids: HashSet<i64> = group.sources.iter().map(|node| node.id).collect();
    let mut detail = NodeDetail {
        video_sections: Vec::new(),
        comic_books: None,
        binding: group.node.binding.clone(),
        breadcrumbs: db::breadcrumbs_conn(connection, group.node.id, true)?,
        node: group.node,
        children: Vec::new(),
        media_files: Vec::new(),
        resource_files: Vec::new(),
        work_sources: None,
        work_target: Some(group.target),
        nested_media_files: Vec::new(),
        expanded_folder_ids: Vec::new(),
        recognition_warnings: Vec::new(),
    };
    for source in &group.sources {
        detail
            .media_files
            .extend(db::list_media_conn(connection, source.id)?);
        detail
            .resource_files
            .extend(db::list_resources_conn(connection, source.id)?);
        detail.children.extend(
            db::list_children_conn(connection, source.id)?
                .into_iter()
                .filter(|node| !source_ids.contains(&node.id)),
        );
    }
    detail.children.sort_by_key(|node| node.id);
    detail.children.dedup_by_key(|node| node.id);
    populate_owned_content_conn(connection, index, &mut detail, &group.sources)?;
    detail.work_sources = Some(group.sources);
    crate::video_presentation::populate(&mut detail);
    Ok(detail)
}

/// A bounded card batch shares one ownership graph and hydrates only selected video payloads.
/// This projection has no children/resources/operation target and never serves a mutation.
pub(crate) fn progress_detail_conn(
    c: &Connection,
    index: &mut LogicalWorkIndex,
    node_id: i64,
    aggregate: bool,
) -> AppResult<NodeDetail> {
    db::ensure_node_visible_conn(c, node_id)?;
    let node = db::get_node_conn(c, node_id)?;
    progress_detail_for_node_conn(c, index, node, aggregate)
}

pub(crate) fn progress_detail_for_node_conn(
    c: &Connection,
    index: &mut LogicalWorkIndex,
    node: MediaNode,
    aggregate: bool,
) -> AppResult<NodeDetail> {
    let sources = progress_sources(index, &node, aggregate)?;
    progress_detail_for_sources_conn(c, index, node, aggregate, sources)
}

pub(crate) fn progress_detail_for_sources_conn(
    c: &Connection,
    index: &mut LogicalWorkIndex,
    node: MediaNode,
    aggregate: bool,
    sources: Vec<MediaNode>,
) -> AppResult<NodeDetail> {
    let ids = sources.iter().map(|n| n.id).collect::<Vec<_>>();
    index.load_detail_videos(c, &ids, false)?;
    // A Series may own direct files without being a logical Work source in the graph.
    let missing = ids
        .iter()
        .filter(|id| !index.videos.contains_key(id))
        .copied()
        .collect::<Vec<_>>();
    for file in db::list_media_for_nodes_conn(c, &missing)? {
        index.videos.entry(file.node_id).or_default().push(file);
    }
    for id in missing {
        index.videos.entry(id).or_default();
    }
    let mut detail = NodeDetail {
        video_sections: Vec::new(),
        comic_books: None,
        node,
        children: Vec::new(),
        media_files: ids
            .iter()
            .flat_map(|id| index.videos.get(id).into_iter().flatten().cloned())
            .collect(),
        resource_files: Vec::new(),
        breadcrumbs: Vec::new(),
        binding: None,
        work_sources: None,
        work_target: None,
        nested_media_files: Vec::new(),
        expanded_folder_ids: Vec::new(),
        recognition_warnings: Vec::new(),
    };
    populate_owned_video_files(index, &mut detail, &sources);
    if aggregate || sources.len() > 1 {
        detail.work_sources = Some(sources);
    }
    Ok(detail)
}

pub(crate) fn progress_sources(
    index: &LogicalWorkIndex,
    node: &MediaNode,
    aggregate: bool,
) -> AppResult<Vec<MediaNode>> {
    progress_sources_indexed(index, node, aggregate, None)
}

/// Snapshot-local lookup with exactly the original progress-source predicate.
/// Mutation membership intentionally has different boundaries and is not reused.
pub(crate) fn progress_membership_index(index: &LogicalWorkIndex) -> BTreeMap<String, Vec<i64>> {
    let mut groups: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    for node in index.nodes.values().filter(|n| index.is_source(n.id)) {
        groups.entry(group_key(node)).or_default().push(node.id);
    }
    groups
}

pub(crate) fn progress_sources_indexed(
    index: &LogicalWorkIndex,
    node: &MediaNode,
    aggregate: bool,
    memberships: Option<&BTreeMap<String, Vec<i64>>>,
) -> AppResult<Vec<MediaNode>> {
    let node_id = node.id;
    let mut sources = if aggregate {
        if !index.is_source(node_id) {
            return Err("NODE_NOT_VISIBLE".into());
        }
        let key = group_key(node);
        let ids = if let Some(groups) = memberships {
            groups.get(&key).cloned().unwrap_or_default()
        } else {
            index
                .nodes
                .values()
                .filter(|n| index.is_source(n.id) && group_key(n) == key)
                .map(|n| n.id)
                .collect::<Vec<_>>()
        };
        ids.iter()
            .filter_map(|id| index.nodes.get(id))
            .cloned()
            .collect()
    } else {
        vec![node.clone()]
    };
    if !aggregate
        && node.node_type == crate::models::NodeType::Container
        && node.parent_node_id.is_some()
    {
        let mut pending = index.children.get(&node.id).cloned().unwrap_or_default();
        let mut visited = HashSet::new();
        while let Some(id) = pending.pop() {
            if !visited.insert(id) {
                continue;
            }
            let Some(child) = index.nodes.get(&id) else {
                continue;
            };
            if child.node_type == crate::models::NodeType::Ignored
                || child.library_root_id != node.library_root_id
            {
                continue;
            }
            if index.owners.get(&id) == Some(&id) {
                sources.push(child.clone());
            }
            pending.extend(index.children.get(&id).into_iter().flatten().copied());
        }
        sources.sort_by_key(|n| n.id);
    }
    sources.sort_by_key(|n| n.id);
    Ok(sources)
}

#[cfg(test)]
pub fn node_detail(database: &Database, node_id: i64) -> AppResult<NodeDetail> {
    database.read_snapshot(|connection| {
        let mut detail = node_detail_conn(connection, node_id, true)?;
        crate::card_progress::hydrate_nodes_conn(connection, &mut detail.children)?;
        Ok(detail)
    })
}

#[cfg(test)]
pub(crate) fn node_detail_conn(
    connection: &Connection,
    node_id: i64,
    lightweight: bool,
) -> AppResult<NodeDetail> {
    node_detail_index_conn(connection, node_id, lightweight).map(|(detail, _)| detail)
}

fn node_detail_index_conn(
    connection: &Connection,
    node_id: i64,
    lightweight: bool,
) -> AppResult<(NodeDetail, Option<LogicalWorkIndex>)> {
    db::ensure_node_visible_conn(connection, node_id)?;
    let node = db::get_node_conn(connection, node_id)?;
    let is_book = node.media_kind.is_book();
    let mut detail = NodeDetail {
        video_sections: Vec::new(),
        comic_books: None,
        binding: node.binding.clone(),
        node,
        children: if is_book {
            Vec::new()
        } else {
            db::list_children_conn(connection, node_id)?
        },
        media_files: db::list_media_conn(connection, node_id)?,
        resource_files: if is_book {
            Vec::new()
        } else {
            db::list_resources_conn(connection, node_id)?
        },
        breadcrumbs: db::breadcrumbs_conn(connection, node_id, true)?,
        work_sources: None,
        work_target: None,
        nested_media_files: Vec::new(),
        expanded_folder_ids: Vec::new(),
        recognition_warnings: Vec::new(),
    };
    if detail.node.media_kind.is_book() {
        crate::comics::populate_detail(connection, &mut detail)?;
        return Ok((detail, None));
    }
    let mut index = if lightweight {
        LogicalWorkIndex::load_for_root_detail(connection, detail.node.library_root_id)?
    } else {
        LogicalWorkIndex::load(connection)?
    };
    // The direct payload was already read above. Keep it in this snapshot's graph.
    let mut direct = detail.media_files.clone();
    direct.sort_by_key(|file| file.id);
    index.videos.insert(node_id, direct);
    index.load_detail_videos(connection, &[node_id], false)?;
    let sources = vec![detail.node.clone()];
    populate_owned_content_conn(connection, &index, &mut detail, &sources)?;
    crate::video_presentation::populate(&mut detail);
    Ok((detail, Some(index)))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationDetail {
    #[serde(flatten)]
    pub detail: NodeDetail,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub episode_watch: Option<crate::episode_watch::EpisodeWatchSnapshot>,
}

/// One SQLite snapshot supplies the visible detail, child card progress and watch
/// controls. Reuse ownership, selected files and parsed episode evidence throughout.
pub fn presentation_detail(
    database: &Database,
    node_id: i64,
    work_view: bool,
) -> AppResult<PresentationDetail> {
    database.read_snapshot(|c| {
        let (mut detail, graph) = if work_view {
            let (detail, graph) = work_detail_index_conn(c, node_id)?;
            (detail, Some(graph))
        } else {
            node_detail_index_conn(c, node_id, true)?
        };
        let mut progress = crate::card_progress::Hydrator::new(c, graph);
        progress.nodes(&mut detail.children, false)?;
        let episode_watch = if detail.node.media_kind.is_book() {
            None
        } else {
            Some(progress.episode_watch(&detail, work_view)?)
        };
        Ok(PresentationDetail {
            detail,
            episode_watch,
        })
    })
}

#[cfg(test)]
pub(crate) fn reference_detail(
    database: &Database,
    node_id: i64,
    aggregate: bool,
) -> AppResult<NodeDetail> {
    database.read_snapshot(|connection| {
        if !aggregate {
            let mut detail = node_detail_conn(connection, node_id, false)?;
            crate::card_progress::hydrate_nodes_conn(connection, &mut detail.children)?;
            return Ok(detail);
        }
        let index = LogicalWorkIndex::load(connection)?;
        let group = groups_from_index(connection, &index)?
            .into_iter()
            .find(|group| group.sources.iter().any(|source| source.id == node_id))
            .ok_or_else(|| "NODE_NOT_VISIBLE".to_string())?;
        let mut detail = detail_for_group_conn(connection, &index, group)?;
        crate::card_progress::hydrate_nodes_conn(connection, &mut detail.children)?;
        Ok(detail)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "Explicit read-only profiling of a verified synthetic database copy"]
    fn synthetic_detail_phase_profile() {
        let path = std::path::PathBuf::from(
            std::env::var("M2SHELF_SYNTHETIC_PROFILE_DB")
                .expect("explicit synthetic copy required"),
        );
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(".tmp")
            .canonicalize()
            .unwrap();
        assert!(path.canonicalize().unwrap().starts_with(&base));
        let db = Database::new(path);
        for title in ["Audit Large", "Wide Series"] {
            let detail = db
                .read_snapshot(|c| {
                    let mut roots = c
                        .prepare("SELECT path FROM library_roots")
                        .map_err(|e| e.to_string())?;
                    let paths = roots
                        .query_map([], |r| r.get::<_, String>(0))
                        .map_err(|e| e.to_string())?
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|e| e.to_string())?;
                    let prefix = base
                        .to_string_lossy()
                        .replace("\\\\?\\", "")
                        .replace('\\', "/")
                        .to_lowercase()
                        + "/";
                    assert!(paths.iter().all(|p| {
                        let normalized = p.replace('\\', "/").to_lowercase();
                        normalized.starts_with(&prefix)
                            && !normalized.split('/').any(|part| part == "..")
                    }));
                    let id = c
                        .query_row("SELECT id FROM nodes WHERE folder_name=?1", [title], |r| {
                            r.get::<_, i64>(0)
                        })
                        .map_err(|e| e.to_string())?;
                    let mut times = Vec::new();
                    macro_rules! phase {
                        ($label:expr,$body:expr) => {{
                            let start = std::time::Instant::now();
                            let value = $body;
                            times.push(($label, start.elapsed().as_secs_f64() * 1000.));
                            value
                        }};
                    }
                    let node = phase!("node", db::get_node_conn(c, id)?);
                    let children = phase!("children", db::list_children_conn(c, id)?);
                    let files = phase!("files", db::list_media_conn(c, id)?);
                    let resources = phase!("resources", db::list_resources_conn(c, id)?);
                    let breadcrumbs = phase!("breadcrumbs", db::breadcrumbs_conn(c, id, true)?);
                    let mut index = phase!(
                        "graph",
                        LogicalWorkIndex::load_for_root_detail(c, node.library_root_id)?
                    );
                    phase!("owned_files", index.load_detail_videos(c, &[id], false)?);
                    let sources = vec![node.clone()];
                    let mut detail = NodeDetail {
                        video_sections: Vec::new(),
                        comic_books: None,
                        binding: node.binding.clone(),
                        node,
                        children,
                        media_files: files,
                        resource_files: resources,
                        breadcrumbs,
                        work_sources: None,
                        work_target: None,
                        nested_media_files: Vec::new(),
                        expanded_folder_ids: Vec::new(),
                        recognition_warnings: Vec::new(),
                    };
                    phase!(
                        "owned_content",
                        populate_owned_content_conn(c, &index, &mut detail, &sources)?
                    );
                    phase!("sections", crate::video_presentation::populate(&mut detail));
                    phase!(
                        "card_summaries",
                        crate::card_progress::hydrate_nodes_conn(c, &mut detail.children)?
                    );
                    let bytes = phase!("serialization", serde_json::to_vec(&detail).unwrap()).len();
                    eprintln!(
                        "SYNTHETIC_PHASES {}",
                        serde_json::json!({"title":title,"times_ms":times,"bytes":bytes})
                    );
                    Ok(detail)
                })
                .unwrap();
            let start = std::time::Instant::now();
            let watch = db.episode_watch_snapshot(detail.node.id, false).unwrap();
            eprintln!(
                "SYNTHETIC_WATCH {}",
                serde_json::json!({"title":title,"ms":start.elapsed().as_secs_f64()*1000.,"rows":watch.rows.len()})
            );
        }
    }

    #[test]
    fn episode_titles_keep_seasons_and_numeric_sequels() {
        assert_eq!(episode_title("Steins;Gate - 01"), "Steins;Gate");
        assert_eq!(episode_title("Steins;Gate s02e03"), "Steins;Gate S02");
        assert_eq!(episode_title("Movie 2"), "Movie 2");
        assert_eq!(episode_title("Movie - 2026"), "Movie - 2026");
        assert_eq!(episode_title("Steins;Gate 0"), "Steins;Gate 0");
    }
}
