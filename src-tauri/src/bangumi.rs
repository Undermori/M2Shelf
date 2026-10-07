use std::{
    error::Error as StdError,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::OnceLock,
    thread,
    time::Duration,
};

use reqwest::{
    blocking::Client,
    header::{HeaderValue, CONTENT_LENGTH, CONTENT_TYPE, RETRY_AFTER},
    StatusCode, Url,
};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};
use unicode_normalization::UnicodeNormalization;

use crate::{cache, db::AppResult, models::BangumiSubject};

const SEARCH_URL: &str = "https://api.bgm.tv/v0/search/subjects";
const SUBJECT_DETAIL_URL: &str = "https://api.bgm.tv/v0/subjects";
const USER_AGENT: &str = concat!(
    "Undermori/M2Shelf/",
    env!("CARGO_PKG_VERSION"),
    " (Windows; https://space.bilibili.com/2903441)"
);
const MAX_COVER_DOWNLOAD_BYTES: u64 = 15 * 1024 * 1024;
const MAX_SEARCH_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_DETAIL_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_SEARCH_KEYWORD_CHARS: usize = 200;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const DEFAULT_RETRY_DELAY: Duration = Duration::from_millis(350);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(2);
const MAX_MATCH_ALIASES: usize = 32;
pub const SUBJECT_TYPE_ANIME: i64 = 2;
pub const SUBJECT_TYPE_LIVE_ACTION: i64 = 6;
const SUPPORTED_SUBJECT_TYPES: [i64; 2] = [SUBJECT_TYPE_ANIME, SUBJECT_TYPE_LIVE_ACTION];
static HTTP_CLIENT: OnceLock<Client> = OnceLock::new();

#[derive(Debug)]
enum ProviderRequestError {
    Transport(reqwest::Error),
    Http {
        status: StatusCode,
        retry_after: Option<Duration>,
    },
    ResponseTooLarge {
        limit: u64,
    },
    Read(std::io::Error),
    Decode(serde_json::Error),
}

impl ProviderRequestError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Transport(error) => should_retry(error),
            Self::Http { status, .. } => is_retryable_status(*status),
            Self::ResponseTooLarge { .. } | Self::Read(_) | Self::Decode(_) => false,
        }
    }

    fn retry_delay(&self) -> Duration {
        match self {
            Self::Http {
                retry_after: Some(delay),
                ..
            } => (*delay).min(MAX_RETRY_DELAY),
            _ => DEFAULT_RETRY_DELAY,
        }
    }
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    data: Vec<ApiSubject>,
}

#[derive(Debug, Deserialize)]
struct ApiSubject {
    id: i64,
    #[serde(rename = "type")]
    subject_type: i64,
    name: String,
    name_cn: Option<String>,
    date: Option<String>,
    summary: Option<String>,
    images: Option<ApiImages>,
    #[serde(default)]
    infobox: Vec<ApiInfoboxItem>,
}

#[derive(Debug, Deserialize)]
struct ApiSubjectDetail {
    id: i64,
    #[serde(rename = "type")]
    subject_type: i64,
    name: String,
    name_cn: Option<String>,
    date: Option<String>,
    summary: Option<String>,
    images: Option<ApiImages>,
    #[serde(default)]
    infobox: Vec<ApiInfoboxItem>,
}

#[derive(Debug, Deserialize)]
struct ApiInfoboxItem {
    key: String,
    value: Value,
}

#[derive(Debug, Deserialize)]
struct ApiImages {
    large: Option<String>,
    common: Option<String>,
    medium: Option<String>,
    small: Option<String>,
    grid: Option<String>,
}

fn client() -> AppResult<Client> {
    if let Some(client) = HTTP_CLIENT.get() {
        return Ok(client.clone());
    }
    let built = Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .http1_only()
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(USER_AGENT)
        .build()
        .map_err(|error| format!("无法初始化 Bangumi 网络客户端：{}", error_chain(&error)))?;
    // A cloned reqwest Client shares its connection pool. Reusing it across the bounded search,
    // detail and cover requests avoids a fresh TLS connection for every candidate. A benign race
    // can build two Clients during first use; subsequent requests use the one stored here.
    let _ = HTTP_CLIENT.set(built.clone());
    Ok(HTTP_CLIENT.get().cloned().unwrap_or(built))
}

#[cfg(test)]
pub fn search(keyword: &str, limit: usize) -> AppResult<Vec<BangumiSubject>> {
    search_for_kind(keyword, limit, crate::models::LibraryMediaKind::Video)
}

pub fn search_for_kind(
    keyword: &str,
    limit: usize,
    kind: crate::models::LibraryMediaKind,
) -> AppResult<Vec<BangumiSubject>> {
    let keyword = keyword.trim();
    if keyword.is_empty() {
        return Err("请输入 Bangumi 搜索词。".into());
    }
    if keyword.chars().count() > MAX_SEARCH_KEYWORD_CHARS {
        return Err(format!(
            "Bangumi 搜索词不能超过 {MAX_SEARCH_KEYWORD_CHARS} 个字符。"
        ));
    }
    let client = client()?;
    let response = search_with_retry(&client, keyword, limit, kind)?;

    Ok(response
        .data
        .into_iter()
        // Type 2 is animation and type 6 is live action. Keep a defensive filter even when the
        // API honors the multi-value filter so unrelated books/music/games never enter matching.
        .filter(|subject| kind.accepts_subject(subject.subject_type))
        .take(limit.clamp(1, 50))
        .map(api_subject_to_model)
        .collect())
}

