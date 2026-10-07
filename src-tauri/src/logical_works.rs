//! The directory classifier, catalogue and matcher share this index-derived ownership model.
//! No media paths or Node identities are changed and no persistent Work entity is introduced.
use std::collections::{HashMap, HashSet};

use rusqlite::Connection;

use crate::{
    db::{self, AppResult},
    models::{MediaFile, MediaNode, NodeType},
    scanner::{classify_directory, is_supplementary_directory_name},
    title_extractor::{extract_title_signals, is_generic_title, normalize_title_for_match},
};

pub fn is_disc_name(name: &str) -> bool {
    let value = name.trim().to_ascii_lowercase();
    ["volume", "disc", "disk", "dvd", "vol", "cd", "bd"]
        .iter()
        .any(|prefix| {
            value.strip_prefix(prefix).is_some_and(|rest| {
                let number = rest.trim_start_matches([' ', '.', '_', '-']);
                !number.is_empty() && number.chars().all(|c| c.is_ascii_digit())
            })
        })
}

fn useful_title(value: &str) -> bool {
    let normalized = normalize_title_for_match(value);
    normalized.chars().count() >= 3
        && !normalized.chars().all(|c| c.is_numeric())
        && !is_generic_title(value)
        && !is_disc_name(value)
        && !is_supplementary_directory_name(value)
}

pub struct LogicalWorkIndex {
    pub nodes: HashMap<i64, MediaNode>,
    pub children: HashMap<i64, Vec<i64>>,
    pub owners: HashMap<i64, i64>,
    pub warnings: Vec<i64>,
    content: HashMap<i64, Vec<i64>>,
    video_counts: HashMap<i64, i64>,
    folder_roots: HashSet<i64>,
    pub videos: HashMap<i64, Vec<MediaFile>>,
}

impl LogicalWorkIndex {
    pub fn load(connection: &Connection) -> AppResult<Self> {
        Self::load_with_classification(connection, false)
    }

    pub fn reclassify(connection: &Connection, root_id: Option<i64>) -> AppResult<()> {
        Self::load_with_classification(connection, true)?
            .persist_automatic_types(connection, root_id)
    }

