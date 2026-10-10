//! MOBI6/KF8 adapter. A bundled, isolated libmobi process reconstructs markup;
//! only safe text/runs and bounded local images cross the existing reader IPC.
use crate::{
    comics::IndexedPage,
    db::AppResult,
    ebooks::{EpubBlock, EpubRun},
};
use base64::{engine::general_purpose::STANDARD, Engine};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const MAX_SOURCE: u64 = 128 * 1024 * 1024;
const MAX_OUTPUT: usize = 96 * 1024 * 1024;
const MAX_CHAPTER: usize = 4 * 1024 * 1024;
struct Chapter {
    name: String,
    html: String,
    locator: String,
}
struct Book {
    chapters: Vec<Chapter>,
    resources: HashMap<u64, Vec<u8>>,
    cover: u64,
}
static CACHE: Mutex<Option<(String, Arc<Book>)>> = Mutex::new(None);

fn invalid() -> String {
    "BOOK_KINDLE_INVALID".into()
}
fn u16be(bytes: &[u8], offset: usize) -> AppResult<u16> {
    Ok(u16::from_be_bytes(
        bytes
            .get(offset..offset + 2)
            .ok_or_else(invalid)?
            .try_into()
            .map_err(|_| invalid())?,
    ))
}
fn u32be(bytes: &[u8], offset: usize) -> AppResult<u32> {
    Ok(u32::from_be_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or_else(invalid)?
            .try_into()
            .map_err(|_| invalid())?,
    ))
}
/// Validate all PalmDB offsets and both headers of a hybrid before native parsing.
fn preflight(bytes: &[u8]) -> AppResult<()> {
    if bytes.len() < 86 || !matches!(bytes.get(60..68), Some(b"BOOKMOBI" | b"TEXtREAd")) {
        return Err(invalid());
    }
    let count = usize::from(u16be(bytes, 76)?);
    if count == 0 || count > 20_000 {
        return Err("BOOK_KINDLE_LIMIT".into());
    }
    let table_end = 78 + count * 8;
    let mut previous = table_end;
    for i in 0..count {
        let start = u32be(bytes, 78 + i * 8)? as usize;
        let end = if i + 1 < count {
            u32be(bytes, 78 + (i + 1) * 8)? as usize
        } else {
            bytes.len()
        };
        // PalmDB permits empty optional records sharing the next record's offset.
        if start < previous || start > end || end > bytes.len() || (i == 0 && start == end) {
            return Err(invalid());
        }
        previous = start;
        let record = &bytes[start..end];
        if i == 0 || record.get(16..20) == Some(b"MOBI") {
            if u16be(record, 12)? != 0 {
                return Err("BOOK_KINDLE_DRM".into());
            }
            if !matches!(u16be(record, 0)?, 1 | 2 | 17480) {
                return Err("BOOK_KINDLE_COMPRESSION".into());
            }
            if u32be(record, 4)? > 32 * 1024 * 1024 || u16be(record, 8)? > 8192 {
                return Err("BOOK_KINDLE_LIMIT".into());
            }
            if record.get(16..20) != Some(b"MOBI") {
                return Err("BOOK_KINDLE_VARIANT".into());
            }
            if !matches!(u32be(record, 28)?, 1252 | 65001) {
                return Err("BOOK_KINDLE_ENCODING".into());
            }
            if u32be(record, 36)? > 8 {
                return Err("BOOK_KINDLE_VARIANT".into());
            }
        }
    }
    Ok(())
}

#[cfg(windows)]
struct Job(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
impl Job {
    fn attach(child: &std::process::Child) -> AppResult<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::{Foundation::CloseHandle, System::JobObjects::*};
        // SAFETY: the owned Job handle and initialized structure live through the
        // synchronous API calls; only this child is assigned to this Job.
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return Err("BOOK_KINDLE_WORKER".into());
            }
            let mut limit: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limit.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
                | JOB_OBJECT_LIMIT_PROCESS_MEMORY
                | JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
            limit.BasicLimitInformation.ActiveProcessLimit = 1;
            limit.ProcessMemoryLimit = 384 * 1024 * 1024;
            if SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&limit as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&limit) as u32,
            ) == 0
                || AssignProcessToJobObject(job, child.as_raw_handle()) == 0
            {
                CloseHandle(job);
                return Err("BOOK_KINDLE_WORKER".into());
            }
            Ok(Self(job))
        }
    }
}
#[cfg(windows)]
impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