pub fn is_supported_subject_type(subject_type: i64) -> bool {
    SUPPORTED_SUBJECT_TYPES.contains(&subject_type)
}

fn api_subject_to_model(subject: ApiSubject) -> BangumiSubject {
    let match_aliases = extract_match_aliases(&subject.infobox);
    let title = subject.name;
    let title_cn = subject
        .name_cn
        .and_then(non_empty)
        .or_else(|| extract_infobox_title(&subject.infobox, CHINESE_TITLE_LABELS));
    let title_en = extract_infobox_title(&subject.infobox, ENGLISH_TITLE_LABELS);
    let title_ja = extract_infobox_title(&subject.infobox, JAPANESE_TITLE_LABELS)
        .or_else(|| non_empty(title.clone()));
    let title_ko = extract_infobox_title(&subject.infobox, KOREAN_TITLE_LABELS);
    BangumiSubject {
        subject_id: subject.id,
        title,
        title_cn,
        title_en,
        title_ja,
        title_ko,
        match_aliases,
        date: subject.date.and_then(non_empty),
        image_url: subject.images.and_then(preferred_image),
        summary: subject.summary.and_then(non_empty),
        subject_type: subject.subject_type,
    }
}

pub fn enrich_subject(subject: &BangumiSubject) -> AppResult<BangumiSubject> {
    if subject.subject_id <= 0 || !matches!(subject.subject_type, 1 | 2 | 6) {
        return Err("只能读取有效的 Bangumi 动画或真人影视条目。".into());
    }
    let client = client()?;
    let detail = detail_with_retry(&client, subject.subject_id)?;
    merge_subject_detail(subject, detail)
}

fn merge_subject_detail(
    subject: &BangumiSubject,
    detail: ApiSubjectDetail,
) -> AppResult<BangumiSubject> {
    if detail.id != subject.subject_id
        || detail.subject_type != subject.subject_type
        || !matches!(detail.subject_type, 1 | 2 | 6)
    {
        return Err("Bangumi 条目详情与所选作品不匹配。".into());
    }

    let mut match_aliases = subject.match_aliases.clone();
    for alias in extract_match_aliases(&detail.infobox) {
        push_match_alias(&mut match_aliases, alias);
    }
    let title = non_empty(detail.name).unwrap_or_else(|| subject.title.clone());
    let title_cn = detail
        .name_cn
        .and_then(non_empty)
        .or_else(|| extract_infobox_title(&detail.infobox, CHINESE_TITLE_LABELS))
        .or_else(|| subject.title_cn.clone());
    let title_en = extract_infobox_title(&detail.infobox, ENGLISH_TITLE_LABELS)
        .or_else(|| subject.title_en.clone());
    let title_ja = extract_infobox_title(&detail.infobox, JAPANESE_TITLE_LABELS)
        .or_else(|| non_empty(title.clone()))
        .or_else(|| subject.title_ja.clone());
    let title_ko = extract_infobox_title(&detail.infobox, KOREAN_TITLE_LABELS)
        .or_else(|| subject.title_ko.clone());
    let image_url = detail
        .images
        .and_then(preferred_image)
        .or_else(|| subject.image_url.clone());

    Ok(BangumiSubject {
        subject_id: detail.id,
        title,
        title_cn,
        title_en,
        title_ja,
        title_ko,
        match_aliases,
        date: detail
            .date
            .and_then(non_empty)
            .or_else(|| subject.date.clone()),
        image_url,
        summary: detail
            .summary
            .and_then(non_empty)
            .or_else(|| subject.summary.clone()),
        subject_type: detail.subject_type,
    })
}

const CHINESE_TITLE_LABELS: &[&str] = &["简体中文名", "简体中文标题", "中文名", "中文标题", "中文"];
const ENGLISH_TITLE_LABELS: &[&str] = &[
    "英文名",
    "英语名",
    "英文标题",
    "English",
    "English title",
    "English name",
];
const JAPANESE_TITLE_LABELS: &[&str] =
    &["日文名", "日文标题", "日本語名", "日本語タイトル", "原文名"];
const KOREAN_TITLE_LABELS: &[&str] = &["韩文名", "韓文名", "韩语名", "韓語名", "한국어명"];
const GENERIC_ALIAS_LABELS: &[&str] = &[
    "别名",
    "別名",
    "别称",
    "別稱",
    "又名",
    "其他译名",
    "其他譯名",
    "译名",
    "譯名",
    "alias",
    "aliases",
    "synonym",
    "synonyms",
];
const ROMANIZED_TITLE_LABELS: &[&str] = &[
    "罗马字",
    "羅馬字",
    "罗马音",
    "羅馬音",
    "romaji",
    "romanized",
    "romanization",
    "romaji title",
    "原名",
    "原作名",
];