    pub fn reclassify_related(
        connection: &Connection,
        ids: &[i64],
        include_self: bool,
    ) -> AppResult<()> {
        let index = Self::load_with_classification(connection, true)?;
        let mut related = HashSet::new();
        for id in ids {
            let mut current = if include_self {
                Some(*id)
            } else {
                index.nodes.get(id).and_then(|node| node.parent_node_id)
            };
            while let Some(id) = current {
                if !related.insert(id) {
                    break;
                }
                current = index.nodes.get(&id).and_then(|node| node.parent_node_id);
            }
        }
        for id in related {
            if let Some(node) = index.nodes.get(&id).filter(|node| {
                !node.manual_type_override
                    && node.node_type != NodeType::Ignored
                    && index.folder_roots.contains(&node.library_root_id)
            }) {
                connection
                    .execute(
                        "UPDATE nodes SET node_type=?1 WHERE id=?2 AND node_type<>?1",
                        rusqlite::params![node.node_type.as_db(), node.id],
                    )
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }

    fn load_with_classification(connection: &Connection, reclassify: bool) -> AppResult<Self> {
        let mut statement = connection
            .prepare(&format!("{} WHERE n.library_root_id IN (SELECT id FROM library_roots WHERE media_kind='VIDEO')",db::node_select()))
            .map_err(|e| e.to_string())?;
        let mut nodes = statement
            .query_map([], db::node_from_row)
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        db::hydrate_nodes_metadata_conn(connection, &mut nodes)?;
        let mut statement = connection
            .prepare("SELECT id FROM library_roots WHERE recognition_mode='FOLDER'")
            .map_err(|e| e.to_string())?;
        let folder_roots = statement
            .query_map([], |row| row.get(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<HashSet<i64>, _>>()
            .map_err(|e| e.to_string())?;
        let mut statement = connection
            .prepare(&format!(
                "SELECT {} FROM media_files f",
                db::media_columns("f")
            ))
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([], db::media_from_row)
            .map_err(|e| e.to_string())?;
        let mut videos: HashMap<i64, Vec<MediaFile>> = HashMap::new();
        for row in rows {
            let file = row.map_err(|e| e.to_string())?;
            videos.entry(file.node_id).or_default().push(file);
        }
        let mut index = Self {
            nodes: nodes.into_iter().map(|n| (n.id, n)).collect(),
            children: HashMap::new(),
            owners: HashMap::new(),
            warnings: Vec::new(),
            content: HashMap::new(),
            video_counts: HashMap::new(),
            folder_roots,
            videos,
        };
        for node in index.nodes.values() {
            if let Some(parent) = node.parent_node_id {
                index.children.entry(parent).or_default().push(node.id);
            }
        }
        for children in index.children.values_mut() {
            children.sort_unstable();
        }
        let automatic: Vec<_> = index
            .nodes
            .values()
            .filter(|n| {
                !n.manual_type_override
                    && n.node_type != NodeType::Ignored
                    && index.folder_roots.contains(&n.library_root_id)
            })
            .map(|n| n.id)
            .collect();
        for id in automatic {
            let node = &index.nodes[&id];
            let children = index.media_children(id);
            let supplements = children
                .iter()
                .filter(|child| {
                    !child.manual_type_override
                        && is_supplementary_directory_name(&child.folder_name)
                })
                .count() as i64;
            let bdmv = index.videos.get(&id).is_some_and(|files| {
                files.iter().any(|file| {
                    file.absolute_path
                        .replace('\\', "/")
                        .to_ascii_lowercase()
                        .contains("/bdmv/stream/")
                })
            });
            let kind = if index.is_multi_disc_work(id) {
                NodeType::AutoWork
            } else if reclassify {
                classify_directory(
                    node.direct_video_count,
                    children.len() as i64,
                    supplements,
                    bdmv,
                    false,
                )
            } else {
                node.node_type
            };
            index.nodes.get_mut(&id).unwrap().node_type = kind;
        }
        let roots = index
            .nodes
            .values()
            .filter(|n| n.parent_node_id.is_none())
            .map(|n| n.id)
            .collect::<Vec<_>>();
        let mut pending = roots.into_iter().map(|id| (id, None)).collect::<Vec<_>>();
        let mut visited = HashSet::new();
        while let Some((id, inherited)) = pending.pop() {
            if !visited.insert(id) {
                continue;
            }
            let node = &index.nodes[&id];
            if node.node_type == NodeType::Ignored {
                continue;
            }
            let inherited = inherited.filter(|owner| !index.independent_boundary(id, *owner));
            let owner = inherited.or_else(|| node.node_type.is_work().then_some(id));
            if let Some(owner) = owner {
                index.owners.insert(id, owner);
                index.content.entry(owner).or_default().push(id);
                *index.video_counts.entry(owner).or_default() += node.direct_video_count;
            }
            if is_disc_name(&node.folder_name)
                && node.binding.is_some()
                && !node.manual_type_override
                && inherited.is_none()
            {
                index.warnings.push(id);
            }
            if let Some(children) = index.children.get(&id) {
                pending.extend(children.iter().map(|child| (*child, owner)));
            }
        }
        index.warnings.sort_unstable();
        Ok(index)
    }

    fn media_children(&self, id: i64) -> Vec<&MediaNode> {
        self.children
            .get(&id)
            .into_iter()
            .flatten()
            .filter_map(|id| self.nodes.get(id))
            .filter(|node| node.node_type != NodeType::Ignored && node.total_video_count > 0)
            .collect()
    }

    fn independent_boundary(&self, id: i64, owner: i64) -> bool {
        let node = &self.nodes[&id];
        if node.manual_type_override {
            return true;
        }
        if let Some(binding) = &node.binding {
            return self.nodes[&owner].binding.as_ref().is_none_or(|parent| {
                (parent.provider_subject_type, parent.provider_subject_id)
                    != (binding.provider_subject_type, binding.provider_subject_id)
            });
        }
        false
    }

    fn is_multi_disc_work(&self, id: i64) -> bool {
        let parent = &self.nodes[&id];
        let title = extract_title_signals(&parent.display_name);
        if !useful_title(&title.cleaned_title) {
            return false;
        }
        let children = self.media_children(id);
        if !children.iter().any(|n| is_disc_name(&n.folder_name)) {
            return false;
        }
        if children.iter().any(|n| {
            n.manual_type_override
                || (!is_disc_name(&n.folder_name)
                    && !is_supplementary_directory_name(&n.folder_name))
        }) {
            return false;
        }
        let mut pending = children
            .iter()
            .filter(|n| is_disc_name(&n.folder_name))
            .map(|n| n.id)
            .collect::<Vec<_>>();
        let mut visited = HashSet::new();
        let mut video_title: Option<crate::title_extractor::TitleSignals> = None;
        while let Some(child_id) = pending.pop() {
            if !visited.insert(child_id) {
                return false;
            }
            if self.independent_boundary(child_id, id) {
                return false;
            }
            for descendant in self.media_children(child_id) {
                if !is_disc_name(&descendant.folder_name)
                    && !is_supplementary_directory_name(&descendant.folder_name)
                {
                    return false;
                }
                if is_disc_name(&descendant.folder_name) {
                    pending.push(descendant.id);
                }
            }
            for file in self.videos.get(&child_id).into_iter().flatten() {
                if file
                    .absolute_path
                    .replace('\\', "/")
                    .to_ascii_lowercase()
                    .contains("/bdmv/stream/")
                {
                    continue;
                }
                let signals = extract_title_signals(&file.file_name);
                if !useful_title(&signals.cleaned_title) {
                    continue;
                }
                let conflict =
                    |a: &crate::title_extractor::TitleSignals,
                     b: &crate::title_extractor::TitleSignals| {
                        a.season_number
                            .zip(b.season_number)
                            .is_some_and(|(a, b)| a != b)
                            || a.year.zip(b.year).is_some_and(|(a, b)| a != b)
                            || normalize_title_for_match(&a.series_title)
                                != normalize_title_for_match(&b.series_title)
                    };
                if conflict(&title, &signals)
                    || video_title
                        .as_ref()
                        .is_some_and(|old| conflict(old, &signals))
                {
                    return false;
                }
                video_title = Some(signals);
            }
        }
        true
    }

    pub fn is_source(&self, id: i64) -> bool {
        self.owners.get(&id) == Some(&id)
            && (!self.folder_roots.contains(&self.nodes[&id].library_root_id)
                || self.nodes[&id].manual_type_override
                || self.nodes[&id].binding.is_some()
                || !is_supplementary_directory_name(&self.nodes[&id].folder_name))
            && self.video_counts.get(&id).is_some_and(|count| *count > 0)
    }

    pub fn owned_nodes(&self, id: i64) -> Vec<i64> {
        let mut ids = self.content.get(&id).cloned().unwrap_or_default();
        ids.sort_unstable();
        ids
    }

    pub fn sources(&self, connection: &Connection) -> AppResult<Vec<MediaNode>> {
        let mut sources = self
            .nodes
            .values()
            .filter(|node| self.is_source(node.id))
            .cloned()
            .collect::<Vec<_>>();
        let mut times: HashMap<i64, (Option<String>, Option<String>)> = HashMap::new();
        let mut statement = connection.prepare("SELECT node_id,strftime('%Y-%m-%dT%H:%M:%fZ',MAX(modified)),strftime('%Y-%m-%dT%H:%M:%fZ',MAX(watched)) FROM (SELECT node_id,julianday(modified_at) AS modified,NULL AS watched FROM media_files UNION ALL SELECT node_id,julianday(modified_at),NULL FROM resource_files UNION ALL SELECT node_id,NULL,julianday(last_watched_at) FROM watch_history) GROUP BY node_id").map_err(|e|e.to_string())?;
        for row in statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?
        {
            let (id, modified, watched) = row.map_err(|e| e.to_string())?;
            if let Some(owner) = self.owners.get(&id) {
                let entry = times.entry(*owner).or_default();
                entry.0 = entry.0.clone().max(modified);
                entry.1 = entry.1.clone().max(watched);
            }
        }
        for source in &mut sources {
            source.total_video_count = self
                .video_counts
                .get(&source.id)
                .copied()
                .unwrap_or_default();
            let (modified, watched) = times.remove(&source.id).unwrap_or_default();
            source.latest_file_modified_at = modified;
            source.last_watched_at = watched;
        }
        Ok(sources)
    }

    pub fn persist_automatic_types(
        &self,
        connection: &Connection,
        root_id: Option<i64>,
    ) -> AppResult<()> {
        let mut statement = connection.prepare("UPDATE nodes SET node_type=?1 WHERE id=?2 AND manual_type_override=0 AND node_type<>?1")
            .map_err(|e| e.to_string())?;
        for node in self.nodes.values().filter(|node| {
            !node.manual_type_override
                && node.node_type != NodeType::Ignored
                && self.folder_roots.contains(&node.library_root_id)
                && root_id.is_none_or(|id| id == node.library_root_id)
        }) {
            statement
                .execute(rusqlite::params![node.node_type.as_db(), node.id])
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disc_names_require_a_complete_numbered_name() {
        for name in [
            "CD1", "CD 01", "Disc 1", "Disk-2", "BD1", "DVD1", "Vol.1", "Volume 1",
        ] {
            assert!(is_disc_name(name), "{name}");
        }
        for name in [
            "CDs",
            "Scans",
            "Disc",
            "Disc 1 Another Show",
            "BDRip",
            "My CD1",
        ] {
            assert!(!is_disc_name(name), "{name}");
        }
    }
}
