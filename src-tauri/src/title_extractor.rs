use std::collections::HashMap;

use unicode_normalization::UnicodeNormalization;

use crate::models::BangumiSearchPrefill;

const MAX_CONFIRMED_ALIASES: usize = 32;
const MAX_CONFIRMED_ALIAS_CHARS: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditionKind {
    Tv,
    Movie,
    Ova,
    Oad,
    Sp,
    Special,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanguageHint {
    Chinese,
    Japanese,
    Korean,
    Latin,
}

/// Structured, read-only evidence used by the automatic Bangumi matcher.
///
/// Every field is derived in memory. None of these values are written back to the Node's
/// `display_name`, `folder_name`, path, or source filesystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchEvidence {
    pub original_name: String,
    pub primary_title: String,
    /// Clean title taken from the real folder/file name when a user-edited display name became
    /// the primary title. Keeping it explicit lets automatic and manual search try the same
    /// useful local name before the three-query budget is exhausted.
    pub folder_title: Option<String>,
    pub alternate_titles: Vec<String>,
    pub parent_title: Option<String>,
    pub frequent_file_title: Option<String>,
    pub year: Option<i32>,
    /// Years written in a folder/display title are deliberate catalogue evidence. A year inferred
    /// only from release filenames can instead be a remaster/re-encode year, so it remains useful
    /// for ranking without becoming a hard rejection by itself.
    pub year_is_strong: bool,
    pub season_number: Option<u16>,
    pub edition_kind: EditionKind,
    pub language_hints: Vec<LanguageHint>,
    pub removed_noise: Vec<String>,
    /// 0-100 estimate of how specific and useful the local evidence is.
    pub evidence_quality: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TitleSignals {
    pub cleaned_title: String,
    pub series_title: String,
    pub year: Option<i32>,
    pub season_number: Option<u16>,
    pub edition_kind: EditionKind,
}

/// Produces temporary Bangumi search text only. It never writes names back to the database or
/// the source filesystem.
pub fn extract_search_keyword(raw_name: &str) -> String {
    let normalized = raw_name.nfkc().collect::<String>();
    let detected_year = detect_year(&normalized);
    let (candidate, _) = extract_keyword_with_mode(raw_name, false);
    let without_year = remove_year_marker(&candidate, detected_year);
    if without_year.is_empty() {
        candidate
    } else {
        without_year
    }
}

/// Movie queries share the existing cleaner. Locate a release boundary before removing codecs:
/// otherwise unknown groups, audio-channel fragments and language names can strand the year.
pub fn movie_query_title(raw: &str) -> (String, Option<i32>) {
    let normalized = normalize_release_separators(&normalize_release_brackets(
        &strip_known_extension(raw.trim()).nfkc().collect::<String>(),
    ));
    let (prefix, year) = movie_release_prefix(&normalized);
    let film_group = movie_broadcast_title(&normalized);
    let source = film_group.as_deref().unwrap_or(prefix);
    let (cleaned, _) = extract_keyword_with_mode(source, true);
    if year.is_some() {
        return (cleaned, year);
    }
    let tokens = cleaned.split_whitespace().collect::<Vec<_>>();
    let release_atom = |token: &str| {
        let atom = token.trim_matches(|c: char| !c.is_alphanumeric());
        let first = atom.split(['-', '_']).next().unwrap_or(atom);
        (!first.chars().all(|c| c.is_ascii_digit()) && is_technical_atom(first))
            || matches!(first.to_ascii_lowercase().as_str(), "dubbed" | "subbed")
    };
    // Search from the end: title numbers such as 1917 / 2001 survive before the release year.
    for index in (1..tokens.len()).rev() {
        let marker = tokens[index].trim_matches(['(', ')', '[', ']']);
        let year = marker
            .parse::<i32>()
            .ok()
            .filter(|y| (1900..=2099).contains(y));
        if let Some(year) = year {
            if tokens[index + 1..].iter().all(|token| release_atom(token)) {
                return (tokens[..index].join(" "), Some(year));
            }
        }
    }
    // The normal cleaner removes recognized codecs but cannot remove a compound AAC-GROUP.
    // Only trim such compounds after a recognizable technical boundary, never arbitrary words.
    let title = tokens
        .iter()
        .take_while(|token| !release_atom(token))
        .copied()
        .collect::<Vec<_>>()
        .join(" ");
    (if title.is_empty() { cleaned } else { title }, None)
}

fn movie_release_metadata(value: &str) -> bool {
    static MARKER: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    MARKER.get_or_init(|| regex::Regex::new(
        r"(?i)(?:^|[^a-z0-9])(?:(?:bd|hd)?(?:480|576|720|1080|1440|2160|4320)[pi]|\d{3,4}x\d{3,4}|blu[ ._-]?ray|bd(?:rip)?|web[ ._-]?(?:dl|rip)|hdtv|remux|[hx][ .]?26[45]|hevc|avc|uhd|4k)(?:$|[^a-z0-9]|[\p{Han}])"
    ).expect("movie release boundary")).is_match(value)
}

fn movie_release_prefix(value: &str) -> (&str, Option<i32>) {
    static YEAR: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let year = YEAR.get_or_init(|| {
        regex::Regex::new(r"(?:^|[\s._(\[\-])(?P<year>(?:19|20)\d{2})").expect("movie release year")
    });
    for capture in year
        .captures_iter(value)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        let found = capture.name("year").unwrap();
        if value[found.end()..]
            .chars()
            .next()
            .is_some_and(|c| !c.is_whitespace() && !"._)]-".contains(c))
        {
            continue;
        }
        let prefix = value[..found.start()].trim_end_matches([' ', '.', '-', '_', '(', '[']);
        let tail = value[found.end()..].trim_matches([' ', '.', '-', '_', ')', ']']);
        if !prefix.is_empty() && (tail.is_empty() || movie_release_metadata(tail)) {
            return (prefix, found.as_str().parse().ok());
        }
    }
    (value, None)
}

fn movie_broadcast_title(value: &str) -> Option<String> {
    if !movie_release_metadata(value) {
        return None;
    }
    let chars = value.chars().collect::<Vec<_>>();
    for (start, c) in chars.iter().enumerate() {
        if *c != '[' {
            continue;
        }
        let Some(end) = find_matching_square_bracket(&chars, start) else {
            continue;
        };
        let group = chars[start + 1..end].iter().collect::<String>();
        if let Some(title) = group.trim().strip_prefix("映画 ") {
            return Some(title.to_string());
        }
    }
    None
}

fn movie_title_candidates(raw: &str) -> (Vec<String>, Option<i32>) {
    let (title, year) = movie_query_title(raw);
    let parts = extract_parallel_title_candidates_with(&title, true);
    let mut titles = if parts.len() >= 2 { parts } else { vec![title] };
    // Nested bilingual broadcast titles carry the original and English name in one film group.
    // Actor/programme/channel brackets outside that group are not title aliases.
    let normalized = normalize_release_brackets(&raw.nfkc().collect::<String>());
    if let Some(group) = movie_broadcast_title(&normalized) {
        for part in extract_parallel_title_candidates_with(&movie_query_title(&group).0, true) {
            push_unique(&mut titles, part);
        }
    }
    for title in &mut titles {
        *title = title.trim_matches([' ', '(', ')', '[', ']']).to_string();
    }
    (titles, year)
}

pub fn build_movie_match_evidence(
    folder: &str,
    display: &str,
    files: &[String],
    mode: crate::models::LibraryRecognitionMode,
) -> MatchEvidence {
    let mut evidence = build_match_evidence(folder, display, None, files);
    let useful = |title: &str| {
        let lower = normalize_title_for_match(title);
        is_safe_movie_query(title)
            && !matches!(
                lower.as_str(),
                "movies"
                    | "movie"
                    | "films"
                    | "film"
                    | "video"
                    | "videos"
                    | "main"
                    | "电影"
                    | "電影"
                    | "影视"
                    | "影視"
            )
    };
    let folder_title = movie_title_candidates(folder);
    let file_title = files.first().map(|name| movie_title_candidates(name));
    let custom = (display.trim() != folder.trim()).then(|| movie_title_candidates(display));
    let file_first = matches!(mode, crate::models::LibraryRecognitionMode::VideoFile);
    let ordered = if file_first {
        [custom.as_ref(), file_title.as_ref(), Some(&folder_title)]
    } else {
        [custom.as_ref(), Some(&folder_title), file_title.as_ref()]
    };
    let candidates = ordered
        .into_iter()
        .flatten()
        .flat_map(|(titles, year)| {
            titles
                .iter()
                .filter(|title| useful(title))
                .map(move |title| (title.clone(), *year))
        })
        .collect::<Vec<_>>();
    if let Some((title, own_year)) = candidates.first() {
        evidence.primary_title = title.clone();
        evidence.folder_title = candidates
            .iter()
            .skip(1)
            .find(|(other, _)| other != title)
            .map(|v| v.0.clone());
        evidence.alternate_titles = candidates
            .iter()
            .map(|v| v.0.clone())
            .filter(|v| v != title)
            .collect();
        evidence.alternate_titles.dedup();
        evidence.alternate_titles.truncate(8);
        evidence.frequent_file_title = file_title
            .as_ref()
            .and_then(|v| v.0.iter().find(|title| useful(title)).cloned());
        let years = candidates
            .iter()
            .filter_map(|v| v.1)
            .collect::<std::collections::HashSet<_>>();
        evidence.year =
            own_year.or_else(|| (years.len() == 1).then(|| *years.iter().next().unwrap()));
        evidence.year_is_strong = evidence.year.is_some() && years.len() <= 1;
    } else {
        evidence.primary_title.clear();
        evidence.year = None;
        evidence.year_is_strong = false;
    }
    evidence.parent_title = None;
    evidence
}

pub fn movie_evidence_for_node(
    node: &crate::models::MediaNode,
    files: &[String],
    root: &crate::models::LibraryRoot,
) -> MatchEvidence {
    let mut evidence = build_movie_match_evidence(
        &node.folder_name,
        &node.display_name,
        files,
        root.recognition_mode.clone(),
    );
    // Flat file Nodes sit under the hidden Root, but their indexed source path still supplies
    // a specific enclosing movie folder for generic filenames. Never use the Library Root.
    if matches!(
        root.recognition_mode.clone(),
        crate::models::LibraryRecognitionMode::VideoFile
    ) && evidence.primary_title.is_empty()
    {
        if let Some(parent) = std::path::Path::new(&node.absolute_path)
            .parent()
            .filter(|p| *p != std::path::Path::new(&root.path))
        {
            if let Some(name) = parent.file_name().and_then(|v| v.to_str()) {
                evidence =
                    build_movie_match_evidence(name, name, files, root.recognition_mode.clone());
            }
        }
    }
    evidence
}