fn worker(file: &File) -> AppResult<Vec<u8>> {
    let executable = std::env::current_exe().map_err(|_| "BOOK_KINDLE_WORKER")?;
    let directory = executable.parent().ok_or("BOOK_KINDLE_WORKER")?;
    let mut path = directory.join("M2ShelfMobi.exe");
    if cfg!(debug_assertions)
        && directory.file_name().is_some_and(|n| n == "deps")
        && !path.exists()
    {
        path = directory
            .parent()
            .ok_or("BOOK_KINDLE_WORKER")?
            .join("M2ShelfMobi.exe");
    }
    let mut command = Command::new(path);
    command
        .arg("--read-stdio-v1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(
            file.try_clone().map_err(|_| "COMIC_READ_FAILED")?,
        ));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn().map_err(|_| "BOOK_KINDLE_WORKER")?;
    #[cfg(windows)]
    let _job = match Job::attach(&child) {
        Ok(job) => job,
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
    };
    let output = child.stdout.take().ok_or("BOOK_KINDLE_WORKER")?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        output
            .take(MAX_OUTPUT as u64 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let start = Instant::now();
    let result = (|| {
        child
            .stdin
            .take()
            .ok_or("BOOK_KINDLE_WORKER")?
            .write_all(b"S")
            .map_err(|_| "BOOK_KINDLE_WORKER")?;
        loop {
            if let Some(status) = child.try_wait().map_err(|_| "BOOK_KINDLE_WORKER")? {
                return match status.code() {
                    Some(0) => Ok(()),
                    Some(10) => Err("BOOK_KINDLE_DRM"),
                    Some(11) => Err("BOOK_KINDLE_VARIANT"),
                    Some(12) => Err("BOOK_KINDLE_LIMIT"),
                    _ => Err("BOOK_KINDLE_INVALID"),
                };
            }
            if start.elapsed() > Duration::from_secs(20) {
                return Err("BOOK_KINDLE_LIMIT");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    })();
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    let bytes = reader
        .join()
        .map_err(|_| "BOOK_KINDLE_WORKER")?
        .map_err(|_| "BOOK_KINDLE_WORKER")?;
    result.map_err(str::to_string)?;
    if bytes.len() > MAX_OUTPUT {
        return Err("BOOK_KINDLE_LIMIT".into());
    }
    Ok(bytes)
}
fn take_number(bytes: &[u8], offset: &mut usize) -> AppResult<u64> {
    let value = u64::from_le_bytes(
        bytes
            .get(*offset..*offset + 8)
            .ok_or_else(invalid)?
            .try_into()
            .map_err(|_| invalid())?,
    );
    *offset += 8;
    Ok(value)
}
fn parsed(file: &mut File) -> AppResult<Arc<Book>> {
    let size = file.metadata().map_err(|_| "COMIC_READ_FAILED")?.len();
    if size > MAX_SOURCE {
        return Err("BOOK_KINDLE_LIMIT".into());
    }
    let identity = file_identity(file)?;
    // The opened-handle identity, revision metadata and Root are checked again by
    // the reader. Reuse only that exact source; do not reread a complete book for
    // every chapter/image or font-size change.
    let mut cache = CACHE.lock().map_err(|_| "BOOK_KINDLE_WORKER")?;
    if let Some((key, book)) = &*cache {
        if key == &identity {
            return Ok(book.clone());
        }
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|_| "COMIC_READ_FAILED")?;
    let mut source = Vec::with_capacity(size as usize);
    Read::by_ref(file)
        .take(MAX_SOURCE + 1)
        .read_to_end(&mut source)
        .map_err(|_| "COMIC_READ_FAILED")?;
    if source.len() as u64 != size {
        return Err("COMIC_PAGE_CHANGED".into());
    }
    preflight(&source)?;
    let digest = format!("{:x}", Sha256::digest(&source));
    drop(source);
    // One generator/cache entry: concurrent reader and scanner requests cannot
    // launch unbounded workers or accumulate complete decompressed books.
    file.seek(SeekFrom::Start(0))
        .map_err(|_| "COMIC_READ_FAILED")?;
    let bytes = worker(file)?;
    if bytes.get(..8) != Some(b"M2MOBI01") {
        return Err(invalid());
    }
    let mut offset = 8;
    let _version = take_number(&bytes, &mut offset)?;
    let cover = take_number(&bytes, &mut offset)?;
    let mut book = Book {
        chapters: Vec::new(),
        resources: HashMap::new(),
        cover,
    };
    let mut markup = Vec::new();
    let mut toc = Vec::new();
    let mut parts = 0;
    loop {
        let kind = take_number(&bytes, &mut offset)?;
        if kind == 0 {
            break;
        }
        parts += 1;
        if parts > 20_000 {
            return Err("BOOK_KINDLE_LIMIT".into());
        }
        let uid = take_number(&bytes, &mut offset)?;
        let _type = take_number(&bytes, &mut offset)?;
        let length = usize::try_from(take_number(&bytes, &mut offset)?).map_err(|_| invalid())?;
        let data = bytes
            .get(offset..offset.checked_add(length).ok_or_else(invalid)?)
            .ok_or_else(invalid)?;
        offset += length;
        match kind {
            1 => {
                if data.len() > 32 * 1024 * 1024 {
                    return Err("BOOK_KINDLE_LIMIT".into());
                }
                let html = std::str::from_utf8(data).map_err(|_| "BOOK_KINDLE_ENCODING")?;
                markup.push((uid, html.to_owned()));
            }
            2 => {
                if data.len() <= 8 * 1024 * 1024
                    && crate::comic_reader::image_dimensions(data).is_ok()
                {
                    book.resources.insert(uid, data.to_vec());
                }
            }
            3 => {
                let split = data.iter().position(|b| *b == 0).ok_or_else(invalid)?;
                let target = std::str::from_utf8(&data[..split]).map_err(|_| invalid())?;
                let label = std::str::from_utf8(&data[split + 1..]).map_err(|_| invalid())?;
                if target.len() > 150 || label.len() > 4096 || toc.len() >= 10_000 {
                    return Err("BOOK_KINDLE_LIMIT".into());
                }
                toc.push((uid, target.to_owned(), label.to_owned()));
            }
            _ => return Err(invalid()),
        }
    }
    if offset != bytes.len() {
        return Err(invalid());
    }
    for (uid, html) in markup {
        book.chapters
            .extend(split_chapters(uid, &html, &toc, &digest)?);
    }
    if book.chapters.len() > 10_000 {
        return Err("BOOK_KINDLE_LIMIT".into());
    }
    let mut chapters = Vec::new();
    for chapter in book.chapters.drain(..) {
        match safe_html(&chapter.html, &book.resources) {
            Ok(_) => chapters.push(chapter),
            Err(error) if error == "COMIC_NO_PAGES" => {}
            Err(error) => return Err(error),
        }
    }
    book.chapters = chapters;
    if book.chapters.is_empty() {
        return Err("COMIC_NO_PAGES".into());
    }
    let book = Arc::new(book);
    if file_identity(file)? != identity {
        return Err("COMIC_PAGE_CHANGED".into());
    }
    *cache = Some((identity, book.clone()));
    Ok(book)
}

fn split_chapters(
    uid: u64,
    html: &str,
    toc: &[(u64, String, String)],
    digest: &str,
) -> AppResult<Vec<Chapter>> {
    let mut anchors = Vec::new();
    for (_, target, label) in toc.iter().filter(|(part, _, _)| *part == uid) {
        let offset = if target.is_empty() {
            Some(0)
        } else {
            let expression = format!(
                r#"(?i)<[^>]+\b(?:id|name)\s*=\s*["']{}["'][^>]*>"#,
                regex::escape(target)
            );
            regex::Regex::new(&expression)
                .map_err(|_| invalid())?
                .find(html)
                .map(|m| m.start())
        };
        if let Some(offset) = offset {
            anchors.push((offset, label.trim().to_owned()));
        }
    }
    if anchors.is_empty() {
        let pagebreak = regex::Regex::new(r"(?i)<mbp:pagebreak\b[^>]*>").map_err(|_| invalid())?;
        anchors.push((0, String::new()));
        anchors.extend(pagebreak.find_iter(html).map(|m| (m.end(), String::new())));
    } else if !anchors.iter().any(|(offset, _)| *offset == 0) {
        anchors.push((0, String::new()));
    }
    anchors.sort_by_key(|(offset, _)| *offset);
    anchors.dedup_by_key(|(offset, _)| *offset);
    let mut chapters = Vec::new();
    let heading = scraper::Selector::parse("h1,h2,h3,title").map_err(|_| invalid())?;
    for (i, (start, label)) in anchors.iter().enumerate() {
        let end = anchors.get(i + 1).map_or(html.len(), |(offset, _)| *offset);
        let content = &html[*start..end];
        if content.trim().is_empty() {
            continue;
        }
        if content.len() > MAX_CHAPTER {
            return Err("BOOK_KINDLE_LIMIT".into());
        }
        let name = if label.is_empty() {
            scraper::Html::parse_fragment(content)
                .select(&heading)
                .map(|n| n.text().collect::<String>())
                .find(|s| !s.trim().is_empty())
                .unwrap_or_default()
        } else {
            label.clone()
        };
        chapters.push(Chapter {
            name: name.chars().take(200).collect(),
            html: content.into(),
            locator: format!("kindle:{uid}:{start}:{digest}"),
        });
    }
    Ok(chapters)
}

fn file_identity(file: &File) -> AppResult<String> {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return Err("COMIC_READ_FAILED".into());
        }
        Ok(format!(
            "{}:{}:{}:{}:{}:{}:{}",
            info.dwVolumeSerialNumber,
            info.nFileIndexHigh,
            info.nFileIndexLow,
            info.nFileSizeHigh,
            info.nFileSizeLow,
            info.ftLastWriteTime.dwHighDateTime,
            info.ftLastWriteTime.dwLowDateTime
        ))
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::MetadataExt;
        let info = file.metadata().map_err(|_| "COMIC_READ_FAILED")?;
        Ok(format!(
            "{}:{}:{}:{}:{}",
            info.dev(),
            info.ino(),
            info.len(),
            info.mtime(),
            info.mtime_nsec()
        ))
    }
}
#[cfg(test)]
pub fn index(path: &std::path::Path) -> AppResult<Vec<IndexedPage>> {
    index_file(File::open(path).map_err(|_| "COMIC_READ_FAILED")?)
}
pub fn index_file(mut file: File) -> AppResult<Vec<IndexedPage>> {
    let book = parsed(&mut file)?;
    Ok(book
        .chapters
        .iter()
        .enumerate()
        .map(|(i, chapter)| IndexedPage {
            name: if chapter.name.is_empty() {
                (i + 1).to_string()
            } else {
                chapter.name.clone()
            },
            locator: chapter.locator.clone(),
            size: chapter.html.len() as u64,
            modified: String::new(),
            crc: None,
        })
        .collect())
}
pub fn chapter(file: &mut File, locator: &str) -> AppResult<Vec<EpubBlock>> {
    let book = parsed(file)?;
    let chapter = book
        .chapters
        .iter()
        .find(|c| c.locator == locator)
        .ok_or("COMIC_PAGE_CHANGED")?;
    safe_html(&chapter.html, &book.resources)
}
pub fn cover(file: &mut File) -> AppResult<Option<Vec<u8>>> {
    let book = parsed(file)?;
    if let Some(cover) = book.resources.get(&book.cover) {
        return Ok(Some(cover.clone()));
    }
    // Some DRM-free books omit EXTH cover metadata: use the first local image
    // referenced in reading order, never a remote URL or an arbitrary resource.
    for chapter in &book.chapters {
        let document = scraper::Html::parse_fragment(&chapter.html);
        let selector = scraper::Selector::parse("img").map_err(|_| invalid())?;
        for image in document.select(&selector) {
            if let Some(bytes) = image
                .attr("src")
                .and_then(resource_id)
                .and_then(|id| book.resources.get(&id))
            {
                return Ok(Some(bytes.clone()));
            }
        }
    }
    Ok(None)
}
fn resource_id(value: &str) -> Option<u64> {
    let file = value.strip_prefix("resource")?;
    let (id, suffix) = file.split_once('.')?;
    if !matches!(suffix, "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp")
        || id.is_empty()
        || !id.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    id.parse().ok()
}
fn safe_html(html: &str, resources: &HashMap<u64, Vec<u8>>) -> AppResult<Vec<EpubBlock>> {
    let document = scraper::Html::parse_fragment(html);
    let mut stack = vec![(document.tree.root(), false, 0usize)];
    let mut blocks = Vec::new();
    let (mut text, mut runs, mut tag) = (String::new(), Vec::new(), "p".to_string());
    let (mut bold, mut italic, mut sup, mut sub) = (0usize, 0usize, 0usize, 0usize);
    let (mut count, mut images, mut pixels) = (0usize, 0usize, 0u64);
    while let Some((node, closing, depth)) = stack.pop() {
        count += 1;
        if depth > 128 || count > 60_000 {
            return Err("BOOK_KINDLE_LIMIT".into());
        }
        let name = node
            .value()
            .as_element()
            .map(|e| e.name())
            .unwrap_or_default();
        if matches!(
            name,
            "script"
                | "style"
                | "head"
                | "title"
                | "iframe"
                | "object"
                | "audio"
                | "video"
                | "noscript"
                | "template"
                | "svg"
        ) {
            continue;
        }
        if closing {
            if matches!(name, "b" | "strong") {
                bold -= 1;
            }
            if matches!(name, "i" | "em") {
                italic -= 1;
            }
            if name == "sup" {
                sup -= 1;
            }
            if name == "sub" {
                sub -= 1;
            }
            if crate::ebooks::is_block(name) {
                crate::ebooks::flush_text(&mut blocks, &mut text, &mut runs, &tag);
                tag = "p".into();
            }
            continue;
        }
        if crate::ebooks::is_block(name) {
            crate::ebooks::flush_text(&mut blocks, &mut text, &mut runs, &tag);
            tag = if name == "div" { "p" } else { name }.into();
        }
        if matches!(name, "b" | "strong") {
            bold += 1;
        }
        if matches!(name, "i" | "em") {
            italic += 1;
        }
        if name == "sup" {
            sup += 1;
        }
        if name == "sub" {
            sub += 1;
        }
        if let Some(value) = node.value().as_text() {
            let value = if tag == "pre" {
                value.to_string()
            } else {
                value
                    .split_inclusive(char::is_whitespace)
                    .map(|s| {
                        if s.ends_with(char::is_whitespace) {
                            format!("{} ", s.trim_end())
                        } else {
                            s.into()
                        }
                    })
                    .collect::<String>()
            };
            text.push_str(&value);
            runs.push(EpubRun {
                text: value,
                bold: bold > 0,
                italic: italic > 0,
                superscript: sup > 0,
                subscript: sub > 0,
            });
        }
        if name == "br" {
            text.push('\n');
            runs.push(EpubRun {
                text: "\n".into(),
                ..Default::default()
            });
        }
        if name == "img" {
            crate::ebooks::flush_text(&mut blocks, &mut text, &mut runs, &tag);
            if let Some(bytes) = node
                .value()
                .as_element()
                .and_then(|e| e.attr("src"))
                .and_then(resource_id)
                .and_then(|id| resources.get(&id))
            {
                let (w, h) = crate::comic_reader::image_dimensions(bytes)?;
                images += bytes.len();
                pixels += u64::from(w) * u64::from(h);
                if images > 16 * 1024 * 1024 || pixels > 32_000_000 {
                    return Err("COMIC_PAGE_LIMIT".into());
                }
                blocks.push(EpubBlock::Image {
                    data_url: format!(
                        "data:{};base64,{}",
                        crate::comic_reader::image_mime(bytes)?,
                        STANDARD.encode(bytes)
                    ),
                });
            }
        }
        stack.push((node, true, depth));
        stack.extend(node.children().rev().map(|child| (child, false, depth + 1)));
    }
    crate::ebooks::flush_text(&mut blocks, &mut text, &mut runs, &tag);
    if blocks.is_empty() {
        return Err("COMIC_NO_PAGES".into());
    }
    Ok(blocks)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn directory_labels_split_exact_local_targets_without_remote_navigation() {
        let html="<body><p>Preface</p><h1 id='chapter-one'>One</h1><p>First</p><h2 id=\"chapter-two\">Two</h2><p>Second</p></body>";
        let toc = vec![
            (7, "chapter-one".into(), "第一章".into()),
            (7, "chapter-two".into(), "第二章".into()),
            (8, "ignored".into(), "Other part".into()),
        ];
        let chapters = split_chapters(7, html, &toc, "digest").unwrap();
        assert_eq!(chapters.len(), 3);
        assert_eq!(chapters[1].name, "第一章");
        assert_eq!(chapters[2].name, "第二章");
        assert!(!chapters[1].html.contains("Second"));
        assert_ne!(chapters[1].locator, chapters[2].locator);
    }
    #[test]
    fn safe_markup_keeps_inline_semantics_and_blocks_active_content() {
        let blocks = safe_html("<head><title>metadata must not become a reading screen</title></head><h1>第一章</h1><p>Hello <b>bold</b> <i>italic</i>.</p><script>bad()</script><style>bad</style><iframe>bad</iframe><img src='https://example.com/a.png'><img src='../../secret.png'>", &HashMap::new()).unwrap();
        let json = serde_json::to_string(&blocks).unwrap();
        assert!(
            json.contains("第一章")
                && json.contains("\"bold\":true")
                && json.contains("\"italic\":true")
        );
        assert!(!json.contains("bad") && !json.contains("https:") && !json.contains("secret"));
        assert!(!json.contains("metadata must not become a reading screen"));
        assert!(resource_id("resource00042.jpg").is_some());
        assert!(resource_id("resource00042.ttf").is_none());
        assert!(resource_id("../resource00042.jpg").is_none());
    }
    #[test]
    fn invalid_headers_and_deep_markup_fail_closed() {
        assert_eq!(preflight(b"not a book").unwrap_err(), "BOOK_KINDLE_INVALID");
        let nested = format!("{}x{}", "<div>".repeat(130), "</div>".repeat(130));
        assert_eq!(
            safe_html(&nested, &HashMap::new()).err().unwrap(),
            "BOOK_KINDLE_LIMIT"
        );
    }
    #[test]
    #[ignore = "requires explicitly provided read-only real MOBI/KF8 fixtures"]
    fn real_kindle_samples() {
        let root = std::env::var_os("M2SHELF_KINDLE_SAMPLES").expect("M2SHELF_KINDLE_SAMPLES");
        let mut passed = 0;
        for entry in std::fs::read_dir(root).unwrap() {
            let path = entry.unwrap().path();
            if !matches!(crate::ebooks::format(&path), Some("MOBI" | "AZW3")) {
                continue;
            }
            if path.file_name().unwrap().to_string_lossy().contains("drm") {
                let error = index(&path).err().expect("protected sample must fail");
                assert_eq!(error, "BOOK_KINDLE_DRM");
                continue;
            }
            if path.file_name().unwrap().to_string_lossy().contains("dict") {
                continue;
            }
            let before = Sha256::digest(std::fs::read(&path).unwrap());
            let pages = index(&path)
                .unwrap_or_else(|e| panic!("{}: {e}", path.file_name().unwrap().to_string_lossy()));
            let mut file = File::open(&path).unwrap();
            let mut text = 0;
            let mut images = 0;
            for page in &pages {
                let blocks = chapter(&mut file, &page.locator).unwrap();
                text += blocks
                    .iter()
                    .filter(|b| matches!(b, EpubBlock::Text { .. }))
                    .count();
                images += blocks
                    .iter()
                    .filter(|b| matches!(b, EpubBlock::Image { .. }))
                    .count();
            }
            let cover = cover(&mut file).unwrap();
            if let Some(output) = std::env::var_os("M2SHELF_READER_EXPORT") {
                let output = std::path::PathBuf::from(output);
                std::fs::create_dir_all(&output).unwrap();
                if path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("public-domain-")
                {
                    let format = crate::ebooks::format(&path).unwrap();
                    let names: Vec<_> = pages.iter().map(|p| p.name.clone()).collect();
                    std::fs::write(
                        output.join(format!("{format}-chapters.json")),
                        serde_json::to_vec(&names).unwrap(),
                    )
                    .unwrap();
                    for (index, page) in pages.iter().take(4).enumerate() {
                        std::fs::write(
                            output.join(format!("{format}-{index}.json")),
                            serde_json::to_vec(&chapter(&mut file, &page.locator).unwrap())
                                .unwrap(),
                        )
                        .unwrap();
                    }
                }
            }
            println!(
                "sample={} chapters={} text_blocks={} images={} cover={}",
                path.file_name().unwrap().to_string_lossy(),
                pages.len(),
                text,
                images,
                cover.is_some()
            );
            assert!(text + images > 0);
            assert_eq!(
                before,
                Sha256::digest(std::fs::read(&path).unwrap()),
                "read-only source digest"
            );
            passed += 1;
        }
        assert!(
            passed >= 4,
            "need real MOBI6 and KF8/Chinese/compression fixtures"
        );
    }
}
