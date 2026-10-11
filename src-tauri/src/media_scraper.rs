//! One indexed-evidence entry for automatic matching, explicit rematching and manual prefill.
//! Filesystem metadata is imported only by the scanner; opening a detail never enumerates disk.
use crate::{
    db::{AppResult, Database},
    models::{LibraryMediaKind, MediaNode},
    title_extractor::{self, MatchEvidence},
};
pub use m2shelf_media_parser::{Kind, Parsed, Subtype};
pub fn kind(kind: LibraryMediaKind) -> Kind {
    match kind {
        LibraryMediaKind::Animation => Kind::Animation,
        LibraryMediaKind::LiveAction | LibraryMediaKind::Video => Kind::LiveAction,
        LibraryMediaKind::Comic => Kind::Comic,
        LibraryMediaKind::Ebook => Kind::Ebook,
        LibraryMediaKind::Doujin => Kind::Doujin,
        LibraryMediaKind::Artbook => Kind::Artbook,
    }
}
pub fn parse(node: &MediaNode, files: &[String]) -> Parsed {
    let kind = kind(node.media_kind);
    let mut parsed = m2shelf_media_parser::parse(&node.folder_name, kind);
    if node.display_name != node.folder_name {
        let manual = m2shelf_media_parser::parse(&node.display_name, kind);
        if !manual.display_title.is_empty() {
            parsed.search_titles.insert(
                0,
                m2shelf_media_parser::TitleCandidate {
                    title: manual.display_title.clone(),
                    source: "display-name".into(),
                },
            );
            parsed.display_title = manual.display_title;
        }
    }
    let sources = files
        .iter()
        .take(32)
        .map(|f| m2shelf_media_parser::parse(f, kind))
        .collect::<Vec<_>>();
    if matches!(
        node.media_kind,
        LibraryMediaKind::LiveAction | LibraryMediaKind::Video
    ) {
        let tv = sources
            .iter()
            .filter(|p| p.subtype == Subtype::Tv)
            .collect::<Vec<_>>();
        if !tv.is_empty() && sources.iter().all(|p| p.subtype != Subtype::Movie) {
            parsed.subtype = Subtype::Tv;
            parsed.season = parsed.season.or(tv[0].season);
            parsed.aired_date = parsed.aired_date.or_else(|| tv[0].aired_date.clone());
            if title_extractor::is_generic_title(&parsed.display_title)
                || parsed
                    .display_title
                    .to_ascii_lowercase()
                    .starts_with("season ")
            {
                parsed.display_title = tv[0].display_title.clone();
            }
            for p in tv {
                if !p.display_title.is_empty()
                    && !parsed
                        .search_titles
                        .iter()
                        .any(|t| t.title == p.display_title)
                {
                    parsed
                        .search_titles
                        .push(m2shelf_media_parser::TitleCandidate {
                            title: p.display_title.clone(),
                            source: "indexed-episode".into(),
                        });
                }
                parsed.episodes.extend(&p.episodes);
            }
        }
    }
    parsed.episodes.sort_unstable();
    parsed.episodes.dedup();
    parsed.episodes.truncate(128);
    parsed.search_titles.truncate(8);
    parsed
}
pub fn indexed_parse(db: &Database, node: &MediaNode, files: &[String]) -> AppResult<Parsed> {
    let mut parsed = parse(node, files);
    crate::scraper_metadata::enrich(&db.connect()?, node.id, &mut parsed)?;
    if node.display_name != node.folder_name {
        let manual = m2shelf_media_parser::parse(&node.display_name, kind(node.media_kind));
        if !manual.display_title.is_empty() {
            parsed.display_title = manual.display_title;
        }
    }
    Ok(parsed)
}
pub fn evidence(
    db: &Database,
    node: &MediaNode,
    files: &[String],
    parent: Option<&str>,
) -> AppResult<MatchEvidence> {
    let parsed = indexed_parse(db, node, files)?;
    let mut e = if node.media_kind.is_book() {
        crate::comics::match_evidence_with_books(
            &node.folder_name,
            &node.display_name,
            parent,
            files,
        )
    } else if node.media_kind == LibraryMediaKind::LiveAction {
        title_extractor::movie_evidence_for_node(node, files, &db.get_root(node.library_root_id)?)
    } else {
        title_extractor::build_match_evidence(&node.folder_name, &node.display_name, parent, files)
    };
    // The compatibility scorer keeps its proven edition/season/conflict rules. The shared parser
    // supplies the release boundary and contextual episode evidence for every provider.
    if !parsed.display_title.is_empty()
        && (!parsed.episodes.is_empty()
            || parsed.volume.is_some()
            || parsed.chapter.is_some()
            || parsed.search_titles.iter().any(|v| v.source == "embedded"))
    {
        if !node.media_kind.is_book()
            || (parsed.volume.is_none()
                && parsed.chapter.is_none()
                && !crate::comics::book_is_re(&e.primary_title))
        {
            e.primary_title = parsed.display_title.clone();
        }
        e.season_number = parsed.season.or(e.season_number);
    }
    if parsed.subtype == Subtype::Tv {
        e.edition_kind = title_extractor::EditionKind::Tv;
    }
    if parsed.year.is_some() {
        e.year = parsed.year;
        e.year_is_strong = true;
    }
    if parsed
        .warnings
        .iter()
        .any(|v| matches!(v.as_str(), "YEAR_CONFLICT" | "PROVIDER_ID_CONFLICT"))
    {
        e.evidence_quality = 0;
    }
    for title in parsed.search_titles {
        if title.title != e.primary_title && !e.alternate_titles.contains(&title.title) {
            e.alternate_titles.push(title.title);
        }
    }
    e.alternate_titles.truncate(8);
    Ok(e)
}
pub fn indexed_evidence(db: &Database, node: &MediaNode) -> AppResult<(Parsed, MatchEvidence)> {
    let parent = node
        .parent_node_id
        .and_then(|id| db.get_node(id).ok())
        .map(|n| n.display_name);
    let files = if node.media_kind.is_book() {
        crate::comic_reader::detail(db, node.id)?
            .into_iter()
            .take(32)
            .map(|b| b.display_name)
            .collect()
    } else {
        db.list_owned_video_names(node.id)?
    };
    Ok((
        indexed_parse(db, node, &files)?,
        evidence(db, node, &files, parent.as_deref())?,
    ))
}