/// Builds the evidence consumed by the confidence matcher. `parent_name` is optional because a
/// Library Root or a stale parent can legitimately have no displayable parent Node.
pub fn build_match_evidence(
    folder_name: &str,
    display_name: &str,
    parent_name: Option<&str>,
    media_file_names: &[String],
) -> MatchEvidence {
    let original_name =
        if !display_name.trim().is_empty() && display_name.trim() != folder_name.trim() {
            display_name.trim().to_string()
        } else {
            folder_name.trim().to_string()
        };
    let primary_signals = extract_title_signals(&original_name);
    let folder_signals = extract_title_signals(folder_name);
    let display_signals = extract_title_signals(display_name);
    let parent_title = parent_name
        .map(extract_search_keyword)
        .filter(|value| is_useful_candidate(value));
    let ranked_file_titles = ranked_file_candidates(media_file_names);
    let media_file_year = dominant_media_file_year(media_file_names);
    let frequent_file_title = ranked_file_titles.first().cloned();
    let local_year = primary_signals.year.or(folder_signals.year);
    let local_year_is_ambiguous = if primary_signals.year.is_some() {
        has_multiple_distinct_year_markers(&original_name)
    } else {
        has_multiple_distinct_year_markers(folder_name)
    };
    let file_corrects_title_year = frequent_file_title.as_deref().is_some_and(|file_title| {
        file_title_corrects_title_shaped_year(
            &primary_signals.cleaned_title,
            local_year,
            file_title,
            media_file_year,
        )
    });
    let resolved_year = if file_corrects_title_year {
        media_file_year
    } else {
        local_year.or(media_file_year)
    };
    let primary_title = if file_corrects_title_year {
        frequent_file_title
            .clone()
            .unwrap_or_else(|| primary_signals.cleaned_title.clone())
    } else if is_safe_match_query(&primary_signals.cleaned_title) {
        primary_signals.cleaned_title.clone()
    } else {
        frequent_file_title
            .clone()
            .filter(|value| is_safe_match_query(value) || is_four_digit_numeric_title(value))
            .unwrap_or_else(|| primary_signals.cleaned_title.clone())
    };
    let folder_title = is_useful_candidate(&folder_signals.cleaned_title)
        .then(|| folder_signals.cleaned_title.clone())
        .filter(|value| {
            normalize_title_for_match(value) != normalize_title_for_match(&primary_title)
        });

    let mut alternate_titles = Vec::new();
    for source in [
        &primary_signals.cleaned_title,
        &folder_signals.cleaned_title,
        &display_signals.cleaned_title,
    ] {
        for candidate in extract_parallel_title_candidates(source) {
            push_unique(&mut alternate_titles, candidate);
        }
    }
    for candidate in [
        primary_signals.series_title.clone(),
        folder_signals.cleaned_title,
        folder_signals.series_title,
        display_signals.cleaned_title,
        display_signals.series_title,
    ] {
        if is_useful_candidate(&candidate) {
            push_unique(&mut alternate_titles, candidate);
        }
    }
    for value in ranked_file_titles.into_iter().take(3) {
        if is_useful_candidate(&value) {
            push_unique(&mut alternate_titles, value);
        }
    }
    if let Some(value) = parent_title.as_ref() {
        push_unique(&mut alternate_titles, value.clone());
    }
    for candidate in extract_embedded_title_candidates(folder_name)
        .into_iter()
        .chain(extract_embedded_title_candidates(display_name))
    {
        if is_useful_candidate(&candidate) {
            push_unique(&mut alternate_titles, candidate);
        }
    }
    alternate_titles.retain(|value| {
        normalize_title_for_match(value) != normalize_title_for_match(&primary_title)
    });

    let (_, mut removed_noise) = extract_keyword_with_mode(&original_name, true);
    deduplicate_strings(&mut removed_noise);
    let language_hints = detect_language_hints(&original_name);
    let evidence_quality = evidence_quality(
        &primary_title,
        frequent_file_title.as_deref(),
        parent_title.as_deref(),
    );

    MatchEvidence {
        original_name,
        primary_title,
        folder_title,
        alternate_titles,
        parent_title,
        frequent_file_title,
        year: resolved_year,
        year_is_strong: local_year.is_some()
            && !file_corrects_title_year
            && !local_year_is_ambiguous,
        season_number: primary_signals
            .season_number
            .or(folder_signals.season_number),
        edition_kind: if primary_signals.edition_kind != EditionKind::Unknown {
            primary_signals.edition_kind
        } else {
            folder_signals.edition_kind
        },
        language_hints,
        removed_noise,
        evidence_quality,
    }
}

/// Returns bounded local title evidence suitable for a user-confirmed alias observation.
///
/// Only the Node's own folder/display/file-derived titles are retained. A parent title is useful
/// for search context but is deliberately not learned as an alias for every child in that parent.
/// The caller stores these values only in M²Shelf's database after an explicit manual binding.
pub fn confirmed_alias_candidates(evidence: &MatchEvidence) -> Vec<String> {
    let parent_key = evidence
        .parent_title
        .as_deref()
        .map(normalize_title_for_match);
    let mut aliases = Vec::new();
    for candidate in std::iter::once(Some(evidence.primary_title.as_str()))
        .chain(std::iter::once(evidence.folder_title.as_deref()))
        .chain(std::iter::once(evidence.frequent_file_title.as_deref()))
        .chain(
            evidence
                .alternate_titles
                .iter()
                .map(|value| Some(value.as_str())),
        )
        .flatten()
    {
        if aliases.len() >= MAX_CONFIRMED_ALIASES
            || !is_useful_candidate(candidate)
            || confirmed_alias_has_conflicting_qualifier(candidate, evidence)
        {
            continue;
        }
        let normalized = normalize_title_for_match(candidate);
        if parent_key
            .as_ref()
            .is_some_and(|parent| parent == &normalized)
            && normalize_title_for_match(&evidence.primary_title) != normalized
            && evidence
                .folder_title
                .as_deref()
                .is_none_or(|title| normalize_title_for_match(title) != normalized)
            && evidence
                .frequent_file_title
                .as_deref()
                .is_none_or(|title| normalize_title_for_match(title) != normalized)
        {
            continue;
        }
        push_confirmed_alias_variants(&mut aliases, candidate, evidence);
    }
    aliases.truncate(MAX_CONFIRMED_ALIASES);
    aliases
}

/// Returns the exact alias spellings derived from the current Node's primary title. The automatic
/// matcher uses this subset only to preserve the stricter Container primary-title guard; all other
/// confirmed candidates remain exact alternate-title evidence.
pub(crate) fn confirmed_primary_alias_candidates(evidence: &MatchEvidence) -> Vec<String> {
    let mut aliases = Vec::new();
    push_confirmed_alias_variants(&mut aliases, &evidence.primary_title, evidence);
    aliases
}

fn push_confirmed_alias_variants(
    aliases: &mut Vec<String>,
    candidate: &str,
    evidence: &MatchEvidence,
) {
    if aliases.len() >= MAX_CONFIRMED_ALIASES
        || !is_useful_candidate(candidate)
        || confirmed_alias_has_conflicting_qualifier(candidate, evidence)
    {
        return;
    }
    if let Some(qualified) = qualified_confirmed_alias(candidate, evidence) {
        push_unique(aliases, qualified);
    }
    if aliases.len() < MAX_CONFIRMED_ALIASES {
        if let Some(bounded) = bounded_confirmed_alias(candidate) {
            push_unique(aliases, bounded);
        }
    }
}

fn confirmed_alias_has_conflicting_qualifier(candidate: &str, evidence: &MatchEvidence) -> bool {
    let signals = extract_title_signals(candidate);
    let season_conflicts = evidence
        .season_number
        .zip(signals.season_number)
        .is_some_and(|(expected, actual)| expected != actual);
    let year_conflicts = evidence.year_is_strong
        && evidence
            .year
            .zip(signals.year)
            .is_some_and(|(expected, actual)| expected != actual);
    season_conflicts || year_conflicts
}

/// Reattaches only structured qualifiers which ordinary provider-query cleanup deliberately
/// removes. The base still comes from the cleaned Node-owned evidence, so release-group and
/// encoding noise never becomes a confirmed alias.
fn qualified_confirmed_alias(candidate: &str, evidence: &MatchEvidence) -> Option<String> {
    let signals = extract_title_signals(candidate);
    let mut qualifiers = Vec::with_capacity(2);
    if let Some(season) = evidence.season_number {
        qualifiers.push(format!("S{season}"));
    }
    if evidence.year_is_strong {
        if let Some(year) = evidence.year {
            qualifiers.push(year.to_string());
        }
    }
    if qualifiers.is_empty() {
        return None;
    }

    let base = remove_semantic_markers(
        &signals.cleaned_title,
        signals.season_number,
        EditionKind::Unknown,
    );
    let suffix = qualifiers.join(" ");
    let base_limit = MAX_CONFIRMED_ALIAS_CHARS.checked_sub(suffix.chars().count() + 1)?;
    let base = base
        .trim()
        .chars()
        .take(base_limit)
        .collect::<String>()
        .trim()
        .to_string();
    if !is_useful_candidate(&base) {
        return None;
    }
    bounded_confirmed_alias(&format!("{base} {suffix}"))
}

fn bounded_confirmed_alias(candidate: &str) -> Option<String> {
    let bounded = candidate
        .trim()
        .chars()
        .take(MAX_CONFIRMED_ALIAS_CHARS)
        .collect::<String>();
    (is_useful_candidate(&bounded)
        && (is_safe_match_query(&bounded) || is_four_digit_numeric_title(&bounded)))
    .then_some(bounded)
}

/// Extracts comparable title, year, season, and edition signals from either a local name or an
/// official provider title. This is intentionally public so the scorer can use exactly the same
/// semantics on both sides.
pub fn extract_title_signals(value: &str) -> TitleSignals {
    let nfkc = value.nfkc().collect::<String>();
    let year = detect_year(&nfkc);
    let season_number = detect_season(&nfkc);
    let edition_kind = detect_edition_kind(&nfkc);
    let (semantic_title, _) = extract_keyword_with_mode(&nfkc, true);
    let cleaned_title = remove_year_marker(&semantic_title, year);
    let series_title = remove_semantic_markers(&cleaned_title, season_number, edition_kind);
    TitleSignals {
        cleaned_title: if cleaned_title.is_empty() {
            extract_search_keyword(&nfkc)
        } else {
            cleaned_title
        },
        series_title,
        year,
        season_number,
        edition_kind,
    }
}

/// NFKC title normalization shared by query caches, exact matching, and similarity scoring.
pub fn normalize_title_for_match(value: &str) -> String {
    let nfkc = value.nfkc().collect::<String>().to_lowercase();
    let words = nfkc
        .split(|character: char| !character.is_alphanumeric() && character != '&')
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    words
        .into_iter()
        .map(|word| if word == "&" { "and" } else { word })
        .collect::<String>()
}