/// Keeps every bounded official title alias needed by the confidence scorer. The presentation
/// layer still uses the four explicit locale fields; these aliases are never written to source
/// names and do not require a database column.
fn extract_match_aliases(items: &[ApiInfoboxItem]) -> Vec<String> {
    let mut aliases = Vec::new();
    for item in items {
        if is_match_alias_label(&item.key) {
            collect_alias_values(&item.value, &mut aliases);
        }
        if let Some(values) = item.value.as_array() {
            for value in values {
                let Some(object) = value.as_object() else {
                    continue;
                };
                let Some(label) = object.get("k").and_then(Value::as_str) else {
                    continue;
                };
                if is_match_alias_label(label) {
                    if let Some(value) = object.get("v") {
                        collect_alias_values(value, &mut aliases);
                    }
                }
            }
        }
        if aliases.len() >= MAX_MATCH_ALIASES {
            break;
        }
    }
    aliases.truncate(MAX_MATCH_ALIASES);
    aliases
}

fn is_match_alias_label(value: &str) -> bool {
    label_matches(value, GENERIC_ALIAS_LABELS)
        || label_matches(value, CHINESE_TITLE_LABELS)
        || label_matches(value, ENGLISH_TITLE_LABELS)
        || label_matches(value, JAPANESE_TITLE_LABELS)
        || label_matches(value, KOREAN_TITLE_LABELS)
        || label_matches(value, ROMANIZED_TITLE_LABELS)
}

fn collect_alias_values(value: &Value, aliases: &mut Vec<String>) {
    if aliases.len() >= MAX_MATCH_ALIASES {
        return;
    }
    if let Some(value) = value.as_str() {
        push_match_alias(aliases, value.to_string());
        return;
    }
    if let Some(values) = value.as_array() {
        for value in values {
            collect_alias_values(value, aliases);
            if aliases.len() >= MAX_MATCH_ALIASES {
                return;
            }
        }
        return;
    }
    let Some(object) = value.as_object() else {
        return;
    };
    if let Some(value) = object.get("v").or_else(|| object.get("value")) {
        collect_alias_values(value, aliases);
    }
}

fn push_match_alias(aliases: &mut Vec<String>, value: String) {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > 200
        || value.starts_with("http://")
        || value.starts_with("https://")
        || aliases
            .iter()
            .any(|alias| alias.eq_ignore_ascii_case(value))
    {
        return;
    }
    aliases.push(value.to_string());
}

fn detail_with_retry(client: &Client, subject_id: i64) -> AppResult<ApiSubjectDetail> {
    let endpoint = format!("{SUBJECT_DETAIL_URL}/{subject_id}");
    let mut failures = Vec::new();
    for attempt in 0..2 {
        match detail_endpoint(client, &endpoint) {
            Ok(detail) => return Ok(detail),
            Err(error) => {
                failures.push(provider_endpoint_failure_label(&endpoint, &error));
                if attempt == 0 && error.is_retryable() {
                    thread::sleep(error.retry_delay());
                    continue;
                }
                break;
            }
        }
    }
    Err(format!(
        "读取 Bangumi 条目详情失败：{}。将保留搜索结果中的条目信息。",
        failures.join("；")
    ))
}

fn detail_endpoint(
    client: &Client,
    endpoint: &str,
) -> Result<ApiSubjectDetail, ProviderRequestError> {
    let response = checked_response(client.get(endpoint).send())?;
    decode_bounded_json(response, MAX_DETAIL_RESPONSE_BYTES)
}

fn preferred_image(images: ApiImages) -> Option<String> {
    images
        .large
        .or(images.common)
        .or(images.medium)
        .or(images.small)
        .or(images.grid)
        .and_then(non_empty)
}

fn extract_infobox_title(items: &[ApiInfoboxItem], labels: &[&str]) -> Option<String> {
    for item in items {
        if label_matches(&item.key, labels) {
            if let Some(value) = first_infobox_value(&item.value) {
                return Some(value);
            }
        }
    }
    for item in items {
        let Some(values) = item.value.as_array() else {
            continue;
        };
        for value in values {
            let Some(alias) = value.as_object() else {
                continue;
            };
            let Some(key) = alias.get("k").and_then(Value::as_str) else {
                continue;
            };
            if label_matches(key, labels) {
                if let Some(value) = alias
                    .get("v")
                    .and_then(Value::as_str)
                    .and_then(|value| non_empty(value.to_string()))
                {
                    return Some(value);
                }
            }
        }
    }
    None
}

fn first_infobox_value(value: &Value) -> Option<String> {
    if let Some(value) = value.as_str() {
        return non_empty(value.to_string());
    }
    value.as_array()?.iter().find_map(|item| {
        item.as_str()
            .and_then(|value| non_empty(value.to_string()))
            .or_else(|| {
                item.as_object()?
                    .get("v")?
                    .as_str()
                    .and_then(|value| non_empty(value.to_string()))
            })
    })
}

fn label_matches(value: &str, labels: &[&str]) -> bool {
    let value = normalize_infobox_label(value);
    labels
        .iter()
        .any(|label| value == normalize_infobox_label(label))
}

