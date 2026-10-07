//! Bounded PDF indexing and EPUB spine reading. No extraction, scripting or network resources.
use crate::{
    comics::{self, IndexedPage},
    db::AppResult,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use std::{collections::HashMap, fs::File, io::Read, path::Path};

const XML_LIMIT: u64 = 4 * 1024 * 1024;
pub const MAX_PDF_BYTES: u64 = 512 * 1024 * 1024;
pub const PDF_CHUNK_BYTES: u64 = 2 * 1024 * 1024;
pub fn format(path: &Path) -> Option<&'static str> {
    match comics::extension(path).as_str() {
        "pdf" => Some("PDF"),
        "epub" => Some("EPUB"),
        _ => None,
    }
}
pub fn index(path: &Path) -> AppResult<Vec<IndexedPage>> {
    if format(path) == Some("PDF") {
        let file = File::open(path).map_err(|_| "COMIC_READ_FAILED")?;
        let size = file.metadata().map_err(|_| "COMIC_READ_FAILED")?.len();
        if size > MAX_PDF_BYTES {
            return Err("COMIC_PAGE_LIMIT".into());
        }
        let mut bytes = Vec::with_capacity(size as usize);
        file.take(MAX_PDF_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "COMIC_READ_FAILED")?;
        if bytes.len() as u64 > MAX_PDF_BYTES {
            return Err("COMIC_PAGE_LIMIT".into());
        }
        let doc = lopdf::Document::load_mem(&bytes).map_err(|_| "COMIC_DOCUMENT_INVALID")?;
        if doc.is_encrypted() {
            return Err("COMIC_ARCHIVE_ENCRYPTED".into());
        }
        let count = doc.get_pages().len();
        if count == 0 || count > comics::MAX_PAGES {
            return Err("COMIC_PAGE_LIMIT".into());
        }
        Ok((0..count)
            .map(|i| IndexedPage {
                name: format!("{}.pdf", i + 1),
                locator: i.to_string(),
                size: 0,
                modified: String::new(),
                crc: None,
            })
            .collect())
    } else {
        let mut zip = comics::open_archive(File::open(path).map_err(|_| "COMIC_READ_FAILED")?)?;
        let container = read_text(&mut zip, "META-INF/container.xml")?;
        let container = parse_xml(&container)?;
        let opf = container
            .descendants()
            .find(|n| n.has_tag_name("rootfile"))
            .and_then(|n| n.attribute("full-path"))
            .ok_or("COMIC_DOCUMENT_INVALID")?;
        if !comics::valid_entry(opf) {
            return Err("COMIC_ARCHIVE_PATH".into());
        }
        let text = read_text(&mut zip, opf)?;
        let package = parse_xml(&text)?;
        let manifest: HashMap<_, _> = package
            .descendants()
            .filter(|n| n.has_tag_name("item"))
            .filter_map(|n| {
                Some((
                    n.attribute("id")?,
                    (n.attribute("href")?, n.attribute("media-type")?),
                ))
            })
            .collect();
        let mut pages = Vec::new();
        for reference in package
            .descendants()
            .filter(|n| n.has_tag_name("itemref") && n.attribute("linear") != Some("no"))
        {
            let (href, mime) = manifest
                .get(
                    reference
                        .attribute("idref")
                        .ok_or("COMIC_DOCUMENT_INVALID")?,
                )
                .ok_or("COMIC_DOCUMENT_INVALID")?;
            if !matches!(*mime, "application/xhtml+xml" | "text/html") {
                continue;
            }
            let name = resolve(opf, href)?;
            let entry = zip.by_name(&name).map_err(|_| "COMIC_DOCUMENT_INVALID")?;
            if entry.encrypted() {
                return Err("COMIC_ARCHIVE_ENCRYPTED".into());
            }
            if entry.size() > XML_LIMIT || pages.len() >= comics::MAX_PAGES {
                return Err("COMIC_PAGE_LIMIT".into());
            }
            pages.push(IndexedPage {
                name: name.clone(),
                locator: name,
                size: entry.size(),
                modified: String::new(),
                crc: Some(entry.crc32()),
            });
        }
        if pages.is_empty() {
            return Err("COMIC_NO_PAGES".into());
        }
        Ok(pages)
    }
}
fn parse_xml(text: &str) -> AppResult<roxmltree::Document<'_>> {
    let document = roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: 30_000,
        },
    )
    .map_err(|_| "COMIC_DOCUMENT_INVALID".to_string())?;
    // Bound ancestor walks in semantic extraction even for a deeply nested small XML file.
    if document
        .descendants()
        .any(|node| node.ancestors().take(130).count() > 128)
    {
        return Err("COMIC_PAGE_LIMIT".into());
    }
    Ok(document)
}
fn read_text(zip: &mut zip::ZipArchive<File>, name: &str) -> AppResult<String> {
    let mut entry = zip.by_name(name).map_err(|_| "COMIC_DOCUMENT_INVALID")?;
    if entry.encrypted() {
        return Err("COMIC_ARCHIVE_ENCRYPTED".into());
    }
    if entry.size() > XML_LIMIT {
        return Err("COMIC_PAGE_LIMIT".into());
    }
    let mut bytes = Vec::new();
    entry
        .by_ref()
        .take(XML_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "COMIC_DOCUMENT_INVALID")?;
    if bytes.len() as u64 > XML_LIMIT {
        return Err("COMIC_PAGE_LIMIT".into());
    }
    let text = if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        if bytes.len() % 2 != 0 {
            return Err("COMIC_DOCUMENT_INVALID".into());
        }
        let little = bytes[0] == 0xff;
        let units = bytes[2..]
            .chunks_exact(2)
            .map(|pair| {
                if little {
                    u16::from_le_bytes([pair[0], pair[1]])
                } else {
                    u16::from_be_bytes([pair[0], pair[1]])
                }
            })
            .collect::<Vec<_>>();
        String::from_utf16(&units).map_err(|_| "COMIC_DOCUMENT_INVALID")?
    } else {
        String::from_utf8(bytes).map_err(|_| "COMIC_DOCUMENT_INVALID")?
    };
    normalize_xhtml(&text)
}