pub fn build_search_prefill(
    folder_name: &str,
    display_name: &str,
    media_file_names: &[String],
) -> BangumiSearchPrefill {
    let folder_candidate = extract_search_keyword(folder_name);
    let display_candidate = extract_search_keyword(display_name);
    let mut candidates = Vec::new();
    if is_useful_candidate(&folder_candidate) {
        push_unique(&mut candidates, folder_candidate.clone());
    }
    if display_name.trim() != folder_name.trim() && is_useful_candidate(&display_candidate) {
        push_unique(&mut candidates, display_candidate.clone());
    }
    for candidate in ranked_file_candidates(media_file_names).into_iter().take(3) {
        push_unique(&mut candidates, candidate);
    }
    for candidate in extract_embedded_title_candidates(folder_name)
        .into_iter()
        .chain(extract_embedded_title_candidates(display_name))
    {
        if is_useful_candidate(&candidate) {
            push_unique(&mut candidates, candidate);
        }
    }
    if candidates.is_empty() {
        let fallback = folder_name.trim().to_string();
        if !fallback.is_empty() {
            candidates.push(fallback);
        }
    }

    BangumiSearchPrefill {
        original_name: folder_name.to_string(),
        extracted_name: candidates
            .first()
            .cloned()
            .unwrap_or_else(|| folder_name.trim().to_string()),
        candidates,
    }
}

fn ranked_file_candidates(media_file_names: &[String]) -> Vec<String> {
    let mut frequency: HashMap<String, (String, usize)> = HashMap::new();
    for file_name in media_file_names {
        let candidate = extract_media_file_keyword(file_name);
        if is_useful_candidate(&candidate)
            && numeric_file_title_has_independent_year(file_name, &candidate)
        {
            let key = normalize_title_for_match(&candidate);
            frequency
                .entry(key)
                .and_modify(|entry| entry.1 += 1)
                .or_insert((candidate, 1));
        }
    }
    let mut candidates = frequency.into_values().collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .1
            .cmp(&left.1)
            .then_with(|| right.0.chars().count().cmp(&left.0.chars().count()))
            .then_with(|| left.0.cmp(&right.0))
    });
    candidates.into_iter().map(|entry| entry.0).collect()
}

/// Uses file-name dates only as a fallback when the Node names carry no year. A single movie
/// file is useful evidence; for multi-file works, conflicting years must have one unique winner
/// so episode batches cannot introduce an arbitrary year conflict.
fn dominant_media_file_year(media_file_names: &[String]) -> Option<i32> {
    let mut frequency = HashMap::<i32, usize>::new();
    for file_name in media_file_names {
        let normalized = file_name.nfkc().collect::<String>();
        if let Some(year) = detect_year(&normalized) {
            let candidate = extract_media_file_keyword(file_name);
            if !numeric_file_title_has_independent_year(file_name, &candidate) {
                continue;
            }
            *frequency.entry(year).or_default() += 1;
        }
    }
    let mut ranked = frequency.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    let (year, count) = ranked.first().copied()?;
    if ranked
        .get(1)
        .is_some_and(|(_, next_count)| *next_count == count)
    {
        None
    } else {
        Some(year)
    }
}

fn numeric_file_title_has_independent_year(file_name: &str, candidate: &str) -> bool {
    if !is_four_digit_numeric_title(candidate) {
        return true;
    }
    let normalized = file_name.nfkc().collect::<String>();
    let Some(year) = detect_year(&normalized) else {
        return false;
    };
    let Ok(title_number) = normalize_title_for_match(candidate).parse::<i32>() else {
        return false;
    };
    title_number != year || count_year_occurrences(&normalized, year) >= 2
}

fn file_title_corrects_title_shaped_year(
    local_title: &str,
    local_year: Option<i32>,
    file_title: &str,
    file_year: Option<i32>,
) -> bool {
    let (Some(local_year), Some(file_year)) = (local_year, file_year) else {
        return false;
    };
    if local_year == file_year {
        return false;
    }
    let normalized_file = normalize_title_for_match(file_title);
    if is_four_digit_numeric_title(local_title)
        && normalized_file == normalize_title_for_match(local_title)
    {
        return true;
    }

    let year = local_year.to_string();
    let mut removed = false;
    let without_local_year = clean_join(
        file_title
            .split_whitespace()
            .filter_map(|token| {
                if !removed && trim_numeric(token) == year {
                    removed = true;
                    None
                } else {
                    Some(token.to_string())
                }
            })
            .collect(),
    );
    removed
        && normalize_title_for_match(&without_local_year) == normalize_title_for_match(local_title)
}

/// File names frequently contain a useful romanized/original title even when the folder uses a
/// fan-created English translation that Bangumi does not index. Keep an explicit season marker
/// from a file name after the ordinary keyword cleaner removes episode/encode noise, so this
/// independent local source can become one of the three bounded official queries.
fn extract_media_file_keyword(file_name: &str) -> String {
    let mut candidate = extract_search_keyword(file_name);
    let source_season = detect_season(file_name);
    if let Some(season) = source_season.filter(|season| *season > 1) {
        if detect_season(&candidate).is_none() && is_useful_candidate(&candidate) {
            candidate.push_str(&format!(" S{season}"));
        }
    }
    candidate
}

fn extract_keyword_with_mode(raw_name: &str, preserve_semantics: bool) -> (String, Vec<String>) {
    // Keep the advisory/manual prefill byte semantics stable (for example Japanese `！`). The
    // structured matcher requests NFKC through `extract_title_signals` before entering this path.
    let owned;
    let source = if preserve_semantics {
        owned = raw_name.nfkc().collect::<String>();
        owned.as_str()
    } else {
        raw_name
    };
    let without_extension = strip_known_extension(source.trim());
    let normalized_brackets = normalize_release_brackets(without_extension);
    let normalized_separators = normalize_release_separators(&normalized_brackets);
    let mut output = String::with_capacity(normalized_separators.len());
    let mut removed_noise = Vec::new();
    let characters = normalized_separators.chars().collect::<Vec<_>>();
    let mut index = 0;
    let mut visible_text_seen = false;

    while index < characters.len() {
        if characters[index] == '[' {
            if let Some(end) = find_matching_square_bracket(&characters, index) {
                let group = characters[index + 1..end].iter().collect::<String>();
                let group = group.trim();
                if group.contains('[') {
                    let (nested_title, nested_noise) =
                        extract_keyword_with_mode(group, preserve_semantics);
                    removed_noise.extend(nested_noise);
                    if !nested_title.trim().is_empty() {
                        push_separated(&mut output, &nested_title);
                        visible_text_seen = true;
                    } else if !group.is_empty() {
                        removed_noise.push(group.to_string());
                    }
                    index = end + 1;
                    continue;
                }
                let release_prefix = !visible_text_seen
                    && (looks_like_release_group(group)
                        || (looks_like_contextual_release_identity(group)
                            && has_following_title_candidate(&characters, end + 1)));
                let semantic = preserve_semantics && is_semantic_group(group);
                if !release_prefix && (!is_technical_group(group) || semantic) {
                    push_separated(&mut output, group);
                    visible_text_seen |= !group.is_empty();
                } else if !group.is_empty() {
                    removed_noise.push(group.to_string());
                }
                index = end + 1;
                continue;
            }
        }

        let character = characters[index];
        output.push(if character == '_' { ' ' } else { character });
        visible_text_seen |= !character.is_whitespace();
        index += 1;
    }

    let (cleaned, word_noise) = remove_technical_words(&output, preserve_semantics);
    removed_noise.extend(word_noise);
    (cleaned, removed_noise)
}