fn normalize_infobox_label(value: &str) -> String {
    value
        .nfkc()
        .collect::<String>()
        .trim()
        .trim_end_matches(|character: char| {
            character.is_whitespace() || matches!(character, ':' | '：')
        })
        .to_lowercase()
}

fn non_empty(value: String) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn search_with_retry(
    client: &Client,
    keyword: &str,
    limit: usize,
    kind: crate::models::LibraryMediaKind,
) -> AppResult<SearchResponse> {
    let mut failures = Vec::new();
    for attempt in 0..2 {
        match search_endpoint(client, SEARCH_URL, keyword, limit, kind) {
            Ok(response) => return Ok(response),
            Err(error) => {
                failures.push(provider_endpoint_failure_label(SEARCH_URL, &error));
                if attempt == 0 && error.is_retryable() {
                    thread::sleep(error.retry_delay());
                    continue;
                }
                break;
            }
        }
    }

    Err(format!(
        "搜索 Bangumi 失败：{}。请检查网络、系统代理或 DNS 后重试。",
        failures.join("；")
    ))
}

fn search_endpoint(
    client: &Client,
    endpoint: &str,
    keyword: &str,
    limit: usize,
    kind: crate::models::LibraryMediaKind,
) -> Result<SearchResponse, ProviderRequestError> {
    let response = checked_response(
        client
            .post(endpoint)
            .query(&[("limit", limit.clamp(1, 50)), ("offset", 0_usize)])
            .json(&search_request_body_for_kind(keyword, kind))
            .send(),
    )?;
    decode_bounded_json(response, MAX_SEARCH_RESPONSE_BYTES)
}

#[cfg(test)]
fn search_request_body(keyword: &str) -> Value {
    search_request_body_for_kind(keyword, crate::models::LibraryMediaKind::Video)
}
#[cfg(test)]
#[test]
fn new_library_searches_use_only_their_subject_type() {
    use crate::models::LibraryMediaKind;
    for (kind, expected) in [
        (LibraryMediaKind::Animation, 2),
        (LibraryMediaKind::LiveAction, 6),
        (LibraryMediaKind::Comic, 1),
        (LibraryMediaKind::Ebook, 1),
    ] {
        assert_eq!(
            search_request_body_for_kind("fixture", kind)["filter"]["type"],
            json!([expected])
        );
    }
}

fn search_request_body_for_kind(keyword: &str, kind: crate::models::LibraryMediaKind) -> Value {
    let types: &[i64] = match kind {
        crate::models::LibraryMediaKind::Comic | crate::models::LibraryMediaKind::Ebook => &[1],
        crate::models::LibraryMediaKind::Animation => &[2],
        crate::models::LibraryMediaKind::LiveAction => &[6],
        crate::models::LibraryMediaKind::Video => &SUPPORTED_SUBJECT_TYPES,
    };
    json!({
        "keyword": keyword,
        "sort": "match",
        "filter": { "type": types, "nsfw": false }
    })
}

fn checked_response(
    response: Result<reqwest::blocking::Response, reqwest::Error>,
) -> Result<reqwest::blocking::Response, ProviderRequestError> {
    let response = response.map_err(ProviderRequestError::Transport)?;
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    Err(ProviderRequestError::Http {
        status,
        retry_after: parse_retry_after(response.headers().get(RETRY_AFTER)),
    })
}

