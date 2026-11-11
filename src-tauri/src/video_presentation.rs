//! Presentation sections derived only from already indexed files and existing parser evidence.
use crate::models::{MediaFile, MediaNode, NodeDetail};
use serde::Serialize;
use std::collections::HashMap;
use unicode_normalization::UnicodeNormalization;

/// Source-title protection is shared by tabs and watch identities. Parse each owner once.
pub(crate) struct SourceEvidence {
    pub parsed_owner: m2shelf_media_parser::Parsed,
    titles: Vec<String>,
}
fn normalized(value: &str) -> String {
    value
        .nfkc()
        .take(4096)
        .flat_map(char::to_lowercase)
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
impl SourceEvidence {
    pub fn new(owner: &MediaNode) -> Self {
        let parsed_owner = m2shelf_media_parser::watch_episodes(
            &owner.folder_name,
            crate::media_scraper::kind(owner.media_kind),
        );
        let mut titles = vec![
            normalized(&parsed_owner.display_title),
            normalized(&owner.folder_name),
        ];
        if owner.manual_display_name {
            titles.push(normalized(&owner.display_name));
        }
        if let Some(binding) = &owner.binding {
            titles.push(normalized(&binding.provider_title));
        }
        titles.retain(|s| {
            s.chars().filter(|c| c.is_alphanumeric()).count() >= 2
                && s.chars().any(char::is_alphabetic)
        });
        titles.sort_by_key(|s| std::cmp::Reverse(s.len()));
        titles.dedup();
        Self {
            parsed_owner,
            titles,
        }
    }
    pub fn supplemental(&self, value: &str) -> bool {
        if !crate::episode_watch::supplemental(value) {
            return false;
        }
        let mut remaining = normalized(value);
        for title in &self.titles {
            let latin_start = title.starts_with(|c: char| c.is_ascii_alphanumeric());
            let latin_end = title.ends_with(|c: char| c.is_ascii_alphanumeric());
            let ranges = remaining
                .match_indices(title)
                .filter_map(|(start, _)| {
                    let end = start + title.len();
                    let before = remaining[..start].chars().next_back();
                    let after = remaining[end..].chars().next();
                    ((!latin_start || before.is_none_or(|c| !c.is_ascii_alphanumeric()))
                        && (!latin_end || after.is_none_or(|c| !c.is_ascii_alphanumeric())))
                    .then_some(start..end)
                })
                .collect::<Vec<_>>();
            for range in ranges.into_iter().rev() {
                remaining.replace_range(range, " ");
            }
        }
        crate::episode_watch::supplemental(&remaining)
    }
    fn classify(&self, file: &MediaFile, owner: &MediaNode, relative: &str) -> VideoSection {
        if self.supplemental(&file.file_name)
            || relative.split(['/', '\\']).any(|s| self.supplemental(s))
        {
            return VideoSection::Sps;
        }
        let explicit = m2shelf_media_parser::explicit_season_episodes(&file.file_name);
        let direct_or_disc = relative.is_empty()
            || relative
                .split(['/', '\\'])
                .any(crate::logical_works::is_disc_name);
        // Direct/disc videos are Main unless the shared parser can identify season zero.
        // Unlabelled filenames need no title/edition/provider parse merely to prove that.
        if direct_or_disc
            && explicit.is_none()
            && !m2shelf_media_parser::has_season_episode_label(&file.file_name)
        {
            return VideoSection::Main;
        }
        let (season, episode, movie) = if let Some((season, episodes)) = explicit {
            (Some(season), !episodes.is_empty(), false)
        } else {
            let p = m2shelf_media_parser::watch_episodes(
                &file.file_name,
                crate::media_scraper::kind(owner.media_kind),
            );
            (
                p.season,
                !p.episodes.is_empty() && !p.warnings.iter().any(|w| w.contains("CONFLICT")),
                p.subtype == m2shelf_media_parser::Subtype::Movie,
            )
        };
        if season == Some(0) {
            VideoSection::Sps
        } else if direct_or_disc || episode || movie {
            VideoSection::Main
        } else {
            VideoSection::Other
        }
    }
}
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VideoSection {
    Main,
    Sps,
    Other,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoSectionEntry {
    pub file_id: i64,
    pub section: VideoSection,
}
#[cfg(test)]
pub fn classify(file: &MediaFile, owner: &MediaNode, relative: &str) -> VideoSection {
    SourceEvidence::new(owner).classify(file, owner, relative)
}
pub fn populate(detail: &mut NodeDetail) {
    let sources = detail
        .work_sources
        .as_deref()
        .unwrap_or(std::slice::from_ref(&detail.node));
    let owners: HashMap<_, _> = sources
        .iter()
        .map(|n| (n.id, (n, SourceEvidence::new(n))))
        .collect();
    let fallback = SourceEvidence::new(&detail.node);
    let classify = |file: &MediaFile, source, relative: &str| {
        let (owner, context) = owners
            .get(&source)
            .map(|(n, c)| (*n, c))
            .unwrap_or((&detail.node, &fallback));
        context.classify(file, owner, relative)
    };
    detail.video_sections = detail
        .media_files
        .iter()
        .filter_map(|file| {
            let section = classify(file, file.node_id, "");
            // Direct=Main and nested=Other are the existing DTO defaults. Send deviations
            // only, rather than repeating one JSON object for every ordinary episode.
            (section != VideoSection::Main).then_some(VideoSectionEntry {
                file_id: file.id,
                section,
            })
        })
        .chain(detail.nested_media_files.iter().filter_map(|entry| {
            let section = classify(&entry.file, entry.source_node_id, &entry.relative_directory);
            (section != VideoSection::Other).then_some(VideoSectionEntry {
                file_id: entry.file.id,
                section,
            })
        }))
        .collect();
}
