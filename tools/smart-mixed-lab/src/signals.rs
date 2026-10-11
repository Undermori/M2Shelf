use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Signals {
    pub title: String,
    pub volume: Option<u32>,
    pub chapter: Option<u32>,
    pub bare_number: Option<u32>,
}
pub fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}
pub fn stem(path: &str) -> &str {
    let name = basename(path);
    name.rsplit_once('.').map_or(name, |(s, _)| s)
}
pub fn title_key(s: &str) -> String {
    s.trim_matches([' ', '.', '_', '-']).to_lowercase()
}
pub fn normalize_path(path: &str) -> Result<String, String> {
    let p = path.replace('\\', "/");
    if p.len() > 32767 || p.starts_with('/') || p.contains(':') || p.chars().any(|c| c.is_control())
    {
        return Err("INVALID_RELATIVE_PATH".into());
    }
    if p.is_empty() {
        return Ok(p);
    }
    let parts: Vec<_> = p.split('/').collect();
    if parts.len() > 64
        || parts
            .iter()
            .any(|s| s.is_empty() || *s == "." || *s == ".." || s.ends_with([' ', '.']))
    {
        return Err("INVALID_RELATIVE_PATH".into());
    }
    Ok(p)
}
/// Matches the production SQLite NOCASE baseline. Unicode normalization is deliberately not a source identity heuristic.
pub fn path_key(path: &str) -> String {
    path.to_ascii_lowercase()
}
pub fn parent(path: &str) -> Option<&str> {
    if path.is_empty() {
        None
    } else {
        Some(path.rsplit_once('/').map_or("", |(p, _)| p))
    }
}
pub fn below(path: &str, directory: &str) -> bool {
    directory.is_empty()
        || path == directory
        || path
            .strip_prefix(directory)
            .is_some_and(|r| r.starts_with('/'))
}
pub fn signals(name: &str) -> Signals {
    if is_range(name) {
        return Signals::default();
    }
    let parsed = m2shelf_media_parser::parse_for_grouping(name, m2shelf_media_parser::Kind::Comic);
    Signals {
        title: parsed.display_title,
        volume: parsed.volume,
        chapter: parsed.chapter,
        bare_number: if name.trim().chars().all(|c| c.is_ascii_digit()) && !name.trim().is_empty() {
            name.trim().parse().ok()
        } else {
            None
        },
    }
}

pub fn extra(name: &str) -> bool {
    let s = name.to_lowercase();
    [
        "特典",
        "番外",
        "附录",
        "附錄",
        "设定",
        "設定",
        "supplement",
        "extras",
        "bonus",
        "appendix",
    ]
    .iter()
    .any(|p| s.contains(p))
}
pub fn collection(name: &str) -> bool {
    let s = name.to_lowercase();
    [
        "全集",
        "合集",
        "合辑",
        "合輯",
        "omnibus",
        "complete collection",
    ]
    .iter()
    .any(|p| s.contains(p))
}
pub fn assets(name: &str) -> bool {
    let s = name.to_lowercase();
    [
        "wallpaper",
        "keyvisual",
        "key visual",
        "宣传图",
        "宣傳圖",
        "素材",
        "壁紙",
        "原画",
        "原畫",
    ]
    .iter()
    .any(|p| s.contains(p))
}
pub fn cover(name: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(||Regex::new(r"(?i)^(cover|front[_ -]?cover|folder|thumb(?:nail)?|poster|封面|カバー|표지)(?:[_ -]?[0-9]+)?$").unwrap()).is_match(stem(name))
}
pub fn intermediate(name: &str) -> bool {
    let s = name.to_ascii_lowercase();
    if [
        "pdf", "cbz", "epub", "txt", "mobi", "azw3", "jpg", "png", "images", "ing_pdf", "ing_jpg",
        "volumes", "chapters", "卷", "章节", "正篇", "本篇", "正文",
    ]
    .contains(&s.as_str())
    {
        return true;
    }
    is_range(name) || {
        let sig = signals(name);
        sig.title.is_empty() && (sig.volume.is_some() || sig.chapter.is_some())
    }
}
fn is_range(name: &str) -> bool {
    static RANGE: OnceLock<Regex> = OnceLock::new();
    RANGE
        .get_or_init(|| {
            Regex::new(r"(?i)^(?:第\s*)?[0-9]+\s*[-~–至]\s*[0-9]+\s*(?:卷|巻|话|話)$").unwrap()
        })
        .is_match(name)
}
pub fn ordered_numbered_pages(names: &[String]) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^(.*?)([0-9]+)$").unwrap());
    let mut prefix = None;
    let mut last = None;
    for name in names {
        let Some(c) = re.captures(stem(name)) else {
            return false;
        };
        let p = c[1].to_lowercase();
        let Ok(n) = c[2].parse::<u64>() else {
            return false;
        };
        if prefix.as_ref().is_some_and(|x| x != &p) || last.is_some_and(|x| n <= x) {
            return false;
        }
        prefix = Some(p);
        last = Some(n);
    }
    names.len() >= 2
}