/// EPUB 2 commonly declares an external XHTML DTD. Remove the declaration locally;
/// never load it or enable entity expansion. Internal subsets remain forbidden.
fn normalize_xhtml(text: &str) -> AppResult<String> {
    let mut value = text.trim_start_matches('\u{feff}').to_string();
    if value.contains("<!ENTITY") {
        return Err("COMIC_DOCUMENT_INVALID".into());
    }
    if let Some(start) = value.find("<!DOCTYPE") {
        let mut quote = None;
        let mut end = None;
        for (offset, c) in value[start + 9..].char_indices() {
            match (quote, c) {
                (Some(q), c) if c == q => quote = None,
                (None, '\'' | '"') => quote = Some(c),
                (None, '[') => return Err("COMIC_DOCUMENT_INVALID".into()),
                (None, '>') => {
                    end = Some(start + 9 + offset + 1);
                    break;
                }
                _ => {}
            }
        }
        value.replace_range(start..end.ok_or("COMIC_DOCUMENT_INVALID")?, "");
    }
    for (entity, character) in [
        ("nbsp", "\u{a0}"),
        ("ensp", "\u{2002}"),
        ("emsp", "\u{2003}"),
        ("thinsp", "\u{2009}"),
        ("mdash", "—"),
        ("ndash", "–"),
        ("hellip", "…"),
        ("lsquo", "‘"),
        ("rsquo", "’"),
        ("ldquo", "“"),
        ("rdquo", "”"),
        ("copy", "©"),
        ("reg", "®"),
    ] {
        value = value.replace(&format!("&{entity};"), character);
    }
    Ok(value)
}
fn resolve(base: &str, href: &str) -> AppResult<String> {
    let href = href.split('#').next().unwrap_or_default();
    // EPUB manifest references are URLs; resolve encoded spaces and Unicode once,
    // then apply the same archive-only path checks to the decoded value.
    let mut decoded = Vec::with_capacity(href.len());
    let mut chars = href.as_bytes().iter().copied();
    while let Some(byte) = chars.next() {
        if byte == b'%' {
            let hi = chars
                .next()
                .and_then(|b| (b as char).to_digit(16))
                .ok_or("COMIC_ARCHIVE_PATH")?;
            let lo = chars
                .next()
                .and_then(|b| (b as char).to_digit(16))
                .ok_or("COMIC_ARCHIVE_PATH")?;
            decoded.push((hi * 16 + lo) as u8);
        } else {
            decoded.push(byte);
        }
    }
    let href = String::from_utf8(decoded).map_err(|_| "COMIC_ARCHIVE_PATH")?;
    if href.contains([':', '\\', '\0']) || href.starts_with('/') {
        return Err("COMIC_ARCHIVE_PATH".into());
    }
    let mut parts: Vec<_> = base.split('/').collect();
    parts.pop();
    for part in href.split('/') {
        match part {
            "." | "" => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err("COMIC_ARCHIVE_PATH".into());
                }
            }
            _ => parts.push(part),
        }
    }
    let value = parts.join("/");
    if !comics::valid_entry(&value) {
        return Err("COMIC_ARCHIVE_PATH".into());
    }
    Ok(value)
}
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum EpubBlock {
    Text {
        text: String,
        tag: String,
        runs: Vec<EpubRun>,
    },
    Image {
        data_url: String,
    },
}
#[derive(Clone, Default, Serialize)]
pub struct EpubRun {
    text: String,
    bold: bool,
    italic: bool,
    superscript: bool,
    subscript: bool,
}
pub fn chapter(
    file: File,
    locator: &str,
    size: u64,
    crc: Option<u32>,
) -> AppResult<Vec<EpubBlock>> {
    let mut zip = comics::open_archive(file)?;
    {
        let entry = zip.by_name(locator).map_err(|_| "COMIC_PAGE_CHANGED")?;
        if entry.size() != size || crc != Some(entry.crc32()) {
            return Err("COMIC_PAGE_CHANGED".into());
        }
    }
    let text = read_text(&mut zip, locator)?;
    let doc = parse_xml(&text)?;
    let mut blocks = Vec::new();
    let mut paragraph = String::new();
    let mut runs = Vec::new();
    let mut tag = "p".to_string();
    let mut image_bytes = 0;
    let mut image_pixels = 0_u64;
    let mut traversal = vec![(doc.root(), false)];
    while let Some((n, closing)) = traversal.pop() {
        if closing {
            if is_block(n.tag_name().name()) {
                flush_text(&mut blocks, &mut paragraph, &mut runs, &tag);
                tag = "p".into();
            }
            continue;
        }
        if n.ancestors().any(|a| {
            matches!(
                a.tag_name().name(),
                "script" | "style" | "head" | "iframe" | "object" | "audio" | "video" | "noscript"
            )
        }) {
            continue;
        }
        traversal.push((n, true));
        traversal.extend(n.children().rev().map(|child| (child, false)));
        if n.is_element() && is_block(n.tag_name().name()) {
            flush_text(&mut blocks, &mut paragraph, &mut runs, &tag);
            tag = if n.has_tag_name("div") {
                "p"
            } else {
                n.tag_name().name()
            }
            .to_string();
        }
        if n.is_text() {
            let raw = n.text().unwrap_or_default();
            let text = if tag == "pre" {
                raw.to_string()
            } else {
                // Collapse source formatting whitespace while retaining inline word boundaries.
                let mut text = String::new();
                let mut space = false;
                for c in raw.chars() {
                    if c.is_ascii_whitespace() {
                        if !space {
                            text.push(' ');
                        }
                        space = true;
                    } else {
                        text.push(c);
                        space = false;
                    }
                }
                text
            };
            if paragraph.is_empty() && text.trim().is_empty() {
                continue;
            }
            paragraph.push_str(&text);
            runs.push(EpubRun {
                text,
                bold: n
                    .ancestors()
                    .any(|a| matches!(a.tag_name().name(), "b" | "strong")),
                italic: n
                    .ancestors()
                    .any(|a| matches!(a.tag_name().name(), "i" | "em")),
                superscript: n.ancestors().any(|a| a.has_tag_name("sup")),
                subscript: n.ancestors().any(|a| a.has_tag_name("sub")),
            });
        }
        if n.has_tag_name("br") {
            paragraph.push('\n');
            runs.push(EpubRun {
                text: "\n".into(),
                ..Default::default()
            });
        }
        if n.is_element() && matches!(n.tag_name().name(), "img" | "image") {
            flush_text(&mut blocks, &mut paragraph, &mut runs, &tag);
            let Some(href) = n
                .attribute("src")
                .or_else(|| n.attribute("href"))
                .or_else(|| n.attribute(("http://www.w3.org/1999/xlink", "href")))
            else {
                continue;
            };
            // Ignore external illustrations rather than loading or executing them.
            if href.contains(':') || href.starts_with('/') {
                continue;
            }
            let name = resolve(locator, href)?;
            if !comics::is_image(Path::new(&name)) {
                continue;
            }
            let mut entry = zip.by_name(&name).map_err(|_| "COMIC_DOCUMENT_INVALID")?;
            if entry.encrypted()
                || entry.size() > 8 * 1024 * 1024
                || image_bytes + entry.size() > 16 * 1024 * 1024
            {
                return Err("COMIC_PAGE_LIMIT".into());
            }
            let mut bytes = Vec::new();
            entry
                .by_ref()
                .take(8 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| "COMIC_DOCUMENT_INVALID")?;
            if bytes.len() > 8 * 1024 * 1024 {
                return Err("COMIC_PAGE_LIMIT".into());
            }
            image_bytes += bytes.len() as u64;
            let (width, height) = crate::comic_reader::image_dimensions(
                &bytes,
                &comics::extension(Path::new(&name)),
            )?;
            image_pixels += u64::from(width) * u64::from(height);
            if image_pixels > 32_000_000 {
                return Err("COMIC_PAGE_LIMIT".into());
            }
            let ext = comics::extension(Path::new(&name));
            let mime = if matches!(ext.as_str(), "jpg" | "jpeg") {
                "jpeg"
            } else {
                ext.as_str()
            };
            blocks.push(EpubBlock::Image {
                data_url: format!("data:image/{mime};base64,{}", STANDARD.encode(bytes)),
            });
        }
    }
    flush_text(&mut blocks, &mut paragraph, &mut runs, &tag);
    Ok(blocks)
}
fn is_block(tag: &str) -> bool {
    matches!(
        tag,
        "p" | "div" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "li" | "blockquote" | "pre"
    )
}
fn flush_text(blocks: &mut Vec<EpubBlock>, text: &mut String, runs: &mut Vec<EpubRun>, tag: &str) {
    if tag != "pre" {
        let leading = runs
            .iter()
            .position(|run| !run.text.trim_start().is_empty())
            .unwrap_or(runs.len());
        runs.drain(..leading);
        if let Some(run) = runs.first_mut() {
            run.text = run.text.trim_start().to_string();
        }
        while runs
            .last()
            .is_some_and(|run| run.text.trim_end().is_empty())
        {
            runs.pop();
        }
        if let Some(run) = runs.last_mut() {
            run.text = run.text.trim_end().to_string();
        }
        *text = text.trim().to_string();
    }
    if !text.trim().is_empty() {
        blocks.push(EpubBlock::Text {
            text: std::mem::take(text),
            tag: tag.into(),
            runs: std::mem::take(runs),
        });
    } else {
        text.clear();
        runs.clear();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn standard_xhtml_doctype_is_local_but_custom_entities_stay_forbidden() {
        let text = normalize_xhtml("<!DOCTYPE html PUBLIC '-//W3C//DTD XHTML 1.1//EN' 'http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd'><html><body><p>A&nbsp;B</p></body></html>").unwrap();
        let doc = parse_xml(&text).unwrap();
        assert!(doc.descendants().any(|n| n.text() == Some("A\u{a0}B")));
        for text in [
            "<!DOCTYPE html [<!ENTITY x 'value'>]><html/>",
            "<!DOCTYPE html [<!ENTITY x SYSTEM 'file:///private'>]><html/>",
            "<!DOCTYPE html [ ]><html/>",
            "<!DOCTYPE html",
        ] {
            assert!(normalize_xhtml(text).is_err());
        }
    }

    #[test]
    #[ignore = "Requires an explicitly provided local fixture list; never reads a media library by default"]
    fn owner_epub_chapters_are_read_only_and_renderable() {
        use sha2::{Digest, Sha256};
        let list = std::env::var("M2SHELF_EPUB_FIXTURE_LIST").expect("explicit local fixture list");
        let paths: Vec<String> = serde_json::from_slice(&std::fs::read(list).unwrap()).unwrap();
        let output = std::env::var_os("M2SHELF_EPUB_LAYOUT_OUTPUT").map(std::path::PathBuf::from);
        if let Some(directory) = &output {
            std::fs::create_dir_all(directory).unwrap();
        }
        for (i, path) in paths.iter().enumerate() {
            let before = format!("{:x}", Sha256::digest(std::fs::read(path).unwrap()));
            let pages = index(Path::new(path)).unwrap();
            let mut images = 0;
            let mut texts = 0;
            for (chapter_index, page) in pages.iter().enumerate() {
                let blocks = chapter(
                    File::open(path).unwrap(),
                    &page.locator,
                    page.size,
                    page.crc,
                )
                .unwrap();
                if chapter_index < 6 {
                    if let Some(directory) = &output {
                        std::fs::write(
                            directory.join(format!("fixture-{}-{}.json", i + 1, chapter_index)),
                            serde_json::to_vec(&blocks).unwrap(),
                        )
                        .unwrap();
                    }
                }
                assert!(
                    !blocks.is_empty(),
                    "fixture {} chapter produced no content",
                    i + 1
                );
                for block in blocks {
                    match block {
                        EpubBlock::Image { .. } => images += 1,
                        EpubBlock::Text { .. } => texts += 1,
                    }
                }
            }
            assert_eq!(
                before,
                format!("{:x}", Sha256::digest(std::fs::read(path).unwrap()))
            );
            println!("fixture {}: {} chapters, {} text blocks, {} local images, source SHA-256 unchanged",i+1,pages.len(),texts,images);
        }
    }

    #[test]
    fn epub_spine_order_text_and_embedded_images_are_read_without_scripts() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("book.epub");
        let mut writer = zip::ZipWriter::new(File::create(&path).unwrap());
        for (name, text) in [
            ("META-INF/container.xml", "<container><rootfiles><rootfile full-path='OPS/book.opf'/></rootfiles></container>"),
            ("OPS/book.opf", "<package><manifest><item id='one' href='one%20chapter.xhtml' media-type='application/xhtml+xml'/><item id='two' href='two.xhtml' media-type='application/xhtml+xml'/></manifest><spine><itemref idref='two'/><itemref idref='one'/></spine></package>"),
            ("OPS/two.xhtml", "<html><head><title>Hidden title</title></head><body><p>第二章</p><script>malicious()</script><img src='cover.png'/><img src='https://example.org/x.png'/><p>文本结束</p></body></html>"),
            ("OPS/one chapter.xhtml", "<html><body><p>第一章</p></body></html>"),
        ] {
            writer.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
            writer.write_all(text.as_bytes()).unwrap();
        }
        writer
            .start_file("OPS/cover.png", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(&STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jH3sAAAAASUVORK5CYII=").unwrap()).unwrap();
        writer.finish().unwrap();
        let pages = index(&path).unwrap();
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].locator, "OPS/two.xhtml");
        assert_eq!(pages[1].locator, "OPS/one chapter.xhtml");
        let blocks = chapter(
            File::open(&path).unwrap(),
            &pages[0].locator,
            pages[0].size,
            pages[0].crc,
        )
        .unwrap();
        let json = serde_json::to_string(&blocks).unwrap();
        assert!(json.contains("第二章"));
        assert!(json.contains("data:image/png;base64,"));
        assert!(!json.contains("malicious"));
        assert!(!json.contains("Hidden title"));
        assert!(!json.contains("https://"));
        assert!(chapter(
            File::open(path).unwrap(),
            &pages[0].locator,
            pages[0].size + 1,
            pages[0].crc
        )
        .is_err());
    }
    #[test]
    fn epub_paths_are_local_and_bounded() {
        assert_eq!(
            resolve("OPS/1.xhtml", "pages/%E4%B8%AD%20%E6%96%87.xhtml").unwrap(),
            "OPS/pages/中 文.xhtml"
        );
        assert_eq!(
            resolve("OPS/chapters/1.xhtml", "../images/1.jpg").unwrap(),
            "OPS/images/1.jpg"
        );
        for name in [
            "../../../secret",
            "https://example.org/x",
            "C:/file",
            "/absolute",
            "..\\file",
            "%2e%2e/%2e%2e/secret",
            "%3A%2F%2Fx",
            "bad%xx",
        ] {
            assert!(resolve("OPS/1.xhtml", name).is_err());
        }
    }
    #[test]
    fn xml_rejects_external_entities() {
        assert!(parse_xml("<!DOCTYPE x [<!ENTITY a SYSTEM 'file:///x'>]><x>&a;</x>").is_err());
    }
    #[test]
    fn excessive_xml_depth_is_bounded_before_semantic_walks() {
        assert!(parse_xml(&format!(
            "{}text{}",
            "<span>".repeat(256),
            "</span>".repeat(256)
        ))
        .is_err());
        assert!(parse_xml(&format!(
            "{}text{}",
            "<span>".repeat(30),
            "</span>".repeat(30)
        ))
        .is_ok());
    }
}
