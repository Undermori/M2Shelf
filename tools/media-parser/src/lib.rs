//! Bounded, deterministic naming evidence shared by scanners, organization and provider adapters.
//! Original implementation. Upstream algorithms and test mechanisms are documented in SCRAPER_UPGRADE_REPORT.md.
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Animation,
    LiveAction,
    Comic,
    Ebook,
    Doujin,
    Artbook,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Subtype {
    Movie,
    Tv,
    Animation,
    Book,
    #[default]
    Unknown,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TitleCandidate {
    pub title: String,
    pub source: String,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderIds {
    pub tmdb_movie: Option<i64>,
    pub tmdb_tv: Option<i64>,
    pub bangumi: Option<i64>,
    pub isbn: Option<String>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Parsed {
    pub raw_name: String,
    pub display_title: String,
    pub search_titles: Vec<TitleCandidate>,
    pub year: Option<i32>,
    pub subtype: Subtype,
    pub season: Option<u16>,
    pub aired_date: Option<String>,
    pub episodes: Vec<u16>,
    pub volume: Option<u32>,
    pub chapter: Option<u32>,
    pub edition: Vec<String>,
    pub release_tags: Vec<String>,
    pub explicit_provider_ids: ProviderIds,
    pub source_evidence: Vec<String>,
    pub warnings: Vec<String>,
    pub authors: Vec<String>,
    pub languages: Vec<String>,
}
fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("static naming grammar"))
}
pub fn number(s: &str) -> Option<u32> {
    if let Ok(n) = s.parse() {
        return Some(n);
    }
    let (mut total, mut current) = (0, 0);
    for c in s.chars() {
        match c {
            '零' | '〇' => current = 0,
            '一' => current = 1,
            '二' => current = 2,
            '三' => current = 3,
            '四' => current = 4,
            '五' => current = 5,
            '六' => current = 6,
            '七' => current = 7,
            '八' => current = 8,
            '九' => current = 9,
            '十' => {
                total += current.max(1) * 10;
                current = 0
            }
            '百' => {
                total += current.max(1) * 100;
                current = 0
            }
            _ => return None,
        }
    }
    Some(total + current)
}
pub fn title_key(s: &str) -> String {
    s.nfkc()
        .flat_map(char::to_lowercase)
        .filter(|c| c.is_alphanumeric())
        .collect()
}
fn technical(s: &str) -> bool {
    let s = s.to_ascii_lowercase();
    let atom = s.trim_matches(|c: char| !c.is_alphanumeric());
    matches!(
        atom,
        "1080p"
            | "1080i"
            | "2160p"
            | "720p"
            | "480p"
            | "576p"
            | "4320p"
            | "uhd"
            | "4k"
            | "hdr"
            | "hdr10"
            | "hdr10plus"
            | "dv"
            | "bluray"
            | "bdrip"
            | "brrip"
            | "webdl"
            | "webrip"
            | "hdtv"
            | "remux"
            | "x264"
            | "x265"
            | "h264"
            | "h265"
            | "hevc"
            | "avc"
            | "av1"
            | "aac"
            | "ac3"
            | "eac3"
            | "dts"
            | "dtshd"
            | "flac"
            | "truehd"
            | "rarbg"
            | "yify"
            | "yts"
            | "dubbed"
            | "subbed"
            | "chs"
            | "cht"
            | "jpn"
            | "eng"
            | "japanese"
            | "english"
            | "french"
            | "korean"
            | "chinese"
            | "10bit"
            | "8bit"
            | "简日"
            | "繁日"
            | "简体"
            | "繁体"
            | "简繁"
            | "双语"
            | "中字"
            | "内封"
            | "内嵌"
    )
}
fn strip_extension(raw: &str) -> &str {
    match raw.rsplit_once('.') {
        Some((stem, ext))
            if [
                "mkv", "mp4", "avi", "m2ts", "ts", "mov", "wmv", "webm", "cbz", "pdf", "epub",
                "txt", "mobi", "azw3", "jpg", "jpeg", "png", "webp",
            ]
            .contains(&ext.to_ascii_lowercase().as_str()) =>
        {
            stem
        }
        _ => raw,
    }
}
/// Parse recognized semantic spans first, then form titles from the remaining text. Numbers at
/// title position are never consumed as years or absolute episodes without a contextual marker.
pub fn parse(raw: &str, kind: Kind) -> Parsed {
    let normalized = strip_extension(raw).nfkc().take(2048).collect::<String>();
    parse_normalized(raw, kind, normalized)
}
/// Physical grouping must not fold distinct Unicode source spellings into an edition.
/// Matching may normalize candidates, but source identity uses the same grammar without NFKC.
pub fn parse_for_grouping(raw: &str, kind: Kind) -> Parsed {
    parse_normalized(raw, kind, strip_extension(raw).chars().take(2048).collect())
}
fn parse_normalized(raw: &str, kind: Kind, normalized: String) -> Parsed {
    let mut result = Parsed {
        raw_name: raw.chars().take(2048).collect(),
        subtype: match kind {
            Kind::Animation => Subtype::Animation,
            Kind::LiveAction => Subtype::Unknown,
            _ => Subtype::Book,
        },
        ..Default::default()
    };
    let mut text = normalized.clone();
    for edition in [
        "Extended Cut",
        "Director s Cut",
        "Directors Cut",
        "Theatrical Cut",
        "Unrated Edition",
    ] {
        let comparable = text.replace(['.', '_'], " ");
        if let Some(at) = comparable
            .to_ascii_lowercase()
            .find(&edition.to_ascii_lowercase())
        {
            if comparable[..at]
                .split_whitespace()
                .any(|v| v.parse::<i32>().is_ok_and(|y| (1800..=2200).contains(&y)))
            {
                result.edition.push(edition.into());
                text = comparable[..at].into();
                break;
            }
        }
    }

    static IDS: OnceLock<Regex> = OnceLock::new();
    let ids = re(
        &IDS,
        r"(?i)(tmdb(?:id)?|tmdbtv(?:id)?|bgm(?:id)?|bangumi(?:id)?|isbn)[ :=_-]+([0-9Xx-]{1,20})",
    );
    for c in ids.captures_iter(&normalized) {
        let value = &c[2];
        let n = value.parse::<i64>().ok().filter(|n| *n > 0);
        match c[1].to_ascii_lowercase().as_str() {
            "tmdb" | "tmdbid" => result.explicit_provider_ids.tmdb_movie = n,
            "tmdbtv" | "tmdbtvid" => result.explicit_provider_ids.tmdb_tv = n,
            "bgm" | "bgmid" | "bangumi" | "bangumiid" => result.explicit_provider_ids.bangumi = n,
            "isbn" => {
                result.explicit_provider_ids.isbn =
                    valid_isbn(value).then(|| value.replace('-', ""))
            }
            _ => {}
        }
    }
    text = ids.replace_all(&text, "").into_owned();
    static EPISODES: OnceLock<Regex> = OnceLock::new();
    let episodes = re(
        &EPISODES,
        r"(?i)(?:\bS(?P<s>\d{1,2})[ ._-]*E(?P<e>\d{1,4})(?:(?:[ ._-]*E|-)(?P<end>\d{1,4}))?|\b(?P<x>\d{1,2})x(?P<xe>\d{1,4})(?:-(?P<xend>\d{1,4}))?|第\s*(?P<cn>[0-9一二三四五六七八九十百零〇]+)\s*[话話集])",
    );
    if matches!(kind, Kind::Animation | Kind::LiveAction) {
        if let Some(c) = episodes.captures(&text) {
            result.season = c
                .name("s")
                .or_else(|| c.name("x"))
                .and_then(|m| m.as_str().parse().ok());
            let first = c
                .name("e")
                .or_else(|| c.name("xe"))
                .or_else(|| c.name("cn"))
                .and_then(|m| number(m.as_str()))
                .and_then(|n| u16::try_from(n).ok());
            if let Some(first) = first {
                result.episodes.push(first);
                if let Some(end) = c
                    .name("end")
                    .or_else(|| c.name("xend"))
                    .and_then(|m| m.as_str().parse::<u16>().ok())
                    .filter(|end| *end > first && *end - first <= 20)
                {
                    result.episodes.extend(first + 1..=end)
                }
            }
            if kind == Kind::LiveAction {
                result.subtype = Subtype::Tv
            }
            result.source_evidence.push(c[0].into());
            let span = c.get(0).unwrap().range();
            if !text[..span.start]
                .trim_matches([' ', '.', '_', '-'])
                .is_empty()
            {
                text.truncate(span.start);
            } else {
                text.replace_range(span, "");
            }
        }
    }
    if kind == Kind::LiveAction && result.subtype != Subtype::Tv {
        static DATE: OnceLock<Regex> = OnceLock::new();
        if let Some(c)=re(&DATE,r"(?:^|[ ._-])(?P<y>(?:19|20)\d{2})[ ._-](?P<m>0[1-9]|1[0-2])[ ._-](?P<d>0[1-9]|[12]\d|3[01])(?:$|[ ._-])").captures(&text){
        let date=format!("{}-{}-{}",&c["y"],&c["m"],&c["d"]);let at=c.get(0).unwrap().start();
        if at>0 {result.aired_date=Some(date);result.subtype=Subtype::Tv;text.truncate(at);}
      }
    }
    static ORDINAL: OnceLock<Regex> = OnceLock::new();
    let ordinal = re(
        &ORDINAL,
        r"(?ix)(?:\b(?P<en>vol(?:ume)?|ch(?:apter)?)\.?\s*(?P<n>[0-9]{1,6})\b|第\s*(?P<cn>[0-9]{1,6}|[一二三四五六七八九十百零〇]+)\s*(?P<unit>卷|巻|话|話|章)|(?P<jp>[0-9]{1,6})\s*(?P<ju>巻|卷|話|话))",
    );
    if result.subtype == Subtype::Book {
        for c in ordinal.captures_iter(&text) {
            let n = c
                .name("n")
                .or_else(|| c.name("cn"))
                .or_else(|| c.name("jp"))
                .and_then(|m| number(m.as_str()));
            let chapter = c
                .name("en")
                .is_some_and(|m| m.as_str().to_ascii_lowercase().starts_with("ch"))
                || c.name("unit")
                    .or_else(|| c.name("ju"))
                    .is_some_and(|m| matches!(m.as_str(), "话" | "話" | "章"));
            if chapter {
                result.chapter = n
            } else {
                result.volume = n
            }
            result.source_evidence.push(c[0].into());
        }
        text = ordinal.replace_all(&text, "").into_owned();
    }
    // Bracket groups are classified individually. A bracketed title survives; known release
    // groups/languages/resolutions do not. Bare [01] is an episode only after a real title.
    static BRACKETS: OnceLock<Regex> = OnceLock::new();
    let brackets = re(&BRACKETS, r"\[([^\[\]]+)\]");
    let mut seen_title = false;
    text = brackets
        .replace_all(&text, |c: &regex::Captures<'_>| {
            let value = c[1].trim();
            let tag = technical(&value.replace([' ', '-', '.'], ""))
                || value.ends_with('组')
                || value.ends_with("字幕社")
                || value.contains("音轨")
                || value.contains("字幕")
                || value.contains("音軌");
            if tag {
                result.release_tags.push(value.into());
                String::new()
            } else if kind == Kind::Animation
                && seen_title
                && value.len() <= 4
                && value.chars().all(|c| c.is_ascii_digit())
            {
                if let Ok(n) = value.parse() {
                    result.episodes.push(n)
                }
                String::new()
            } else {
                seen_title = true;
                format!(" {value} ")
            }
        })
        .into_owned();
    if kind == Kind::Animation && result.episodes.is_empty() {
        static ABS: OnceLock<Regex> = OnceLock::new();
        if let Some(c) = re(&ABS, r"^(.+?)\s+-\s+(\d{1,3})(?:\s|$)").captures(&text) {
            result.episodes = c[2].parse().ok().into_iter().collect();
            text = c[1].into();
        }
    }
    // A technical boundary makes remaining words release details. Dot punctuation is retained
    // in short titles (K.O.2) unless a release/episode/year context proves separator semantics.
    let tokens = text
        .split(|c: char| c.is_whitespace() || matches!(c, '.' | '_' | '[' | ']' | '(' | ')'))
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    let release_at = tokens.iter().enumerate().find_map(|(i, t)| {
        let compact = t.replace('-', "");
        (technical(&compact)
            || t.split('-').any(technical)
            || compact.to_ascii_lowercase().starts_with("webdl"))
        .then_some(i)
    });
    let cutoff = release_at.unwrap_or(tokens.len());
    if let Some(at) = (1..cutoff).rev().find(|i| {
        tokens[*i]
            .parse::<i32>()
            .is_ok_and(|y| (1800..=2200).contains(&y))
            && (*i + 1 == cutoff
                || release_at.is_some()
                || tokens[*i + 1..cutoff].iter().all(|t| technical(t)))
    }) {
        result.year = tokens[at].parse().ok();
        result.source_evidence.push(format!("year:{}", tokens[at]));
    }
    let title_end = result
        .year
        .and_then(|y| {
            tokens[..cutoff]
                .iter()
                .rposition(|t| t.parse::<i32>() == Ok(y))
        })
        .unwrap_or(cutoff);
    let structured = release_at.is_some()
        || result.year.is_some()
        || !result.episodes.is_empty()
        || result.volume.is_some()
        || result.chapter.is_some()
        || result.aired_date.is_some();
    result.display_title = if structured {
        tokens[..title_end].join(" ")
    } else {
        text.trim_matches([' ', '[', ']', '(', ')', '_', '-'])
            .into()
    };
    result.display_title = result
        .display_title
        .trim_matches([' ', '-', '_', '.'])
        .to_string();
    for token in tokens.iter().skip(cutoff) {
        if !result.release_tags.iter().any(|s| s == token) {
            result.release_tags.push((*token).into())
        }
    }
    if kind == Kind::LiveAction
        && result.subtype != Subtype::Tv
        && (result.year.is_some() || result.explicit_provider_ids.tmdb_movie.is_some())
    {
        result.subtype = Subtype::Movie
    }
    if result.explicit_provider_ids.tmdb_tv.is_some() && kind == Kind::LiveAction {
        result.subtype = Subtype::Tv
    }
    if result.display_title.is_empty() {
        result.warnings.push("TITLE_UNCERTAIN".into())
    } else {
        result.search_titles.push(TitleCandidate {
            title: result.display_title.clone(),
            source: "name".into(),
        })
    }
    result.episodes.sort_unstable();
    result.episodes.dedup();
    result.release_tags.truncate(32);
    result
}
pub fn valid_isbn(value: &str) -> bool {
    let digits = value.chars().filter(|c| *c != '-').collect::<Vec<_>>();
    match digits.len() {
        13 => digits
            .iter()
            .enumerate()
            .try_fold(0, |sum, (i, c)| {
                c.to_digit(10)
                    .map(|n| sum + n * if i % 2 == 0 { 1 } else { 3 })
            })
            .is_some_and(|n| n % 10 == 0),
        10 => digits
            .iter()
            .enumerate()
            .try_fold(0, |sum, (i, c)| {
                if i == 9 && matches!(c, 'X' | 'x') {
                    Some(sum + 10)
                } else {
                    c.to_digit(10).map(|n| sum + n * (10 - i as u32))
                }
            })
            .is_some_and(|n| n % 11 == 0),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn release_goldens() {
        for (raw, title, year) in [
            (
                "Superbad.2007.1080p.BluRay.x264-GROUP.mkv",
                "Superbad",
                2007,
            ),
            (
                "Ernest.And.Celestine.2012.DUBBED.AAC-RARBG.mkv",
                "Ernest And Celestine",
                2012,
            ),
            ("1917.2019.2160p.mkv", "1917", 2019),
            ("2046.2004.mkv", "2046", 2004),
        ] {
            let p = parse(raw, Kind::LiveAction);
            assert_eq!(
                (p.display_title, p.year, p.subtype),
                (title.into(), Some(year), Subtype::Movie)
            );
        }
    }
    #[test]
    fn episode_goldens() {
        for (raw, season, episodes) in [
            ("Series.Name.S02E03.1080p.WEB-DL.mkv", 2, vec![3]),
            ("Series.Name.S01E01-E02.mkv", 1, vec![1, 2]),
            ("Series.Name.1x02.mkv", 1, vec![2]),
        ] {
            let p = parse(raw, Kind::LiveAction);
            assert_eq!(p.display_title, "Series Name");
            assert_eq!(p.season, Some(season));
            assert_eq!(p.episodes, episodes);
            assert_eq!(p.subtype, Subtype::Tv);
        }
    }
    #[test]
    fn cjk_and_numeric_titles() {
        for (raw, title, episode) in [
            (
                "[字幕组][葬送的芙莉莲][01][1080P][简日].mkv",
                "葬送的芙莉莲",
                1,
            ),
            ("86 - 01.mkv", "86", 1),
            ("3月的狮子 第01话.mkv", "3月的狮子", 1),
        ] {
            let p = parse(raw, Kind::Animation);
            assert_eq!(p.display_title, title);
            assert_eq!(p.episodes, vec![episode]);
        }
        let p = parse("20世纪少年 第01卷.pdf", Kind::Comic);
        assert_eq!(p.display_title, "20世纪少年");
        assert_eq!(p.volume, Some(1));
    }
    #[test]
    fn identifiers_and_ambiguous_numbers() {
        assert_eq!(parse("1917", Kind::LiveAction).year, None);
        assert_eq!(parse("86", Kind::Animation).episodes, vec![]);
        let p = parse("Film [tmdbid=123]", Kind::LiveAction);
        assert_eq!(p.explicit_provider_ids.tmdb_movie, Some(123));
        assert!(valid_isbn("978-0-306-40615-7"));
        assert!(!valid_isbn("9780306406158"));
    }
    #[test]
    fn bounded_and_deterministic() {
        let raw = "A".repeat(10000);
        let p = parse(&raw, Kind::Ebook);
        assert!(p.raw_name.len() <= 2048);
        assert_eq!(p, parse(&raw, Kind::Ebook));
    }
    #[test]
    fn episode_titles_dates_and_edition_spans() {
        let p = parse("Show.S01E02.The.Episode.Title.1080p.mkv", Kind::LiveAction);
        assert_eq!(p.display_title, "Show");
        assert_eq!(p.episodes, vec![2]);
        let p = parse("Daily.Show.2020.12.25.720p.mkv", Kind::LiveAction);
        assert_eq!(p.display_title, "Daily Show");
        assert_eq!(p.aired_date.as_deref(), Some("2020-12-25"));
        assert_eq!(p.subtype, Subtype::Tv);
        let p = parse(
            "Example.Feature.2011.Extended.Cut.BluRay.mkv",
            Kind::LiveAction,
        );
        assert_eq!(p.display_title, "Example Feature");
        assert_eq!(p.year, Some(2011));
        assert_eq!(p.edition, vec!["Extended Cut"]);
        assert_eq!(
            parse("東京喰種:re 第02巻.pdf", Kind::Comic).display_title,
            "東京喰種:re"
        );
    }
    #[test]
    fn ten_thousand_names_are_bounded() {
        let start = std::time::Instant::now();
        for i in 0..10000 {
            let p = parse(
                &format!("Synthetic.Show.S02E{:02}.1080p.WEB-DL.mkv", i % 100),
                Kind::LiveAction,
            );
            assert_eq!(p.display_title, "Synthetic Show");
        }
        eprintln!("10000 structured names: {:?}", start.elapsed());
        assert!(start.elapsed() < std::time::Duration::from_secs(20));
    }
}