fn find_matching_square_bracket(characters: &[char], start: usize) -> Option<usize> {
    if characters.get(start) != Some(&'[') {
        return None;
    }
    let mut depth = 0usize;
    for (index, character) in characters.iter().enumerate().skip(start) {
        match character {
            '[' => depth = depth.saturating_add(1),
            ']' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// Scene-style movie and television releases commonly use periods as word separators, for
/// example `The.Sword.of.Doom.1966.1080p`. Scoring ignores punctuation, but sending the whole
/// dotted value as one token prevents the year and encode noise from being removed first. Only
/// normalize a multi-period name when one segment is recognizable release metadata, preserving
/// ordinary dotted titles that do not look like a release name.
fn normalize_release_separators(value: &str) -> String {
    let parts = value.split('.').collect::<Vec<_>>();
    let normalize_periods = parts.len() >= 3
        && parts.iter().any(|part| {
            let part = part.trim();
            detect_year(part).is_some()
                || (is_technical_atom(part)
                    && !part.chars().all(|character| character.is_ascii_digit()))
        });
    value
        .chars()
        .map(|character| {
            if character == '_' || (normalize_periods && character == '.') {
                ' '
            } else {
                character
            }
        })
        .collect()
}

/// Chinese movie release names frequently use full-width square brackets for the publishing
/// site, HDR/audio variants, and subtitle labels. Normalize only square-bracket variants here so
/// they pass through the same bounded group classifier as ordinary square brackets; quotation
/// marks such as 「...」 remain untouched because they can be part of a real title.
fn normalize_release_brackets(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '【' | '［' | '〔' => '[',
            '】' | '］' | '〕' => ']',
            _ => character,
        })
        .collect()
}

fn strip_known_extension(value: &str) -> &str {
    let Some((stem, extension)) = value.rsplit_once('.') else {
        return value;
    };
    let extension = extension.to_ascii_lowercase();
    const FILE_EXTENSIONS: &[&str] = &[
        "mkv", "mp4", "m4v", "avi", "mov", "webm", "ts", "m2ts", "ass", "ssa", "srt", "sup", "vtt",
        "flac", "wav", "mp3", "aac", "jpg", "jpeg", "png", "webp",
    ];
    if FILE_EXTENSIONS.contains(&extension.as_str()) {
        stem.trim_end()
    } else {
        value
    }
}

fn push_separated(target: &mut String, value: &str) {
    if value.is_empty() {
        return;
    }
    if !target.is_empty() && !target.ends_with(char::is_whitespace) {
        target.push(' ');
    }
    target.push_str(&value.replace('_', " "));
    target.push(' ');
}

fn looks_like_release_group(value: &str) -> bool {
    let lower = value.to_lowercase();
    let compact = lower
        .chars()
        .filter(|character| character.is_alphanumeric())
        .collect::<String>();
    lower.contains("studio")
        || lower.contains("raws")
        || lower.contains("fansub")
        || lower.contains("subgroup")
        || lower.contains("字幕")
        || lower.contains("压制")
        || lower.contains("壓制")
        || lower.contains("发布")
        || lower.contains("發佈")
        || lower.contains("發布")
        || lower.contains("www ")
        || lower.starts_with("www.")
        || (lower.contains('&')
            && lower
                .chars()
                .any(|character| character.is_ascii_alphabetic()))
        || matches!(
            compact.as_str(),
            "airota"
                | "vcb"
                | "dbd"
                | "caso"
                | "ktxp"
                | "dmhy"
                | "ani"
                | "beansub"
                | "fzsd"
                | "lolihouse"
                | "reinforce"
                | "moozzi2"
                | "nekomoe"
                | "nekomoekissaten"
                | "nanoalchemist"
                | "uhawings"
                | "lilithraws"
                | "beatriceraws"
        )
}

/// Release folders often use an unregistered short team identity in the first bracket followed
/// by a real title in the next bracket or in plain text. Limit the heuristic to strong identity
/// shapes and require a following title candidate, so a bracketed title such as
/// `[STEINS;GATE][1080p]` remains intact.
fn looks_like_contextual_release_identity(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() || !trimmed.is_ascii() {
        return false;
    }
    let alphanumeric = trimmed
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>();
    if !(2..=24).contains(&alphanumeric.len())
        || !alphanumeric
            .chars()
            .any(|character| character.is_ascii_alphabetic())
    {
        return false;
    }
    let uppercase_identity = trimmed
        .chars()
        .filter(|character| character.is_ascii_alphabetic())
        .all(|character| character.is_ascii_uppercase())
        && trimmed
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'));
    let camel_humps = trimmed
        .chars()
        .filter(|character| character.is_ascii_uppercase())
        .count();
    let compact_camel_identity = !trimmed.contains(char::is_whitespace)
        && !trimmed.contains([':', ';', '!', '?', '/'])
        && camel_humps >= 2;
    uppercase_identity || compact_camel_identity
}

fn has_following_title_candidate(characters: &[char], mut index: usize) -> bool {
    while index < characters.len() {
        while index < characters.len() && characters[index].is_whitespace() {
            index += 1;
        }
        if index >= characters.len() {
            return false;
        }
        if characters[index] == '[' {
            let Some(end) = find_matching_square_bracket(characters, index) else {
                return false;
            };
            let group = characters[index + 1..end].iter().collect::<String>();
            if !group.trim().is_empty()
                && !looks_like_release_group(&group)
                && !is_technical_group(&group)
                && !is_semantic_group(&group)
            {
                return true;
            }
            index = end + 1;
            continue;
        }

        let end = characters[index..]
            .iter()
            .position(|character| *character == '[')
            .map_or(characters.len(), |relative| index + relative);
        let plain = characters[index..end].iter().collect::<String>();
        let (cleaned, _) = remove_technical_words(&plain.replace('_', " "), false);
        if is_useful_candidate(&cleaned) {
            return true;
        }
        index = end;
    }
    false
}

fn extract_embedded_title_candidates(raw_name: &str) -> Vec<String> {
    let source = strip_known_extension(raw_name.trim());
    let normalized_brackets = normalize_release_brackets(source);
    let characters = normalized_brackets.chars().collect::<Vec<_>>();
    let mut candidates = Vec::new();
    let mut index = 0;
    let mut visible_text_seen = false;
    while index < characters.len() {
        if characters[index] == '[' {
            let Some(end) = find_matching_square_bracket(&characters, index) else {
                break;
            };
            let group = characters[index + 1..end].iter().collect::<String>();
            let contextual_release = !visible_text_seen
                && (looks_like_release_group(&group)
                    || (looks_like_contextual_release_identity(&group)
                        && has_following_title_candidate(&characters, end + 1)));
            if !contextual_release && !is_technical_group(&group) && !is_semantic_group(&group) {
                let (candidate, _) = remove_technical_words(&group.replace('_', " "), false);
                if is_useful_candidate(&candidate) {
                    push_unique(&mut candidates, candidate);
                }
            }
            index = end + 1;
            continue;
        }
        visible_text_seen |= !characters[index].is_whitespace();
        index += 1;
    }
    candidates
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TitleScriptFamily {
    Latin,
    EastAsian,
}

/// A common movie release carries the official/localized CJK title directly beside an English
/// title, for example 了不起的盖茨比The Great Gatsby. Searching that concatenation fails even
/// though Bangumi indexes each title independently. Preserve the combined value as primary
/// evidence, but also expose each script run as an alternate title for the existing bounded
/// three-query recall and unchanged confidence scorer.
fn extract_parallel_title_candidates(value: &str) -> Vec<String> {
    extract_parallel_title_candidates_with(value, false)
}
fn extract_parallel_title_candidates_with(value: &str, movie: bool) -> Vec<String> {
    let mut candidates = Vec::new();
    let mut buffer = String::new();
    let mut active_family = None;
    let mut saw_latin = false;
    let mut saw_east_asian = false;

    for character in value.chars() {
        let family = title_script_family(character);
        match family {
            Some(TitleScriptFamily::Latin) => saw_latin = true,
            Some(TitleScriptFamily::EastAsian) => saw_east_asian = true,
            None => {}
        }
        if let (Some(active), Some(next)) = (active_family, family) {
            if active != next {
                push_parallel_title_candidate(&mut candidates, &buffer, movie);
                buffer.clear();
                active_family = Some(next);
            }
        } else if active_family.is_none() && family.is_some() {
            active_family = family;
        }
        buffer.push(character);
    }
    push_parallel_title_candidate(&mut candidates, &buffer, movie);

    if saw_latin && saw_east_asian {
        candidates
    } else {
        Vec::new()
    }
}

fn title_script_family(character: char) -> Option<TitleScriptFamily> {
    if character.is_ascii_alphabetic() {
        return Some(TitleScriptFamily::Latin);
    }
    let code = character as u32;
    ((0x3400..=0x9fff).contains(&code)
        || (0x3040..=0x30ff).contains(&code)
        || (0x31f0..=0x31ff).contains(&code)
        || (0xac00..=0xd7af).contains(&code))
    .then_some(TitleScriptFamily::EastAsian)
}

fn push_parallel_title_candidate(candidates: &mut Vec<String>, value: &str, movie: bool) {
    let candidate = value
        .trim_matches(|character: char| {
            character.is_whitespace()
                || matches!(
                    character,
                    '-' | '_' | '.' | '·' | '|' | ':' | '/' | '[' | ']' | '(' | ')'
                )
        })
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if is_safe_match_query(&candidate) || movie && is_safe_movie_query(&candidate) {
        push_unique(candidates, candidate);
    }
}

fn is_technical_group(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() || is_crc(trimmed) || looks_like_technical_phrase_group(trimmed) {
        return true;
    }
    let atoms = trimmed
        .split(|character: char| {
            character.is_whitespace()
                || matches!(character, '_' | '-' | '+' | ',' | '/' | '\\' | '&')
        })
        .filter(|atom| !atom.is_empty())
        .collect::<Vec<_>>();
    !atoms.is_empty() && atoms.iter().all(|atom| is_technical_atom(atom))
}

/// Some release metadata is a natural-language phrase rather than a sequence of codec atoms.
/// Keep this deliberately scoped to bracket groups; these phrases should never remove matching
/// words from an unbracketed title.
fn looks_like_technical_phrase_group(value: &str) -> bool {
    let lower = value.nfkc().collect::<String>().to_lowercase();
    [
        "字幕",
        "中字",
        "简繁",
        "簡繁",
        "内封",
        "內封",
        "内嵌",
        "內嵌",
        "配音",
        "国语",
        "國語",
        "粤语",
        "粵語",
        "双语",
        "雙語",
        "杜比视界",
        "杜比視界",
        "双版本",
        "雙版本",
        "原盘",
        "原盤",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
        || ["subbed", "dubbed", "multi-sub", "multisub"]
            .iter()
            .any(|marker| lower.contains(marker))
}

fn is_semantic_group(value: &str) -> bool {
    detect_season(value).is_some() || detect_edition_kind(value) != EditionKind::Unknown
}

fn is_technical_atom(value: &str) -> bool {
    let lower = value
        .trim_matches(|character: char| !character.is_alphanumeric() && character != '#')
        .to_ascii_lowercase()
        .replace('\u{00d7}', "x");
    if lower.is_empty() || lower.chars().all(|character| character.is_ascii_digit()) {
        return true;
    }
    if is_crc(&lower) {
        return true;
    }
    if matches!(
        lower.as_str(),
        "bdrip"
            | "bluray"
            | "blu"
            | "ray"
            | "webrip"
            | "webdl"
            | "web"
            | "dl"
            | "hdtv"
            | "remux"
            | "bdmv"
            | "hevc"
            | "avc"
            | "h264"
            | "h265"
            | "x264"
            | "x265"
            | "av1"
            | "yuv"
            | "yuv420p"
            | "yuv420p10"
            | "hdr"
            | "hdr10"
            | "hdr10plus"
            | "dolbyvision"
            | "dv"
            | "flac"
            | "aac"
            | "truehd"
            | "dts"
            | "ac3"
            | "eac3"
            | "opus"
            | "hi10p"
            | "ma10p"
            | "10bit"
            | "8bit"
            | "dual"
            | "audio"
            | "chs"
            | "cht"
            | "jpn"
            | "eng"
            | "gb"
            | "big5"
            | "batch"
            | "complete"
            | "proper"
            | "repack"
            | "sp"
            | "ova"
            | "oad"
            | "ona"
            | "ncop"
            | "nced"
            | "season"
            | "disc"
            | "disk"
    ) {
        return true;
    }
    if lower == "4k" || lower == "uhd" {
        return true;
    }
    if lower.split_once('x').is_some_and(|(width, height)| {
        matches!(
            width,
            "720" | "1280" | "1920" | "2048" | "2560" | "3840" | "4096" | "7680"
        ) && matches!(height, "480" | "720" | "1080" | "1440" | "2160" | "4320")
    }) {
        return true;
    }
    if lower
        .strip_suffix('p')
        .or_else(|| lower.strip_suffix('i'))
        .is_some_and(|number| {
            matches!(
                number,
                "480" | "576" | "720" | "1080" | "1440" | "2160" | "4320"
            )
        })
    {
        return true;
    }
    if lower
        .strip_suffix("bit")
        .is_some_and(|number| number.chars().all(|character| character.is_ascii_digit()))
    {
        return true;
    }
    looks_like_episode_token(&lower)
}

fn looks_like_episode_token(value: &str) -> bool {
    let without_version = value
        .strip_suffix("v2")
        .or_else(|| value.strip_suffix("v3"))
        .unwrap_or(value);
    for prefix in ["episode", "ep", "e", "#", "vol", "season", "disc", "disk"] {
        if without_version.strip_prefix(prefix).is_some_and(|number| {
            !number.is_empty() && number.chars().all(|character| character.is_ascii_digit())
        }) {
            return true;
        }
    }
    if let Some(rest) = without_version.strip_prefix('s') {
        if is_ascii_number(rest) {
            return true;
        }
        let mut parts = rest.split('e');
        return parts.next().is_some_and(is_ascii_number)
            && parts.next().is_some_and(is_ascii_number)
            && parts.next().is_none();
    }
    false
}

fn is_ascii_number(value: &str) -> bool {
    !value.is_empty() && value.chars().all(|character| character.is_ascii_digit())
}

fn is_likely_plain_episode_number(value: &str) -> bool {
    (1..=3).contains(&value.len()) && is_ascii_number(value)
}

fn is_crc(value: &str) -> bool {
    let value = value.trim();
    value.len() == 8 && value.chars().all(|character| character.is_ascii_hexdigit())
}

fn remove_technical_words(value: &str, preserve_semantics: bool) -> (String, Vec<String>) {
    let normalized = value
        .replace(['(', ')', '{', '}', '【', '】'], " ")
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let mut kept = Vec::new();
    let mut removed = Vec::new();
    let mut index = 0;
    while index < normalized.len() {
        let token = normalized[index]
            .trim_matches(|character: char| matches!(character, ',' | ';' | '|' | '_'));
        let lower = token.to_ascii_lowercase();
        if matches!(lower.as_str(), "disc" | "disk") {
            removed.push(token.to_string());
            index += 1;
            if index < normalized.len() && is_ascii_number(trim_numeric(&normalized[index])) {
                removed.push(normalized[index].clone());
                index += 1;
            }
            continue;
        }
        if lower == "season" {
            if preserve_semantics {
                kept.push(token.to_string());
                index += 1;
                if index < normalized.len() && is_ascii_number(trim_numeric(&normalized[index])) {
                    kept.push(normalized[index].clone());
                    index += 1;
                }
            } else {
                removed.push(token.to_string());
                index += 1;
                if index < normalized.len() && is_ascii_number(trim_numeric(&normalized[index])) {
                    removed.push(normalized[index].clone());
                    index += 1;
                }
            }
            continue;
        }
        let semantic = preserve_semantics
            && (detect_season(token).is_some()
                || detect_edition_kind(token) != EditionKind::Unknown);
        if is_technical_atom(token)
            && !token.chars().all(|character| character.is_ascii_digit())
            && !semantic
        {
            removed.push(token.to_string());
            index += 1;
            continue;
        }
        if !token.is_empty() {
            kept.push(token.to_string());
        }
        index += 1;
    }
    while kept.len() > 1
        && kept.last().is_some_and(|token| {
            let trimmed = token.trim_matches(|character: char| {
                !character.is_ascii_alphanumeric() && character != '第' && character != '話'
            });
            (!preserve_semantics && is_likely_plain_episode_number(trimmed))
                || (looks_like_episode_token(&trimmed.to_ascii_lowercase())
                    && !(preserve_semantics && detect_season(trimmed).is_some()))
        })
    {
        if let Some(value) = kept.pop() {
            removed.push(value);
        }
    }
    (clean_join(kept), removed)
}

fn clean_join(values: Vec<String>) -> String {
    values
        .join(" ")
        .trim_matches(|character: char| {
            character.is_whitespace() || matches!(character, '-' | '_' | '.' | '·' | '|')
        })
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn trim_numeric(value: &str) -> &str {
    value.trim_matches(|character: char| !character.is_ascii_digit())
}

fn detect_year(value: &str) -> Option<i32> {
    let chars = value.chars().collect::<Vec<_>>();
    let mut index = 0;
    let mut detected = None;
    while index + 3 < chars.len() {
        if chars[index..index + 4]
            .iter()
            .all(|character| character.is_ascii_digit())
        {
            let left_clear = index == 0 || !chars[index - 1].is_ascii_digit();
            let right_clear = index + 4 == chars.len() || !chars[index + 4].is_ascii_digit();
            if left_clear && right_clear {
                let year = chars[index..index + 4]
                    .iter()
                    .collect::<String>()
                    .parse::<i32>()
                    .ok()?;
                if (1900..=2099).contains(&year) && !is_resolution_component(&chars, index) {
                    // Scene releases can contain a year-shaped number in the title before the
                    // actual release year (`Blade.Runner.2049.2017`). The later valid marker is
                    // the useful date while the earlier number remains part of the work title.
                    detected = Some(year);
                }
            }
            index += 4;
        } else {
            index += 1;
        }
    }
    detected
}

fn has_multiple_distinct_year_markers(value: &str) -> bool {
    let chars = value.chars().collect::<Vec<_>>();
    let mut first_year = None;
    let mut index = 0;
    while index + 3 < chars.len() {
        if chars[index..index + 4]
            .iter()
            .all(|character| character.is_ascii_digit())
        {
            let left_clear = index == 0 || !chars[index - 1].is_ascii_digit();
            let right_clear = index + 4 == chars.len() || !chars[index + 4].is_ascii_digit();
            if left_clear && right_clear && !is_resolution_component(&chars, index) {
                if let Ok(year) = chars[index..index + 4]
                    .iter()
                    .collect::<String>()
                    .parse::<i32>()
                {
                    if (1900..=2099).contains(&year) {
                        if first_year.is_some_and(|first| first != year) {
                            return true;
                        }
                        first_year = Some(year);
                    }
                }
            }
            index += 4;
        } else {
            index += 1;
        }
    }
    false
}

fn count_year_occurrences(value: &str, year: i32) -> usize {
    let target = year.to_string().chars().collect::<Vec<_>>();
    let characters = value.chars().collect::<Vec<_>>();
    if target.is_empty() || characters.len() < target.len() {
        return 0;
    }
    (0..=characters.len() - target.len())
        .filter(|start| {
            characters[*start..*start + target.len()] == target
                && (*start == 0 || !characters[*start - 1].is_ascii_digit())
                && (*start + target.len() == characters.len()
                    || !characters[*start + target.len()].is_ascii_digit())
        })
        .count()
}

fn is_resolution_component(characters: &[char], start: usize) -> bool {
    resolution_side_after(characters, start + 4) || resolution_side_before(characters, start)
}

fn resolution_side_after(characters: &[char], mut index: usize) -> bool {
    while characters
        .get(index)
        .is_some_and(|character| character.is_whitespace())
    {
        index += 1;
    }
    if !characters
        .get(index)
        .is_some_and(|character| matches!(character, 'x' | 'X' | '\u{00d7}'))
    {
        return false;
    }
    index += 1;
    while characters
        .get(index)
        .is_some_and(|character| character.is_whitespace())
    {
        index += 1;
    }
    let digit_start = index;
    while characters
        .get(index)
        .is_some_and(|character| character.is_ascii_digit())
    {
        index += 1;
    }
    is_resolution_side(&characters[digit_start..index])
}

fn resolution_side_before(characters: &[char], mut index: usize) -> bool {
    while index > 0 && characters[index - 1].is_whitespace() {
        index -= 1;
    }
    if index == 0 || !matches!(characters[index - 1], 'x' | 'X' | '\u{00d7}') {
        return false;
    }
    index -= 1;
    while index > 0 && characters[index - 1].is_whitespace() {
        index -= 1;
    }
    let digit_end = index;
    while index > 0 && characters[index - 1].is_ascii_digit() {
        index -= 1;
    }
    is_resolution_side(&characters[index..digit_end])
}

fn is_resolution_side(characters: &[char]) -> bool {
    (3..=4).contains(&characters.len())
        && characters
            .iter()
            .collect::<String>()
            .parse::<u16>()
            .is_ok_and(|value| value >= 480)
}

fn detect_season(value: &str) -> Option<u16> {
    let lower = value.nfkc().collect::<String>().to_lowercase();
    let tokens = lower
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    for (index, token) in tokens.iter().enumerate() {
        if matches!(*token, "season" | "シーズン" | "시즌") {
            if let Some(number) = tokens.get(index + 1).and_then(|value| parse_ordinal(value)) {
                return Some(number);
            }
        }
        if let Some(number) = parse_ordinal(token) {
            if tokens
                .get(index + 1)
                .is_some_and(|next| matches!(*next, "season" | "シーズン" | "시즌"))
            {
                return Some(number);
            }
        }
        for prefix in ["season", "シーズン", "시즌"] {
            if let Some(number) = token.strip_prefix(prefix).and_then(parse_ordinal) {
                if number > 0 {
                    return Some(number);
                }
            }
        }
        if let Some(number_text) = token.strip_suffix("season") {
            if let Some(number) = parse_ordinal(number_text) {
                if number > 0 {
                    return Some(number);
                }
            }
        }
        if let Some((season, episode)) = token
            .strip_prefix('s')
            .and_then(|token| token.split_once('e'))
        {
            if season.chars().all(|c| c.is_ascii_digit())
                && episode.chars().all(|c| c.is_ascii_digit())
            {
                if let Some(number) = parse_small_u16(season).filter(|number| *number > 0) {
                    if !episode.is_empty() {
                        return Some(number);
                    }
                }
            }
        }
        if let Some(number) = token.strip_prefix('s').and_then(parse_small_u16) {
            if number > 0 && !token.contains('e') {
                return Some(number);
            }
        }
    }
    detect_cjk_season(&lower)
}

fn detect_cjk_season(value: &str) -> Option<u16> {
    let chars = value.chars().collect::<Vec<_>>();
    for start in 0..chars.len() {
        if chars[start] != '第' {
            continue;
        }
        for end in start + 1..chars.len().min(start + 6) {
            if matches!(chars[end], '季' | '期') {
                let number = chars[start + 1..end].iter().collect::<String>();
                return parse_cjk_number(&number);
            }
        }
    }
    for (marker_index, marker) in chars.iter().enumerate() {
        if !matches!(*marker, '季' | '期' | '기') || marker_index == 0 {
            continue;
        }
        let mut start = marker_index;
        while start > 0 && marker_index - start < 3 && is_cjk_number_character(chars[start - 1]) {
            start -= 1;
        }
        if start < marker_index {
            let number = chars[start..marker_index].iter().collect::<String>();
            // Chinese ordinal seasons normally use `第N季`; accepting any bare Han numeral
            // before `季` would misread ordinary words such as `四季`. ASCII `2季`, Japanese
            // `二期`, and Korean `2기` remain supported here.
            if *marker == '季' && !number.chars().all(|character| character.is_ascii_digit()) {
                continue;
            }
            if let Some(number) = parse_cjk_number(&number) {
                if number > 0 {
                    return Some(number);
                }
            }
        }
    }
    None
}

fn is_cjk_number_character(character: char) -> bool {
    character.is_ascii_digit()
        || matches!(
            character,
            '一' | '二' | '两' | '兩' | '三' | '四' | '五' | '六' | '七' | '八' | '九' | '十'
        )
}

fn parse_cjk_number(value: &str) -> Option<u16> {
    if let Some(number) = parse_small_u16(value) {
        return Some(number);
    }
    match value {
        "一" => Some(1),
        "二" | "两" | "兩" => Some(2),
        "三" => Some(3),
        "四" => Some(4),
        "五" => Some(5),
        "六" => Some(6),
        "七" => Some(7),
        "八" => Some(8),
        "九" => Some(9),
        "十" => Some(10),
        _ if value.starts_with('十') => parse_cjk_number(&value["十".len()..]).map(|n| 10 + n),
        _ if value.ends_with('十') => {
            parse_cjk_number(&value[..value.len() - "十".len()]).map(|n| n * 10)
        }
        _ => None,
    }
}

fn parse_ordinal(value: &str) -> Option<u16> {
    let digits = value
        .strip_suffix("st")
        .or_else(|| value.strip_suffix("nd"))
        .or_else(|| value.strip_suffix("rd"))
        .or_else(|| value.strip_suffix("th"))
        .unwrap_or(value);
    parse_small_u16(digits)
}

fn parse_small_u16(value: &str) -> Option<u16> {
    if value.is_empty() || !value.chars().all(|character| character.is_ascii_digit()) {
        return None;
    }
    let parsed = value.parse::<u16>().ok()?;
    (parsed <= 99).then_some(parsed)
}

fn detect_edition_kind(value: &str) -> EditionKind {
    let lower = value.nfkc().collect::<String>().to_lowercase();
    if contains_word(&lower, "oad") {
        EditionKind::Oad
    } else if contains_word(&lower, "ova") || contains_word(&lower, "ona") {
        EditionKind::Ova
    } else if lower.contains("剧场版")
        || lower.contains("劇場版")
        || contains_word(&lower, "movie")
        || contains_word(&lower, "film")
    {
        EditionKind::Movie
    } else if contains_word(&lower, "sp") {
        EditionKind::Sp
    } else if contains_word(&lower, "special")
        || lower.contains("特别篇")
        || lower.contains("特別編")
        || lower.contains("特典")
    {
        EditionKind::Special
    } else if contains_word(&lower, "tv") {
        EditionKind::Tv
    } else {
        EditionKind::Unknown
    }
}

fn contains_word(value: &str, needle: &str) -> bool {
    value
        .split(|character: char| !character.is_alphanumeric())
        .any(|token| token == needle)
}

fn remove_year_marker(value: &str, year: Option<i32>) -> String {
    let Some(year) = year else {
        return value.to_string();
    };
    let year = year.to_string();
    let tokens = value.split_whitespace().collect::<Vec<_>>();
    let without_year = tokens
        .iter()
        .filter(|token| trim_numeric(token) != year)
        .map(|token| (*token).to_string())
        .collect::<Vec<_>>();
    if without_year.is_empty()
        && tokens.len() > 1
        && tokens.iter().all(|token| trim_numeric(token) == year)
    {
        year
    } else {
        clean_join(without_year)
    }
}

fn remove_semantic_markers(value: &str, season: Option<u16>, edition: EditionKind) -> String {
    let mut tokens = value
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>();
    if let Some(season) = season {
        let number = season.to_string();
        tokens.retain(|token| {
            let lower = token.to_lowercase();
            !(matches!(lower.as_str(), "season" | "シーズン" | "시즌")
                || lower == format!("s{season:02}")
                || lower == format!("s{season}")
                || lower == format!("season{season}")
                || lower == format!("シーズン{season}")
                || lower == format!("시즌{season}")
                || parse_ordinal(&lower) == Some(season)
                || (detect_season(&lower) == Some(season) && is_standalone_season_marker(&lower))
                || lower == number)
        });
    }
    if edition != EditionKind::Unknown {
        tokens.retain(|token| detect_edition_kind(token) == EditionKind::Unknown);
    }
    let mut result = clean_join(tokens);
    if let Some(season) = season {
        result = strip_trailing_season_marker(&result, season);
    }
    if result.is_empty() {
        value.to_string()
    } else {
        result
    }
}

fn is_standalone_season_marker(value: &str) -> bool {
    let lower = value.nfkc().collect::<String>().to_lowercase();
    if lower == "season" || lower == "シーズン" || lower == "시즌" {
        return true;
    }
    if lower.strip_prefix('s').and_then(parse_small_u16).is_some()
        || lower
            .strip_prefix("season")
            .and_then(parse_ordinal)
            .is_some()
        || lower
            .strip_prefix("シーズン")
            .and_then(parse_small_u16)
            .is_some()
        || lower
            .strip_prefix("시즌")
            .and_then(parse_small_u16)
            .is_some()
    {
        return true;
    }
    let chars = lower.chars().collect::<Vec<_>>();
    chars
        .last()
        .is_some_and(|marker| matches!(marker, '季' | '期' | '기'))
        && chars[..chars.len().saturating_sub(1)]
            .iter()
            .copied()
            .filter(|character| *character != '第')
            .all(is_cjk_number_character)
}

fn strip_trailing_season_marker(value: &str, season: u16) -> String {
    let cjk_number = match season {
        1 => Some("一"),
        2 => Some("二"),
        3 => Some("三"),
        4 => Some("四"),
        5 => Some("五"),
        6 => Some("六"),
        7 => Some("七"),
        8 => Some("八"),
        9 => Some("九"),
        10 => Some("十"),
        _ => None,
    };
    let mut suffixes = vec![
        format!("season {season}"),
        format!("season{season}"),
        format!("s{season:02}"),
        format!("s{season}"),
        format!("第{season}季"),
        format!("第{season}期"),
        format!("{season}季"),
        format!("{season}期"),
        format!("シーズン{season}"),
        format!("시즌 {season}"),
        format!("시즌{season}"),
        format!("{season}기"),
    ];
    if let Some(cjk_number) = cjk_number {
        suffixes.extend([
            format!("第{cjk_number}季"),
            format!("第{cjk_number}期"),
            format!("{cjk_number}季"),
            format!("{cjk_number}期"),
        ]);
    }
    suffixes.sort_by_key(|suffix| std::cmp::Reverse(suffix.chars().count()));
    let lower = value.to_lowercase();
    for suffix in suffixes {
        if lower.ends_with(&suffix) {
            let prefix_len = value.len().saturating_sub(suffix.len());
            if value.is_char_boundary(prefix_len) {
                let prefix = value[..prefix_len].trim_end_matches(|character: char| {
                    character.is_whitespace() || matches!(character, '-' | '_' | ':' | '·' | '|')
                });
                if is_useful_candidate(prefix) {
                    return prefix.to_string();
                }
            }
        }
    }
    value.to_string()
}

fn detect_language_hints(value: &str) -> Vec<LanguageHint> {
    let mut hints = Vec::new();
    for character in value.chars() {
        let code = character as u32;
        if (0x4e00..=0x9fff).contains(&code) && !hints.contains(&LanguageHint::Chinese) {
            hints.push(LanguageHint::Chinese);
        }
        if ((0x3040..=0x30ff).contains(&code) || (0x31f0..=0x31ff).contains(&code))
            && !hints.contains(&LanguageHint::Japanese)
        {
            hints.push(LanguageHint::Japanese);
        }
        if (0xac00..=0xd7af).contains(&code) && !hints.contains(&LanguageHint::Korean) {
            hints.push(LanguageHint::Korean);
        }
        if character.is_ascii_alphabetic() && !hints.contains(&LanguageHint::Latin) {
            hints.push(LanguageHint::Latin);
        }
    }
    hints
}

fn evidence_quality(primary: &str, file: Option<&str>, parent: Option<&str>) -> u8 {
    let normalized = normalize_title_for_match(primary);
    if !is_useful_candidate(primary) || is_generic_title(primary) {
        return 25;
    }
    let mut quality: u8 = if normalized.chars().count() >= 6 {
        70
    } else {
        55
    };
    if file.is_some_and(|value| normalize_title_for_match(value) == normalized) {
        quality = quality.saturating_add(20);
    }
    if parent.is_some_and(|value| normalize_title_for_match(value) == normalized) {
        quality = quality.saturating_add(10);
    }
    quality.min(100)
}

pub fn is_generic_title(value: &str) -> bool {
    matches!(
        normalize_title_for_match(value).as_str(),
        "bd" | "bdmv"
            | "sp"
            | "ova"
            | "oad"
            | "extra"
            | "extras"
            | "special"
            | "specials"
            | "season"
            | "season2"
            | "合集"
            | "合辑"
            | "動畫"
            | "动画"
            | "anime"
            | "animation"
            | "movie"
            | "movies"
            | "film"
            | "films"
            | "the"
            | "电影"
            | "電影"
            | "影视"
            | "影視"
            | "电视剧"
            | "電視劇"
            | "剧场版"
            | "劇場版"
            | "media"
            | "video"
            | "videos"
    )
}

pub fn is_safe_match_query(value: &str) -> bool {
    let trimmed = value.trim();
    let normalized = normalize_title_for_match(trimmed);
    let numeric_slash_title = trimmed.contains('/')
        && trimmed.chars().all(|character| {
            character.is_ascii_digit() || character == '/' || character.is_whitespace()
        });
    normalized.chars().count() >= 3
        && (!normalized.chars().all(|character| character.is_numeric()) || numeric_slash_title)
        && !is_generic_title(trimmed)
}

pub fn is_safe_movie_query(value: &str) -> bool {
    let normalized = normalize_title_for_match(value);
    is_safe_match_query(value)
        || is_four_digit_numeric_title(value)
        || (normalized.chars().count() == 2
            && normalized
                .chars()
                .all(|c| title_script_family(c) == Some(TitleScriptFamily::EastAsian))
            && !is_generic_title(value)
            && !matches!(normalized.as_str(), "映画" | "本編" | "中字"))
}

pub fn is_four_digit_numeric_title(value: &str) -> bool {
    let normalized = normalize_title_for_match(value);
    normalized.len() == 4
        && normalized
            .chars()
            .all(|character| character.is_ascii_digit())
}

fn is_useful_candidate(value: &str) -> bool {
    let compact = value.trim();
    compact.chars().count() >= 2 && !is_generic_title(compact)
}

fn push_unique(values: &mut Vec<String>, candidate: String) {
    let normalized = normalize_title_for_match(&candidate);
    if !values
        .iter()
        .any(|value| normalize_title_for_match(value) == normalized)
    {
        values.push(candidate);
    }
}

fn deduplicate_strings(values: &mut Vec<String>) {
    let mut unique = Vec::new();
    for value in std::mem::take(values) {
        if !value.trim().is_empty() {
            push_unique(&mut unique, value);
        }
    }
    *values = unique;
}

#[cfg(test)]
mod tests {
    #[test]
    fn movie_release_boundaries_do_not_keep_audio_groups_or_drop_title_numbers() {
        for (raw, title, year) in [
            (
                "Example.Feature.2009.2160p.UHD.BluRay.x265.10bit.HDR.DTS-HD.MA.5.1-GROUP",
                "Example Feature",
                2009,
            ),
            (
                "Example Feature (2011) (1080p BluRay x265 r00t)",
                "Example Feature",
                2011,
            ),
            (
                "Example.Feature.2011.Extended.Cut.Bluray.1080p.MNHD-12345@SITE.COM",
                "Example Feature",
                2011,
            ),
            (
                "Example.Feature.2014.JAPANESE.1080p.BluRay.H264.AAC-VXT",
                "Example Feature",
                2014,
            ),
            (
                "Example.Feature.2014.BD1080P.X264.AAC.Cantonese&Mandarin.CHS.Mp4Ba",
                "Example Feature",
                2014,
            ),
            (
                "Example.Feature.2013.2160p.iTunes.WEB-DL.DD5.1.DV.HDR.H.265-GROUP",
                "Example Feature",
                2013,
            ),
            (
                "Blade.Runner.2049.2017.1080p.BluRay",
                "Blade Runner 2049",
                2017,
            ),
            ("1917.2019.1080p", "1917", 2019),
            (
                "2001.A.Space.Odyssey.1968.1080p",
                "2001 A Space Odyssey",
                1968,
            ),
            (
                "[发布站www.example.com]示例电影2-2022_BD法语中字",
                "示例电影2",
                2022,
            ),
        ] {
            assert_eq!(
                super::movie_query_title(raw),
                (title.to_string(), Some(year)),
                "{raw}"
            );
        }
        assert_eq!(
            super::movie_query_title("Class of 1999 II").0,
            "Class of 1999 II"
        );
        assert_eq!(super::movie_query_title("K.O.2").0, "K.O.2");
    }

    #[test]
    fn movie_bilingual_and_broadcast_queries_exclude_programme_and_publisher_names() {
        use crate::models::LibraryRecognitionMode::Folder;
        let e=super::build_movie_match_evidence("【发布站 www.example.com】小姐[国韩多音轨+中文字幕].The.Handmaiden.2016.Extended.BluRay.REMUX.1080p.AVC.DTS-HD.MA5.1.2Audio-GROUP", "", &[], Folder);
        assert_eq!(e.primary_title, "小姐");
        assert_eq!(e.year, Some(2016));
        assert!(e.alternate_titles.iter().any(|s| s == "The Handmaiden"));
        let raw="[Apple&Kuno-V2 7000K][RAW][映画 小さき勇者たち～ガメラ～(Gamera the Brave)][夏帆(KAHO THE MOVIE 2006)](NECO-HD 1440x1080 H264 AAC)";
        let e = super::build_movie_match_evidence(raw, raw, &[], Folder);
        assert_eq!(e.year, Some(2006));
        assert!(e.alternate_titles.iter().any(|s| s == "Gamera the Brave"));
        assert!(crate::auto_match::match_queries(&e)
            .iter()
            .all(|s| !s.contains("KAHO") && !s.contains("NECO") && !s.contains("Apple")));
    }
    #[test]
    fn audit_optional_local_movie_names() {
        let Ok(input) = std::env::var("M2SHELF_MOVIE_NAMES_INPUT") else {
            return;
        };
        let output = std::env::var("M2SHELF_MOVIE_NAMES_OUTPUT").expect("audit output");
        let data: serde_json::Value =
            serde_json::from_slice(&std::fs::read(input).unwrap()).unwrap();
        let mut rows = Vec::new();
        for case in data["cases"].as_array().unwrap() {
            let folder = case["folder"].as_str().unwrap();
            let files = case["files"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| f.as_str().unwrap().to_string())
                .collect::<Vec<_>>();
            for mode in [
                crate::models::LibraryRecognitionMode::Folder,
                crate::models::LibraryRecognitionMode::VideoFile,
            ] {
                let e = super::build_movie_match_evidence(folder, folder, &files, mode.clone());
                rows.push(serde_json::json!({"folder":folder,"mode":format!("{mode:?}"),"primary":e.primary_title,"year":e.year,"strong":e.year_is_strong,"alternates":e.alternate_titles,"queries":crate::auto_match::match_queries(&e),"season":e.season_number,"edition":format!("{:?}",e.edition_kind)}));
            }
        }
        std::fs::write(output, serde_json::to_vec_pretty(&rows).unwrap()).unwrap();
    }
    use super::*;

    #[test]
    fn extracts_complex_latin_release_name() {
        assert_eq!(
            extract_search_keyword(
                "[Airota&VCB-Studio] Sousou no Frieren [Ma10p_1080p][x265_flac]"
            ),
            "Sousou no Frieren"
        );
    }

    #[test]
    fn preserves_japanese_title_and_removes_episode_parameters() {
        assert_eq!(
            extract_search_keyword("[VCB-Studio] ぼっち・ざ・ろっく！ [01][Ma10p_1080p]"),
            "ぼっち・ざ・ろっく！"
        );
    }

    #[test]
    fn preserves_title_inside_brackets_after_release_group() {
        assert_eq!(
            extract_search_keyword("[DBD-Raws][STEINS;GATE][1080P][BDRip][HEVC-10bit]"),
            "STEINS;GATE"
        );
    }

    #[test]
    fn removes_multiple_release_groups_and_season_markers() {
        assert_eq!(
            extract_search_keyword("[DBD-Raws][VCB-Studio] Title [S01][1080p]"),
            "Title"
        );
    }

    #[test]
    fn strips_real_world_leading_release_groups_without_dropping_bracketed_titles() {
        let cases = [
            (
                "[BeanSub&FZSD][Saiki_Kusuo_no_Psi-nan][S02][1080p]",
                "Saiki Kusuo no Psi-nan",
            ),
            ("[CASO][Rozen_Maiden][01-12][1080P]", "Rozen Maiden"),
            (
                "[NanoAlchemist] Rozen Maiden S2 [01][1080p]",
                "Rozen Maiden",
            ),
            (
                "[Nekomoe kissaten] Tonari no Kyuuketsuki-san [1080p]",
                "Tonari no Kyuuketsuki-san",
            ),
            ("[UHA-WINGS][Fate_stay_night][1080p]", "Fate stay night"),
        ];
        for (raw, expected) in cases {
            assert_eq!(extract_search_keyword(raw), expected, "raw={raw}");
        }
        assert_eq!(extract_search_keyword("[NANA][1080p]"), "NANA");
        assert_eq!(
            extract_search_keyword("[STEINS;GATE][1080p]"),
            "STEINS;GATE"
        );
    }

    #[test]
    fn media_file_frequency_provides_candidate_without_mutating_original() {
        let original = "BD";
        let result = build_search_prefill(
            original,
            original,
            &[
                "[VCB-Studio] Frieren [01][1080p].mkv".into(),
                "[VCB-Studio] Frieren [02][1080p].mkv".into(),
                "[VCB-Studio] Frieren [03][1080p].mkv".into(),
            ],
        );
        assert_eq!(result.original_name, "BD");
        assert!(result.candidates.iter().any(|value| value == "Frieren"));
    }

    #[test]
    fn structured_evidence_extracts_season_year_movie_and_noise() {
        let evidence = build_match_evidence(
            "[VCB-Studio] Made in Abyss Season 2 Movie (2022) [1080p][HEVC]",
            "[VCB-Studio] Made in Abyss Season 2 Movie (2022) [1080p][HEVC]",
            Some("Made in Abyss"),
            &[],
        );
        assert_eq!(evidence.season_number, Some(2));
        assert_eq!(evidence.year, Some(2022));
        assert_eq!(evidence.edition_kind, EditionKind::Movie);
        assert!(evidence.primary_title.contains("Made in Abyss"));
        assert!(evidence.removed_noise.iter().any(|value| value == "1080p"));
        assert_eq!(evidence.parent_title.as_deref(), Some("Made in Abyss"));
    }

    #[test]
    fn structured_evidence_extracts_a_common_live_action_movie_release_name() {
        let evidence = build_match_evidence(
            "[YTS] Oppenheimer (2023) [1080p][BluRay][x264]",
            "[YTS] Oppenheimer (2023) [1080p][BluRay][x264]",
            None,
            &["Oppenheimer.2023.1080p.BluRay.x264.mkv".into()],
        );
        assert_eq!(evidence.primary_title, "Oppenheimer");
        assert_eq!(evidence.year, Some(2023));
        assert!(is_safe_match_query(&evidence.primary_title));
    }

    #[test]
    fn dotted_scene_movie_names_become_clean_structured_queries() {
        let cases = [
            (
                "The.Sword.of.Doom.1966.1080p.BluRay.x264",
                "The Sword of Doom",
                1966,
            ),
            ("WolfWalkers.2020.1080p.BluRay.x265", "WolfWalkers", 2020),
            (
                "The.Empire.of.Corpses.2015.1080p.BDRip.HEVC",
                "The Empire of Corpses",
                2015,
            ),
            (
                "Blade.Runner.2049.2017.1920x1080.BluRay.x265",
                "Blade Runner 2049",
                2017,
            ),
            (
                "Resolution.Test.2020.2048\u{00d7}1080.BluRay.x265",
                "Resolution Test",
                2020,
            ),
        ];
        for (raw, expected_title, expected_year) in cases {
            let evidence = build_match_evidence(raw, raw, None, &[]);
            assert_eq!(evidence.primary_title, expected_title, "raw={raw}");
            assert_eq!(evidence.year, Some(expected_year), "raw={raw}");
            assert_eq!(extract_search_keyword(raw), expected_title, "raw={raw}");
            assert!(is_safe_match_query(&evidence.primary_title), "raw={raw}");
        }
        assert!(
            build_match_evidence(
                "Oppenheimer.2023.1080p.BluRay",
                "Oppenheimer.2023.1080p.BluRay",
                None,
                &[],
            )
            .year_is_strong
        );
        assert!(
            !build_match_evidence(
                "Movie.1999.Remastered.2024.1080p",
                "Movie.1999.Remastered.2024.1080p",
                None,
                &[],
            )
            .year_is_strong
        );
        assert!(
            !build_match_evidence(
                "Blade.Runner.2049.2017.1080p",
                "Blade.Runner.2049.2017.1080p",
                None,
                &[],
            )
            .year_is_strong
        );
    }

    #[test]
    fn media_file_name_supplies_movie_title_and_year_when_folder_is_generic() {
        let evidence = build_match_evidence(
            "Movies",
            "Movies",
            None,
            &["Blade.Runner.2049.2017.1920x1080.BluRay.x265.mkv".into()],
        );

        assert_eq!(evidence.primary_title, "Blade Runner 2049");
        assert_eq!(evidence.year, Some(2017));
    }

    #[test]
    fn numeric_movie_title_requires_independent_release_year_evidence() {
        let structured = build_match_evidence(
            "Movies",
            "Movies",
            None,
            &["1917.2019.1080p.BluRay.x264.mkv".into()],
        );
        assert_eq!(structured.primary_title, "1917");
        assert_eq!(structured.year, Some(2019));
        assert_eq!(structured.frequent_file_title.as_deref(), Some("1917"));

        let same_number_twice = build_match_evidence(
            "Movies",
            "Movies",
            None,
            &["1984.1984.1080p.BluRay.x264.mkv".into()],
        );
        assert_eq!(same_number_twice.primary_title, "1984");
        assert_eq!(same_number_twice.year, Some(1984));

        let ambiguous = build_match_evidence("Movies", "Movies", None, &["1917.mkv".into()]);
        assert_eq!(ambiguous.frequent_file_title, None);
        assert_eq!(ambiguous.year, None);
    }

    #[test]
    fn media_file_year_restores_a_four_digit_number_that_belongs_to_the_title() {
        let evidence = build_match_evidence(
            "Blade Runner 2049",
            "Blade Runner 2049",
            None,
            &["Blade.Runner.2049.2017.1920x1080.BluRay.x265.mkv".into()],
        );
        assert_eq!(evidence.primary_title, "Blade Runner 2049");
        assert_eq!(evidence.year, Some(2017));

        let leading_number = build_match_evidence(
            "2001 A Space Odyssey",
            "2001 A Space Odyssey",
            None,
            &["2001.A.Space.Odyssey.1968.1080p.BluRay.x264.mkv".into()],
        );
        assert_eq!(leading_number.primary_title, "2001 A Space Odyssey");
        assert_eq!(leading_number.year, Some(1968));
    }

    #[test]
    fn dominant_file_year_requires_a_unique_frequency_winner() {
        assert_eq!(
            dominant_media_file_year(&[
                "Movie.2020.1080p.mkv".into(),
                "Movie.2020.2160p.mkv".into(),
                "Movie.2021.1080p.mkv".into(),
            ]),
            Some(2020)
        );
        assert_eq!(
            dominant_media_file_year(&[
                "Movie.2020.1080p.mkv".into(),
                "Movie.2021.1080p.mkv".into(),
            ]),
            None
        );
    }

    #[test]
    fn full_width_movie_release_metadata_yields_independent_bilingual_titles() {
        let raw = "【高清影视之家发布 www.HDBTHD.com】【了不起的盖茨比[HDR+杜比视界双版本][中文字幕]The.Great.Gatsby.2013.1080p.BluRay.x265.10bit】.mkv";
        let evidence = build_match_evidence(raw, raw, Some("电影"), &[]);

        assert_eq!(evidence.year, Some(2013));
        assert!(evidence
            .alternate_titles
            .iter()
            .any(|title| title == "了不起的盖茨比"));
        assert!(evidence
            .alternate_titles
            .iter()
            .any(|title| title == "The Great Gatsby"));
        assert!(!evidence.primary_title.to_lowercase().contains("hdbthd"));
        assert!(!evidence.primary_title.to_lowercase().contains("hdr"));
        assert!(!evidence.primary_title.contains("字幕"));
        assert_eq!(evidence.parent_title, None);
        assert!(evidence.evidence_quality >= 70);
        let queries = crate::auto_match::match_queries(&evidence);
        assert_eq!(queries.len(), 3);
        assert_eq!(queries[1], "了不起的盖茨比");
        assert_eq!(queries[2], "The Great Gatsby");
    }

    #[test]
    fn full_width_brackets_do_not_remove_a_real_bracketed_movie_title() {
        assert_eq!(
            extract_search_keyword("【无名之辈】.2018.1080p.BluRay.x265"),
            "无名之辈"
        );
    }

    #[test]
    fn meaningful_dotted_title_without_release_metadata_is_preserved() {
        assert_eq!(extract_search_keyword("K.O.2"), "K.O.2");
    }

    #[test]
    fn structured_evidence_understands_cjk_seasons() {
        let simplified = extract_title_signals("葬送的芙莉莲 第二季 [2024]");
        let japanese = extract_title_signals("作品名 第3期");
        assert_eq!(simplified.season_number, Some(2));
        assert_eq!(simplified.year, Some(2024));
        assert_eq!(japanese.season_number, Some(3));
    }

    #[test]
    fn season_signals_cover_compact_and_cjk_second_season_forms() {
        let cases = [
            ("Example S2", "Example"),
            ("Example S02", "Example"),
            ("Example Season2", "Example"),
            ("Example Season 2", "Example"),
            ("Example 2nd Season", "Example"),
            ("作品名第二季", "作品名"),
            ("作品名 第2季", "作品名"),
            ("作品名 2期", "作品名"),
            ("作品名 二期", "作品名"),
            ("作品名 シーズン2", "作品名"),
            ("작품명 시즌 2", "작품명"),
            ("작품명 2기", "작품명"),
        ];
        for (raw, expected_series) in cases {
            let signals = extract_title_signals(raw);
            assert_eq!(signals.season_number, Some(2), "raw={raw}");
            assert_eq!(signals.series_title, expected_series, "raw={raw}");
        }
        assert_eq!(extract_title_signals("春夏秋冬四季").season_number, None);
    }

    #[test]
    fn file_and_embedded_aliases_survive_as_independent_match_evidence() {
        let evidence = build_match_evidence(
            "[CASO][A Fan English Translation][Saiki Kusuo no Psi-nan][S02]",
            "[CASO][A Fan English Translation][Saiki Kusuo no Psi-nan][S02]",
            None,
            &[
                "[BeanSub] Saiki Kusuo no Psi-nan S02 [01][1080p].mkv".into(),
                "[BeanSub] Saiki Kusuo no Psi-nan S02 [02][1080p].mkv".into(),
            ],
        );
        assert_eq!(evidence.season_number, Some(2));
        assert_eq!(
            evidence.frequent_file_title.as_deref(),
            Some("Saiki Kusuo no Psi-nan S2")
        );
        assert!(evidence
            .alternate_titles
            .iter()
            .any(|title| title == "Saiki Kusuo no Psi-nan"));
        assert!(evidence
            .alternate_titles
            .iter()
            .any(|title| title == "A Fan English Translation"));
    }

    #[test]
    fn confirmed_aliases_keep_node_titles_but_do_not_learn_parent_context() {
        let evidence = build_match_evidence(
            "斉木楠雄のΨ難",
            "The Disastrous Life of Saiki K.",
            Some("Anime Collection"),
            &["Saiki Kusuo no Psi-nan 01.mkv".into()],
        );
        let aliases = confirmed_alias_candidates(&evidence);
        assert!(aliases.iter().any(|alias| {
            normalize_title_for_match(alias)
                == normalize_title_for_match("The Disastrous Life of Saiki K.")
        }));
        assert!(aliases.iter().any(|alias| alias == "斉木楠雄のΨ難"));
        assert!(aliases
            .iter()
            .any(|alias| alias == "Saiki Kusuo no Psi-nan"));
        assert!(!aliases.iter().any(|alias| alias == "Anime Collection"));
    }

    #[test]
    fn confirmed_aliases_reconstruct_strong_year_qualifiers_end_to_end() {
        let dune_1984 = build_match_evidence("Dune 1984", "Dune 1984", None, &[]);
        let dune_2021 = build_match_evidence("Dune 2021", "Dune 2021", None, &[]);

        assert_eq!(dune_1984.primary_title, "Dune");
        assert_eq!(dune_2021.primary_title, "Dune");
        assert!(dune_1984.year_is_strong);
        assert!(dune_2021.year_is_strong);

        let aliases_1984 = confirmed_alias_candidates(&dune_1984);
        let aliases_2021 = confirmed_alias_candidates(&dune_2021);
        assert_eq!(aliases_1984.first().map(String::as_str), Some("Dune 1984"));
        assert_eq!(aliases_2021.first().map(String::as_str), Some("Dune 2021"));
        assert!(!aliases_1984.iter().any(|alias| alias == "Dune 2021"));
        assert!(!aliases_2021.iter().any(|alias| alias == "Dune 1984"));
        assert_ne!(
            normalize_title_for_match(&aliases_1984[0]),
            normalize_title_for_match(&aliases_2021[0])
        );
        assert!(aliases_1984
            .iter()
            .chain(&aliases_2021)
            .all(|alias| alias.chars().count() <= MAX_CONFIRMED_ALIAS_CHARS));
    }

    #[test]
    fn confirmed_aliases_reconstruct_distinct_seasons_end_to_end() {
        let season_one =
            build_match_evidence("Example Show Season 1", "Example Show Season 1", None, &[]);
        let season_two =
            build_match_evidence("Example Show Season 2", "Example Show Season 2", None, &[]);

        assert_eq!(season_one.season_number, Some(1));
        assert_eq!(season_two.season_number, Some(2));

        let aliases_one = confirmed_alias_candidates(&season_one);
        let aliases_two = confirmed_alias_candidates(&season_two);
        assert_eq!(
            aliases_one.first().map(String::as_str),
            Some("Example Show S1")
        );
        assert_eq!(
            aliases_two.first().map(String::as_str),
            Some("Example Show S2")
        );
        assert!(!aliases_one.iter().any(|alias| alias == "Example Show S2"));
        assert!(!aliases_two.iter().any(|alias| alias == "Example Show S1"));
        assert_ne!(
            normalize_title_for_match(&aliases_one[0]),
            normalize_title_for_match(&aliases_two[0])
        );
    }

    #[test]
    fn meaningful_title_numbers_are_preserved() {
        assert!(
            build_match_evidence("Steins;Gate 0", "Steins;Gate 0", None, &[])
                .primary_title
                .ends_with('0')
        );
        assert!(
            build_match_evidence("86 -Eighty Six-", "86 -Eighty Six-", None, &[])
                .primary_title
                .contains("86")
        );
        assert!(build_match_evidence("22/7", "22/7", None, &[])
            .primary_title
            .contains("22/7"));
        assert!(build_match_evidence("Fate/Zero", "Fate/Zero", None, &[])
            .primary_title
            .contains("Fate/Zero"));
        assert!(build_match_evidence("Re:Zero", "Re:Zero", None, &[])
            .primary_title
            .contains("Re:Zero"));
        assert!(is_safe_match_query("22/7"));
    }

    #[test]
    fn normalization_is_nfkc_case_and_separator_insensitive() {
        assert_eq!(
            normalize_title_for_match("ＳＴＥＩＮＳ；ＧＡＴＥ"),
            normalize_title_for_match("steins;gate")
        );
        assert_eq!(
            normalize_title_for_match("Tom & Jerry"),
            normalize_title_for_match("Tom and Jerry")
        );
    }

    #[test]
    fn high_frequency_file_title_becomes_structured_evidence() {
        let evidence = build_match_evidence(
            "BD",
            "BD",
            None,
            &[
                "[VCB-Studio] Frieren [01][1080p].mkv".into(),
                "[VCB-Studio] Frieren [02][1080p].mkv".into(),
                "[VCB-Studio] Frieren [03][1080p].mkv".into(),
            ],
        );
        assert_eq!(evidence.frequent_file_title.as_deref(), Some("Frieren"));
        assert_eq!(evidence.primary_title, "Frieren");
        assert!(evidence.evidence_quality >= 80);
    }

    #[test]
    fn generic_and_numeric_queries_are_rejected_but_specific_titles_are_safe() {
        assert!(!is_safe_match_query("Season 2"));
        assert!(!is_safe_match_query("123"));
        assert!(!is_safe_match_query("SP"));
        assert!(is_safe_match_query("86 Eighty-Six"));
    }
}