fn parse_retry_after(value: Option<&HeaderValue>) -> Option<Duration> {
    value?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

fn decode_bounded_json<T>(
    response: reqwest::blocking::Response,
    limit: u64,
) -> Result<T, ProviderRequestError>
where
    T: DeserializeOwned,
{
    if response
        .content_length()
        .is_some_and(|length| length > limit)
    {
        return Err(ProviderRequestError::ResponseTooLarge { limit });
    }
    decode_bounded_json_reader(response, limit)
}

fn decode_bounded_json_reader<T, R>(reader: R, limit: u64) -> Result<T, ProviderRequestError>
where
    T: DeserializeOwned,
    R: Read,
{
    let initial_capacity = usize::try_from(limit.min(64 * 1024)).unwrap_or(64 * 1024);
    let mut bytes = Vec::with_capacity(initial_capacity);
    reader
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(ProviderRequestError::Read)?;
    if bytes.len() as u64 > limit {
        return Err(ProviderRequestError::ResponseTooLarge { limit });
    }
    serde_json::from_slice(&bytes).map_err(ProviderRequestError::Decode)
}

fn is_retryable_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn should_retry(error: &reqwest::Error) -> bool {
    error.is_connect() || error.is_timeout() || error.status().is_some_and(is_retryable_status)
}

fn reqwest_retry_delay(_error: &reqwest::Error) -> Duration {
    // error_for_status does not retain Retry-After headers in a usable response. Search/detail
    // use ProviderRequestError above and honor a numeric Retry-After up to two seconds; cover
    // retries use this short bounded fallback.
    DEFAULT_RETRY_DELAY
}

fn provider_endpoint_failure_label(endpoint: &str, error: &ProviderRequestError) -> String {
    let host = Url::parse(endpoint)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_else(|| "未知地址".into());
    match error {
        ProviderRequestError::Transport(error) => endpoint_failure_label(endpoint, error),
        ProviderRequestError::Http { status, .. } => {
            format!("{host} 返回 HTTP {}", status.as_u16())
        }
        ProviderRequestError::ResponseTooLarge { limit } => {
            format!("{host} 响应超过 {} MiB 安全限制", limit / (1024 * 1024))
        }
        ProviderRequestError::Read(error) => {
            format!("{host} 响应读取失败（{error}）")
        }
        ProviderRequestError::Decode(error) => {
            format!("{host} 响应格式无效（{error}）")
        }
    }
}

/// Detail enrichment is optional search evidence. A transport outage, a throttled provider, or a
/// server-wide failure should stop multiplying the same delay across hundreds of Subjects, while
/// an individual stale Subject (404) or malformed record must not disable unrelated details.
pub(crate) fn is_provider_wide_detail_error(error: &str) -> bool {
    is_provider_wide_network_error(error)
}

/// Cover downloads share a CDN and otherwise run once per newly bound Node. Stop the rest of the
/// run after transport/rate-limit/server failures, but keep 404, a missing image, and malformed
/// content local to that one Subject.
pub(crate) fn is_provider_wide_cover_error(error: &str) -> bool {
    is_provider_wide_network_error(error) || error.contains("读取 Bangumi 封面失败")
}

fn is_provider_wide_network_error(error: &str) -> bool {
    if [
        "无法初始化 Bangumi 网络客户端",
        "连接超时",
        "无法连接",
        "请求失败",
        "响应读取失败",
    ]
    .iter()
    .any(|marker| error.contains(marker))
    {
        return true;
    }
    error
        .split("返回 HTTP ")
        .nth(1)
        .map(|suffix| {
            suffix
                .chars()
                .take_while(|character| character.is_ascii_digit())
                .collect::<String>()
        })
        .and_then(|status| status.parse::<u16>().ok())
        .is_some_and(|status| status == 429 || (500..=599).contains(&status))
}

fn endpoint_failure_label(endpoint: &str, error: &reqwest::Error) -> String {
    let host = Url::parse(endpoint)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_else(|| "未知地址".into());
    let kind = if error.is_timeout() {
        "连接超时"
    } else if error.is_connect() {
        "无法连接"
    } else if error.is_decode() {
        "响应格式无效"
    } else if let Some(status) = error.status() {
        return format!("{host} 返回 HTTP {}", status.as_u16());
    } else {
        "请求失败"
    };
    format!("{host} {kind}（{}）", error_chain(error))
}

fn error_chain(error: &(dyn StdError + 'static)) -> String {
    let mut messages = vec![error.to_string()];
    let mut source = error.source();
    while let Some(cause) = source {
        let message = cause.to_string();
        if messages.last() != Some(&message) {
            messages.push(message);
        }
        source = cause.source();
    }
    messages.join(": ")
}

pub fn download_cover(
    _cache_operation: &cache::CoverCacheOperationGuard,
    cache_root: &Path,
    subject: &BangumiSubject,
) -> AppResult<Option<PathBuf>> {
    let Some(image_url) = subject.image_url.as_deref() else {
        return Ok(None);
    };
    let parsed_url = Url::parse(image_url).map_err(|_| "Bangumi 返回了无效的封面地址。")?;
    if parsed_url.scheme() != "https" || !is_allowed_cover_host(parsed_url.host_str()) {
        return Err("拒绝下载非 Bangumi 官方域名的封面。".into());
    }
    cache::ensure_directories(cache_root)?;
    let destination = cache::bangumi_cover_path(cache_root, subject.subject_id, image_url);
    let client = client()?;
    let mut response = download_response_with_retry(&client, image_url)?;

    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if !matches!(
        content_type.as_str(),
        "image/jpeg" | "image/png" | "image/webp"
    ) {
        return Err("Bangumi 封面响应不是支持的图片格式。".into());
    }
    if response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|length| length > MAX_COVER_DOWNLOAD_BYTES)
    {
        return Err("Bangumi 封面超过 15 MiB 安全限制。".into());
    }

    // Every request stages to its own same-directory file. This prevents concurrent retries for
    // the same Subject from truncating one another and lets the final replace remain atomic.
    let (mut pending_file, mut temporary_file) =
        cache::create_pending_cache_file(&destination, true)
            .map_err(|error| format!("创建封面缓存失败：{error}"))?;
    let mut limited = response.by_ref().take(MAX_COVER_DOWNLOAD_BYTES + 1);
    let copied = match std::io::copy(&mut limited, &mut temporary_file) {
        Ok(copied) => copied,
        Err(error) => {
            drop(temporary_file);
            return Err(format!("读取 Bangumi 封面失败：{error}"));
        }
    };
    if let Err(error) = temporary_file.flush() {
        drop(temporary_file);
        return Err(format!("写入封面缓存失败：{error}"));
    }
    if let Err(error) = temporary_file.sync_all() {
        drop(temporary_file);
        return Err(format!("同步封面缓存失败：{error}"));
    }
    if copied > MAX_COVER_DOWNLOAD_BYTES {
        drop(temporary_file);
        return Err("Bangumi 封面超过 15 MiB 安全限制。".into());
    }
    if copied == 0 {
        drop(temporary_file);
        return Err("Bangumi 返回了空封面文件。".into());
    }
    if let Err(error) = temporary_file.seek(SeekFrom::Start(0)) {
        drop(temporary_file);
        return Err(format!("校验封面缓存失败：{error}"));
    }
    let mut bytes = Vec::with_capacity(copied as usize);
    if let Err(error) = temporary_file.read_to_end(&mut bytes) {
        drop(temporary_file);
        return Err(format!("校验封面缓存失败：{error}"));
    }
    if !valid_image_signature(&content_type, &bytes) {
        drop(temporary_file);
        return Err("Bangumi 封面内容与图片格式不匹配。".into());
    }
    if let Err(error) = cache::validate_cover_payload(&bytes) {
        drop(temporary_file);
        return Err(format!("Bangumi 封面未通过安全校验：{error}"));
    }
    if let Err(error) = temporary_file.seek(SeekFrom::Start(0)) {
        drop(temporary_file);
        return Err(format!("校验封面缓存失败：{error}"));
    }
    drop(temporary_file);
    pending_file
        .commit_to(&destination)
        .map_err(|error| format!("保存封面缓存失败：{error}"))?;
    Ok(Some(destination))
}

fn download_response_with_retry(
    client: &Client,
    image_url: &str,
) -> AppResult<reqwest::blocking::Response> {
    let mut failures = Vec::new();
    for attempt in 0..2 {
        match client
            .get(image_url)
            .send()
            .and_then(|response| response.error_for_status())
        {
            Ok(response) => return Ok(response),
            Err(error) => {
                failures.push(endpoint_failure_label(image_url, &error));
                if attempt == 0 && should_retry(&error) {
                    thread::sleep(reqwest_retry_delay(&error));
                    continue;
                }
                break;
            }
        }
    }
    Err(format!(
        "下载 Bangumi 封面失败：{}。绑定已保留，可以稍后重新获取。",
        failures.join("；")
    ))
}

fn is_allowed_cover_host(host: Option<&str>) -> bool {
    host == Some("lain.bgm.tv")
}

fn valid_image_signature(content_type: &str, bytes: &[u8]) -> bool {
    match content_type {
        "image/jpeg" => bytes.starts_with(&[0xff, 0xd8, 0xff]),
        "image/png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "image/webp" => bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP",
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_rejects_oversized_keywords_before_network_access() {
        let error = search(&"x".repeat(MAX_SEARCH_KEYWORD_CHARS + 1), 20).unwrap_err();
        assert!(error.contains("不能超过"));
    }

    #[test]
    fn bounded_json_reader_rejects_oversized_or_invalid_responses() {
        let parsed: Value =
            decode_bounded_json_reader(std::io::Cursor::new(br#"{"ok":true}"#), 64).unwrap();
        assert_eq!(parsed["ok"], true);

        let oversized =
            decode_bounded_json_reader::<Value, _>(std::io::Cursor::new(vec![b' '; 65]), 64)
                .unwrap_err();
        assert!(matches!(
            oversized,
            ProviderRequestError::ResponseTooLarge { limit: 64 }
        ));

        let invalid =
            decode_bounded_json_reader::<Value, _>(std::io::Cursor::new(b"{"), 64).unwrap_err();
        assert!(matches!(invalid, ProviderRequestError::Decode(_)));
    }

    #[test]
    fn retry_policy_handles_rate_limits_with_a_bounded_delay() {
        let rate_limited = ProviderRequestError::Http {
            status: StatusCode::TOO_MANY_REQUESTS,
            retry_after: Some(Duration::from_secs(60)),
        };
        assert!(rate_limited.is_retryable());
        assert_eq!(rate_limited.retry_delay(), MAX_RETRY_DELAY);
        assert!(is_retryable_status(StatusCode::SERVICE_UNAVAILABLE));
        assert!(!is_retryable_status(StatusCode::NOT_FOUND));

        let one_second = HeaderValue::from_static("1");
        assert_eq!(
            parse_retry_after(Some(&one_second)),
            Some(Duration::from_secs(1))
        );
        let invalid = HeaderValue::from_static("not-a-delay");
        assert_eq!(parse_retry_after(Some(&invalid)), None);
    }

    #[test]
    fn detail_circuit_breaker_only_classifies_provider_wide_failures() {
        for error in [
            "读取 Bangumi 条目详情失败：api.bgm.tv 连接超时。",
            "读取 Bangumi 条目详情失败：api.bgm.tv 无法连接。",
            "读取 Bangumi 条目详情失败：api.bgm.tv 返回 HTTP 429。",
            "读取 Bangumi 条目详情失败：api.bgm.tv 返回 HTTP 503。",
        ] {
            assert!(is_provider_wide_detail_error(error), "error={error}");
        }
        assert!(!is_provider_wide_detail_error("api.bgm.tv 返回 HTTP 404"));
        assert!(!is_provider_wide_detail_error("api.bgm.tv 响应格式无效"));
    }

    #[test]
    fn cover_circuit_breaker_only_classifies_provider_wide_failures() {
        for error in [
            "下载 Bangumi 封面失败：lain.bgm.tv 连接超时。",
            "下载 Bangumi 封面失败：lain.bgm.tv 无法连接。",
            "下载 Bangumi 封面失败：lain.bgm.tv 返回 HTTP 429。",
            "下载 Bangumi 封面失败：lain.bgm.tv 返回 HTTP 502。",
            "读取 Bangumi 封面失败：连接被重置",
        ] {
            assert!(is_provider_wide_cover_error(error), "error={error}");
        }
        for error in [
            "lain.bgm.tv 返回 HTTP 404",
            "Bangumi 封面响应不是支持的图片格式。",
            "Bangumi 封面未通过安全校验：封面图片尺寸过大。",
        ] {
            assert!(!is_provider_wide_cover_error(error), "error={error}");
        }
    }

    #[test]
    fn search_requests_only_animation_and_live_action_subjects() {
        let body = search_request_body("奥本海默");
        assert_eq!(body["filter"]["type"], json!([2, 6]));
        assert_eq!(
            search_request_body_for_kind("漫画作品", crate::models::LibraryMediaKind::Comic)
                ["filter"]["type"],
            json!([1])
        );
        assert!(is_supported_subject_type(SUBJECT_TYPE_ANIME));
        assert!(is_supported_subject_type(SUBJECT_TYPE_LIVE_ACTION));
        for unsupported in [1, 3, 4, 5, 7] {
            assert!(!is_supported_subject_type(unsupported));
        }
    }

    #[test]
    fn cover_host_allowlist_rejects_lookalikes() {
        assert!(is_allowed_cover_host(Some("lain.bgm.tv")));
        assert!(!is_allowed_cover_host(Some("lain.bgm.tv.example.com")));
        assert!(!is_allowed_cover_host(Some("example.com")));
        assert!(!is_allowed_cover_host(None));
    }

    #[test]
    fn user_agent_identifies_local_user_app_and_version() {
        assert!(USER_AGENT.starts_with("Undermori/M2Shelf/"));
        assert!(USER_AGENT.contains(env!("CARGO_PKG_VERSION")));
        assert!(!USER_AGENT.contains("unpublished"));
    }

    #[test]
    fn search_result_preserves_multilingual_infobox_titles_for_binding_metadata() {
        let api_subject: ApiSubject = serde_json::from_str(
            r#"{
                "id": 123,
                "type": 2,
                "name": "作品の原題",
                "name_cn": "中文标题",
                "date": "2026-01-01",
                "summary": "summary",
                "images": null,
                "infobox": [
                    {"key": "English", "value": "English Title"},
                    {"key": "日本語名", "value": [{"v": "日本語タイトル"}]},
                    {"key": "한국어명", "value": [{"k": "", "v": "한국어 제목"}]},
                    {"key": "别名", "value": [
                        {"k": "罗马字", "v": "Romanized Title"},
                        {"k": "简称", "v": "Short Title"}
                    ]}
                ]
            }"#,
        )
        .unwrap();
        let subject = api_subject_to_model(api_subject);
        assert_eq!(subject.title_cn.as_deref(), Some("中文标题"));
        assert_eq!(subject.title_en.as_deref(), Some("English Title"));
        assert_eq!(subject.title_ja.as_deref(), Some("日本語タイトル"));
        assert_eq!(subject.title_ko.as_deref(), Some("한국어 제목"));
        for alias in [
            "English Title",
            "日本語タイトル",
            "한국어 제목",
            "Romanized Title",
            "Short Title",
        ] {
            assert!(subject.match_aliases.iter().any(|value| value == alias));
        }
    }

    #[test]
    fn subject_detail_extracts_multilingual_titles_from_official_infobox_shapes() {
        let detail: ApiSubjectDetail = serde_json::from_str(
            r#"{
                "id": 123,
                "type": 2,
                "name": "作品の原題",
                "name_cn": "作品中文名",
                "date": "2025-01-01",
                "summary": "summary",
                "images": null,
                "infobox": [
                    {"key":"English", "value":"English Title"},
                    {"key":"别名", "value":[
                        {"k":"日文名", "v":"明示された日本語名"},
                        {"k":"韓語名", "v":"한국어 제목"}
                    ]}
                ]
            }"#,
        )
        .unwrap();
        assert_eq!(
            extract_infobox_title(&detail.infobox, ENGLISH_TITLE_LABELS).as_deref(),
            Some("English Title")
        );
        assert_eq!(
            extract_infobox_title(&detail.infobox, JAPANESE_TITLE_LABELS).as_deref(),
            Some("明示された日本語名")
        );
        assert_eq!(
            extract_infobox_title(&detail.infobox, KOREAN_TITLE_LABELS).as_deref(),
            Some("한국어 제목")
        );
        let fallback = BangumiSubject {
            subject_id: 123,
            title: "Search title".into(),
            title_cn: None,
            title_en: None,
            title_ja: None,
            title_ko: None,
            match_aliases: Vec::new(),
            date: None,
            image_url: None,
            summary: None,
            subject_type: 2,
        };
        let merged = merge_subject_detail(&fallback, detail).unwrap();
        assert_eq!(merged.title, "作品の原題");
        assert_eq!(merged.title_cn.as_deref(), Some("作品中文名"));
        assert_eq!(merged.title_en.as_deref(), Some("English Title"));
        assert_eq!(merged.title_ja.as_deref(), Some("明示された日本語名"));
        assert_eq!(merged.title_ko.as_deref(), Some("한국어 제목"));
        assert!(merged
            .match_aliases
            .iter()
            .any(|value| value == "明示された日本語名"));
    }

    #[test]
    fn infobox_title_labels_accept_nfkc_case_spacing_and_colons() {
        let items = vec![
            ApiInfoboxItem {
                key: " English Title ： ".into(),
                value: json!("Translated title"),
            },
            ApiInfoboxItem {
                key: "其他譯名:".into(),
                value: json!(["Alternate one", "Alternate two"]),
            },
        ];
        assert_eq!(
            extract_infobox_title(&items, ENGLISH_TITLE_LABELS).as_deref(),
            Some("Translated title")
        );
        let aliases = extract_match_aliases(&items);
        for expected in ["Translated title", "Alternate one", "Alternate two"] {
            assert!(aliases.iter().any(|alias| alias == expected));
        }
    }

    #[test]
    fn subject_detail_uses_main_name_as_japanese_fallback() {
        let detail: ApiSubjectDetail = serde_json::from_str(
            r#"{
                "id": 9,
                "type": 2,
                "name": "メインタイトル",
                "name_cn": "中文标题",
                "date": null,
                "summary": null,
                "images": null,
                "infobox": []
            }"#,
        )
        .unwrap();
        let fallback = BangumiSubject {
            subject_id: 9,
            title: "search".into(),
            title_cn: None,
            title_en: None,
            title_ja: None,
            title_ko: None,
            match_aliases: Vec::new(),
            date: None,
            image_url: None,
            summary: None,
            subject_type: 2,
        };
        assert_eq!(
            merge_subject_detail(&fallback, detail)
                .unwrap()
                .title_ja
                .as_deref(),
            Some("メインタイトル")
        );
    }

    #[test]
    fn subject_detail_preserves_live_action_type_and_rejects_type_switches() {
        let search_subject = BangumiSubject {
            subject_id: 451975,
            title: "Oppenheimer".into(),
            title_cn: Some("奥本海默".into()),
            title_en: None,
            title_ja: None,
            title_ko: None,
            match_aliases: Vec::new(),
            date: Some("2023-07-21".into()),
            image_url: None,
            summary: None,
            subject_type: SUBJECT_TYPE_LIVE_ACTION,
        };
        let detail: ApiSubjectDetail = serde_json::from_str(
            r#"{
                "id": 451975,
                "type": 6,
                "name": "Oppenheimer",
                "name_cn": "奥本海默",
                "date": "2023-07-21",
                "summary": "summary",
                "images": null,
                "infobox": []
            }"#,
        )
        .unwrap();
        let merged = merge_subject_detail(&search_subject, detail).unwrap();
        assert_eq!(merged.subject_type, SUBJECT_TYPE_LIVE_ACTION);

        let wrong_type_detail: ApiSubjectDetail = serde_json::from_str(
            r#"{
                "id": 451975,
                "type": 2,
                "name": "Oppenheimer",
                "name_cn": null,
                "date": null,
                "summary": null,
                "images": null,
                "infobox": []
            }"#,
        )
        .unwrap();
        assert!(merge_subject_detail(&search_subject, wrong_type_detail).is_err());
    }

    #[test]
    #[ignore = "requires external network access"]
    fn live_search_returns_supported_subject_results() {
        let results = search("葬送的芙莉莲", 3).expect("Bangumi live search should succeed");
        assert!(!results.is_empty());
        assert!(results
            .iter()
            .all(|subject| is_supported_subject_type(subject.subject_type)));
        assert!(results
            .iter()
            .any(|subject| !subject.match_aliases.is_empty()));
    }

    #[test]
    #[ignore = "requires external network access"]
    fn live_search_returns_a_live_action_movie() {
        let results = search("盗梦空间", 10).expect("Bangumi live search should succeed");
        assert!(results.iter().any(|subject| {
            subject.subject_id == 24057
                && subject.subject_type == SUBJECT_TYPE_LIVE_ACTION
                && subject.image_url.is_some()
        }));
    }

    #[test]
    #[ignore = "requires external network access"]
    fn live_cover_download_survives_signature_validation() {
        let temp = tempfile::TempDir::new().unwrap();
        let subject = BangumiSubject {
            subject_id: 25417,
            title: "cover test".into(),
            title_cn: None,
            title_en: None,
            title_ja: None,
            title_ko: None,
            match_aliases: Vec::new(),
            date: None,
            image_url: Some("https://lain.bgm.tv/pic/cover/l/22/43/25417_o675v.jpg".into()),
            summary: None,
            subject_type: 2,
        };
        let cache_operation = cache::begin_cover_cache_operation();
        let cover = download_cover(&cache_operation, temp.path(), &subject)
            .expect("Bangumi live cover should download")
            .expect("test subject has an image");
        assert!(cover.is_file());
        assert!(std::fs::metadata(cover).unwrap().len() > 0);
    }
}
