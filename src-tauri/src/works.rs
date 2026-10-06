//! A read-only catalogue over the existing path-owned index. Grouping never merges database
//! rows, so rescans, manual bindings, favorites and playback retain their original identities.
use rusqlite::Connection;
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

/// Strip explicit episode syntax only. A bare numeric sequel title remains distinct.
fn episode_title(value: &str) -> String {
    let normalized = value
        .split_whitespace()
        .map(|token| {
            let lower = token.to_ascii_lowercase();
            if let Some((season, episode)) = lower
                .strip_prefix('s')
                .and_then(|rest| rest.split_once('e'))
            {
                if !season.is_empty()
                    && !episode.is_empty()
                    && season.chars().all(|c| c.is_ascii_digit())
                    && episode.chars().all(|c| c.is_ascii_digit())
                {
                    return format!("S{season}");
                }
            }
            token.to_string()
        })
        .collect::<Vec<_>>()
        .join(" ");
    if let Some((title, episode)) = normalized.rsplit_once(" - ") {
        if !title.is_empty()
            && (1..=3).contains(&episode.len())
            && episode.chars().all(|c| c.is_ascii_digit())
        {
            return title.to_string();
        }
    }
    normalized
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
            if node.binding.is_none() {
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

fn populate_owned_content_conn(
    connection: &Connection,
    index: &LogicalWorkIndex,
    detail: &mut NodeDetail,
    sources: &[MediaNode],
) -> AppResult<()> {
    // Folder-mode details use the same owned statistics as catalogue details. A manual
    // independent descendant must not inflate its parent work's count or activity times.
    if detail.work_target.is_none() && detail.node.node_type.is_work() {
        if let Some(source) = index
            .sources(connection)?
            .into_iter()
            .find(|source| source.id == detail.node.id)
        {
            detail.node.total_video_count = source.total_video_count;
            detail.node.last_watched_at = source.last_watched_at;
            detail.node.latest_file_modified_at = source.latest_file_modified_at;
        }
    }
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
    detail.nested_media_files.sort_by(|a, b| {
        crate::db::natural_cmp(&a.relative_directory, &b.relative_directory)
            .then_with(|| crate::db::natural_cmp(&a.file.file_name, &b.file.file_name))
            .then(a.file.id.cmp(&b.file.id))
    });
    detail.expanded_folder_ids.sort_unstable();
    detail.expanded_folder_ids.dedup();
    Ok(())
}

pub fn work_detail(database: &Database, node_id: i64) -> AppResult<NodeDetail> {
    database.read_snapshot(|connection| work_detail_conn(connection, node_id))
}

fn work_detail_conn(connection: &Connection, node_id: i64) -> AppResult<NodeDetail> {
    let index = LogicalWorkIndex::load(connection)?;
    let group = groups_from_index(connection, &index)?
        .into_iter()
        .find(|group| group.sources.iter().any(|node| node.id == node_id))
        .ok_or_else(|| "NODE_NOT_VISIBLE：作品已不在当前索引中，请刷新资源库。".to_string())?;
    let source_ids: HashSet<i64> = group.sources.iter().map(|node| node.id).collect();
    let mut detail = NodeDetail {
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
    populate_owned_content_conn(connection, &index, &mut detail, &group.sources)?;
    detail.work_sources = Some(group.sources);
    Ok(detail)
}

pub fn node_detail(database: &Database, node_id: i64) -> AppResult<NodeDetail> {
    database.read_snapshot(|connection| {
        db::ensure_node_visible_conn(connection, node_id)?;
        let node = db::get_node_conn(connection, node_id)?;
        let mut detail = NodeDetail {
            binding: node.binding.clone(),
            node,
            children: db::list_children_conn(connection, node_id)?,
            media_files: db::list_media_conn(connection, node_id)?,
            resource_files: db::list_resources_conn(connection, node_id)?,
            breadcrumbs: db::breadcrumbs_conn(connection, node_id, true)?,
            work_sources: None,
            work_target: None,
            nested_media_files: Vec::new(),
            expanded_folder_ids: Vec::new(),
            recognition_warnings: Vec::new(),
        };
        let index = LogicalWorkIndex::load(connection)?;
        let sources = vec![detail.node.clone()];
        populate_owned_content_conn(connection, &index, &mut detail, &sources)?;
        Ok(detail)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn episode_titles_keep_seasons_and_numeric_sequels() {
        assert_eq!(episode_title("Steins;Gate - 01"), "Steins;Gate");
        assert_eq!(episode_title("Steins;Gate s02e03"), "Steins;Gate S02");
        assert_eq!(episode_title("Movie 2"), "Movie 2");
        assert_eq!(episode_title("Movie - 2026"), "Movie - 2026");
        assert_eq!(episode_title("Steins;Gate 0"), "Steins;Gate 0");
    }
}
