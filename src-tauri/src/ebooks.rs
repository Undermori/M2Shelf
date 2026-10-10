//! Bounded PDF indexing and EPUB spine reading. No extraction, scripting or network resources.
use crate::{
    comics::{self, IndexedPage},
    db::AppResult,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{collections::HashMap, fs::File, io::Read, path::Path};

const XML_LIMIT: u64 = 4 * 1024 * 1024;
fn resource_hash(locator: &str) -> u64 {
    u64::from_le_bytes(Sha256::digest(locator.as_bytes())[..8].try_into().unwrap())
}
pub const MAX_PDF_BYTES: u64 = 512 * 1024 * 1024;
pub const PDF_CHUNK_BYTES: u64 = 2 * 1024 * 1024;
pub fn format(path: &Path) -> Option<&'static str> {
    match comics::extension(path).as_str() {
        "pdf" => Some("PDF"),
        "epub" => Some("EPUB"),
        "txt" => Some("TXT"),
        "mobi" => Some("MOBI"),
        "azw3" => Some("AZW3"),
        _ => None,
    }
}
pub fn index(path: &Path) -> AppResult<Vec<IndexedPage>> {
    let format = format(path).ok_or("COMIC_DOCUMENT_INVALID")?;
    index_file(File::open(path).map_err(|_| "COMIC_READ_FAILED")?, format)
}
pub(crate) fn index_file(file: File, format: &str) -> AppResult<Vec<IndexedPage>> {
    if matches!(format, "MOBI" | "AZW3") {
        return crate::kindle_books::index_file(file);
    }
    if format == "TXT" {
        return crate::text_books::index_file(file);
    }
    if format == "PDF" {
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
        let mut zip = comics::open_archive(file)?;
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
            if pages.len() >= comics::MAX_PAGES {
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
pub(crate) fn navigation(file: File) -> AppResult<Vec<(String, String, Option<String>)>> {
    let mut zip = comics::open_archive(file)?;
    let container_text = read_text(&mut zip, "META-INF/container.xml")?;
    let container = parse_xml(&container_text)?;
    let opf = container
        .descendants()
        .find(|n| n.has_tag_name("rootfile"))
        .and_then(|n| n.attribute("full-path"))
        .ok_or("COMIC_DOCUMENT_INVALID")?;
    if !comics::valid_entry(opf) {
        return Err("COMIC_ARCHIVE_PATH".into());
    }
    let package_text = read_text(&mut zip, opf)?;
    let package = parse_xml(&package_text)?;
    let items = package
        .descendants()
        .filter(|n| n.has_tag_name("item"))
        .collect::<Vec<_>>();
    let nav = items
        .iter()
        .find(|n| {
            n.attribute("properties")
                .is_some_and(|v| v.split_whitespace().any(|p| p == "nav"))
        })
        .or_else(|| {
            items
                .iter()
                .find(|n| n.attribute("media-type") == Some("application/x-dtbncx+xml"))
        });
    let Some(href) = nav.and_then(|n| n.attribute("href")) else {
        return Ok(Vec::new());
    };
    let nav_path = resolve(opf, href)?;
    let nav_text = read_text(&mut zip, &nav_path)?;
    let doc = parse_xml(&nav_text)?;
    let mut result = Vec::new();
    for n in doc.descendants() {
        let entry = if n.has_tag_name("navPoint") {
            let href = n
                .children()
                .find(|c| c.has_tag_name("content"))
                .and_then(|c| c.attribute("src"));
            let label = n.children().find(|c| c.has_tag_name("navLabel")).map(|c| {
                c.descendants()
                    .filter(|v| v.is_text())
                    .map(|v| v.text().unwrap_or_default())
                    .collect::<String>()
            });
            href.zip(label)
        } else if n.has_tag_name("a")
            && n.ancestors().any(|a| {
                a.has_tag_name("nav")
                    && a.attributes().any(|p| {
                        p.name() == "type" && p.value().split_whitespace().any(|v| v == "toc")
                    })
            })
        {
            n.attribute("href").map(|h| {
                (
                    h,
                    n.descendants()
                        .filter(|v| v.is_text())
                        .map(|v| v.text().unwrap_or_default())
                        .collect::<String>(),
                )
            })
        } else {
            None
        };
        if let Some((href, title)) = entry {
            if href.contains(':') || href.starts_with('/') {
                continue;
            }
            let (path, fragment) = href
                .split_once('#')
                .map_or((href, None), |(p, f)| (p, Some(f)));
            let Ok(locator) = resolve(&nav_path, path) else {
                continue;
            };
            let title = title
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(200)
                .collect::<String>();
            if title.is_empty() {
                continue;
            }
            let fragment = fragment
                .filter(|f| !f.is_empty() && f.len() <= 512)
                .map(|f| decode_component(f).unwrap_or_else(|_| f.to_string()));
            result.push((locator, title, fragment));
            if result.len() >= comics::MAX_PAGES {
                break;
            }
        }
    }
    Ok(result)
}

fn parse_xml(text: &str) -> AppResult<roxmltree::Document<'_>> {
    let document = roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: 30_000,
        },
    )
    .map_err(|error| {
        if matches!(error, roxmltree::Error::NodesLimitReached) {
            eprintln!("EPUB limit=xml_nodes max=30000 actual_at_least=30001");
            "COMIC_PAGE_LIMIT".to_string()
        } else {
            "COMIC_DOCUMENT_INVALID".to_string()
        }
    })?;
    // Bound ancestor walks in semantic extraction even for a deeply nested small XML file.
    if let Some(depth) = document
        .descendants()
        .map(|node| node.ancestors().take(130).count())
        .find(|depth| *depth > 128)
    {
        eprintln!("EPUB limit=xml_depth max=128 actual_at_least={depth}");
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
        eprintln!(
            "EPUB limit=chapter_xml max={} actual={} resource_hash={:08x}",
            XML_LIMIT,
            entry.size(),
            resource_hash(name)
        );
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
fn decode_component(href: &str) -> AppResult<String> {
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
    String::from_utf8(decoded).map_err(|_| "COMIC_ARCHIVE_PATH".to_string())
}

fn resolve(base: &str, href: &str) -> AppResult<String> {
    let href = decode_component(href.split('#').next().unwrap_or_default())?;
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

/// Read only embedded images; missing/vector-only covers use the normal placeholder.
/// These bytes still pass the common image limits before entering the poster cache.
pub(crate) fn cover(file: File, format: &str) -> AppResult<Option<Vec<u8>>> {
    if format == "PDF" {
        let size = file.metadata().map_err(|_| "COMIC_READ_FAILED")?.len();
        if size > MAX_PDF_BYTES {
            return Err("COMIC_PAGE_LIMIT".into());
        }
        let mut bytes = Vec::new();
        file.take(MAX_PDF_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "COMIC_READ_FAILED")?;
        if bytes.len() as u64 > MAX_PDF_BYTES {
            return Err("COMIC_PAGE_LIMIT".into());
        }
        let doc = lopdf::Document::load_mem(&bytes).map_err(|_| "COMIC_DOCUMENT_INVALID")?;
        if doc.is_encrypted() {
            return Ok(None);
        }
        let Some((_, page)) = doc.get_pages().into_iter().next() else {
            return Ok(None);
        };
        let mut selected: Option<(u64, Vec<u8>)> = None;
        for image in doc
            .get_page_images(page)
            .unwrap_or_default()
            .into_iter()
            .take(32)
        {
            // A directly embedded JPEG from the first page avoids unbounded PDF rasterization.
            if image
                .filters
                .as_ref()
                .is_some_and(|filters| filters.len() == 1 && filters[0] == "DCTDecode")
                && image.content.len() <= 8 * 1024 * 1024
            {
                if let Ok((w, h)) = crate::comic_reader::image_dimensions(image.content) {
                    let pixels = u64::from(w) * u64::from(h);
                    if selected.as_ref().is_none_or(|old| pixels > old.0) {
                        selected = Some((pixels, image.content.to_vec()));
                    }
                }
            }
        }
        return Ok(selected.map(|entry| entry.1));
    }
    if format != "EPUB" {
        return Ok(None);
    }
    let mut zip = comics::open_archive(file)?;
    let container_text = read_text(&mut zip, "META-INF/container.xml")?;
    let container = parse_xml(&container_text)?;
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
    let cover_id = package
        .descendants()
        .find(|n| n.has_tag_name("meta") && n.attribute("name") == Some("cover"))
        .and_then(|n| n.attribute("content"));
    let mut names: Vec<String> = package
        .descendants()
        .filter(|n| {
            n.has_tag_name("item")
                && (n.attribute("id") == cover_id && cover_id.is_some()
                    || n.attribute("properties")
                        .is_some_and(|p| p.split_whitespace().any(|p| p == "cover-image")))
        })
        .filter_map(|n| n.attribute("href"))
        .filter_map(|href| resolve(opf, href).ok())
        .collect();
    // EPUB2 often supplies a cover XHTML document via guide or the first spine item.
    let manifest: HashMap<_, _> = package
        .descendants()
        .filter(|n| n.has_tag_name("item"))
        .filter_map(|n| Some((n.attribute("id")?, n.attribute("href")?)))
        .collect();
    let chapter = package
        .descendants()
        .find(|n| n.has_tag_name("reference") && n.attribute("type") == Some("cover"))
        .and_then(|n| n.attribute("href"))
        .or_else(|| {
            package
                .descendants()
                .find(|n| n.has_tag_name("itemref"))
                .and_then(|n| n.attribute("idref"))
                .and_then(|id| manifest.get(id).copied())
        });
    if let Some(name) = chapter.and_then(|href| resolve(opf, href).ok()) {
        if let Ok(text) = read_text(&mut zip, &name) {
            if let Ok(doc) = parse_xml(&text) {
                names.extend(
                    doc.descendants()
                        .filter(|n| n.has_tag_name("img") || n.has_tag_name("image"))
                        .filter_map(|n| {
                            n.attribute("src")
                                .or_else(|| n.attribute("href"))
                                .or_else(|| n.attribute(("http://www.w3.org/1999/xlink", "href")))
                        })
                        .take(8)
                        .filter_map(|href| resolve(&name, href).ok()),
                );
            }
        }
    }
    for name in names.into_iter().take(8) {
        let Ok(mut entry) = zip.by_name(&name) else {
            continue;
        };
        if entry.encrypted() || entry.size() > 8 * 1024 * 1024 {
            continue;
        }
        let mut bytes = Vec::new();
        if entry
            .by_ref()
            .take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .is_ok()
            && bytes.len() <= 8 * 1024 * 1024
            && crate::comic_reader::validate_image(&bytes).is_ok()
        {
            return Ok(Some(bytes));
        }
    }
    Ok(None)
}
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum EpubBlock {
    Anchor {
        id: String,
    },
    Text {
        text: String,
        tag: String,
        runs: Vec<EpubRun>,
    },
    Image {
        data_url: String,
    },
    ImageReference {
        locator: String,
        size: u64,
        crc32: u32,
    },
}
#[derive(Clone, Default, Serialize)]
pub struct EpubRun {
    pub(crate) text: String,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    pub(crate) superscript: bool,
    pub(crate) subscript: bool,
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
    let text = read_text(&mut zip, locator).map_err(|e| {
        if e == "COMIC_PAGE_LIMIT" {
            "EPUB_CHAPTER_LIMIT".into()
        } else {
            e
        }
    })?;
    let doc = parse_xml(&text).map_err(|e| {
        if e == "COMIC_PAGE_LIMIT" {
            eprintln!(
                "EPUB structure resource_hash={:016x}",
                resource_hash(locator)
            );
            "EPUB_STRUCTURE_LIMIT".into()
        } else {
            e
        }
    })?;
    let mut blocks = Vec::new();
    let mut paragraph = String::new();
    let mut runs = Vec::new();
    let mut tag = "p".to_string();
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
        if let Some(id) = n
            .attribute("id")
            .filter(|s| !s.is_empty() && s.len() <= 512)
        {
            flush_text(&mut blocks, &mut paragraph, &mut runs, &tag);
            blocks.push(EpubBlock::Anchor { id: id.to_string() });
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
            let entry = zip.by_name(&name).map_err(|_| "COMIC_DOCUMENT_INVALID")?;
            if entry.encrypted() {
                return Err("COMIC_ARCHIVE_ENCRYPTED".into());
            }
            // No raster decoding or base64 images in the chapter IPC. Each illustration is
            // independently read on demand through the same opened-handle/Root boundary.
            blocks.push(EpubBlock::ImageReference {
                locator: name,
                size: entry.size(),
                crc32: entry.crc32(),
            });
        }
    }
    flush_text(&mut blocks, &mut paragraph, &mut runs, &tag);
    Ok(blocks)
}
/// Read one explicitly referenced illustration; callers validate source identity and revision.
pub fn illustration(
    file: File,
    chapter_locator: &str,
    chapter_size: u64,
    chapter_crc: Option<u32>,
    block_index: usize,
) -> AppResult<Vec<u8>> {
    let blocks = chapter(
        file.try_clone().map_err(|_| "COMIC_READ_FAILED")?,
        chapter_locator,
        chapter_size,
        chapter_crc,
    )?;
    let Some(EpubBlock::ImageReference {
        locator,
        size,
        crc32,
    }) = blocks.get(block_index)
    else {
        return Err("COMIC_ARCHIVE_PATH".into());
    };
    if *size > 8 * 1024 * 1024 {
        eprintln!(
            "EPUB limit=illustration_bytes max={} actual={} resource_hash={:08x}",
            8 * 1024 * 1024,
            size,
            resource_hash(locator)
        );
        return Err("EPUB_IMAGE_LIMIT".into());
    }
    let mut zip = comics::open_archive(file)?;
    let mut entry = zip.by_name(locator).map_err(|_| "COMIC_PAGE_CHANGED")?;
    if entry.encrypted() || entry.size() != *size || entry.crc32() != *crc32 {
        return Err("COMIC_PAGE_CHANGED".into());
    }
    let mut bytes = Vec::with_capacity(*size as usize);
    entry
        .by_ref()
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "COMIC_PAGE_CHANGED")?;
    if bytes.len() as u64 != *size {
        return Err("COMIC_PAGE_CHANGED".into());
    }
    crate::comic_reader::validate_image(&bytes)?;
    Ok(bytes)
}

pub(crate) fn is_block(tag: &str) -> bool {
    matches!(
        tag,
        "p" | "div" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "li" | "blockquote" | "pre"
    )
}
pub(crate) fn flush_text(
    blocks: &mut Vec<EpubBlock>,
    text: &mut String,
    runs: &mut Vec<EpubRun>,
    tag: &str,
) {
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
    use base64::{engine::general_purpose::STANDARD, Engine};
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
                        EpubBlock::Image { .. } | EpubBlock::ImageReference { .. } => images += 1,
                        EpubBlock::Text { .. } => texts += 1,
                        EpubBlock::Anchor { .. } => {}
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
        assert!(json.contains("imageReference"));
        assert!(!json.contains("base64"));
        let image_index = blocks
            .iter()
            .position(|b| matches!(b, EpubBlock::ImageReference { .. }))
            .unwrap();
        let bytes = illustration(
            File::open(&path).unwrap(),
            &pages[0].locator,
            pages[0].size,
            pages[0].crc,
            image_index,
        )
        .unwrap();
        assert!(bytes.starts_with(b"\x89PNG"));
        assert!(illustration(
            File::open(&path).unwrap(),
            &pages[0].locator,
            pages[0].size,
            pages[0].crc,
            0
        )
        .is_err());
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
    fn illustrated_fixture(
        chapter_text: &str,
        image_size: usize,
    ) -> (tempfile::TempDir, std::path::PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("synthetic.epub");
        let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
        for (name,text) in [("META-INF/container.xml","<container><rootfiles><rootfile full-path='book.opf'/></rootfiles></container>"),("book.opf","<package><manifest><item id='one' href='one.xhtml' media-type='application/xhtml+xml'/><item id='two' href='two.xhtml' media-type='application/xhtml+xml'/></manifest><spine><itemref idref='one'/><itemref idref='two'/></spine></package>"),("one.xhtml",chapter_text),("two.xhtml","<html><body><p>Usable chapter</p></body></html>")]{zip.start_file(name,zip::write::SimpleFileOptions::default()).unwrap();zip.write_all(text.as_bytes()).unwrap();}
        zip.start_file("image.png", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&vec![0; image_size]).unwrap();
        zip.finish().unwrap();
        (temp, path)
    }
    #[test]
    fn epub_many_images_do_not_expand_chapter_ipc() {
        let text = format!(
            "<html><body><p>Readable</p>{}</body></html>",
            "<img src='image.png'/>".repeat(100)
        );
        let (_temp, path) = illustrated_fixture(&text, 1024 * 1024);
        let pages = index(&path).unwrap();
        let blocks = chapter(
            File::open(path).unwrap(),
            &pages[0].locator,
            pages[0].size,
            pages[0].crc,
        )
        .unwrap();
        assert_eq!(
            blocks
                .iter()
                .filter(|b| matches!(b, EpubBlock::ImageReference { .. }))
                .count(),
            100
        );
        assert!(serde_json::to_vec(&blocks).unwrap().len() < 20000);
    }
    #[test]
    fn oversize_illustration_does_not_block_text_or_other_chapters() {
        let (_temp, path) = illustrated_fixture(
            "<html><body><p>Readable</p><img src='image.png'/></body></html>",
            8 * 1024 * 1024 + 1,
        );
        let pages = index(&path).unwrap();
        let blocks = chapter(
            File::open(&path).unwrap(),
            &pages[0].locator,
            pages[0].size,
            pages[0].crc,
        )
        .unwrap();
        assert!(matches!(&blocks[0], EpubBlock::Text { .. }));
        assert_eq!(
            illustration(
                File::open(&path).unwrap(),
                &pages[0].locator,
                pages[0].size,
                pages[0].crc,
                1
            )
            .unwrap_err(),
            "EPUB_IMAGE_LIMIT"
        );
        assert!(chapter(
            File::open(path).unwrap(),
            &pages[1].locator,
            pages[1].size,
            pages[1].crc
        )
        .is_ok());
    }
    #[test]
    fn oversize_and_deep_chapters_keep_contents_and_other_chapters() {
        for text in [
            format!(
                "<html><body><p>{}</p></body></html>",
                "x".repeat(XML_LIMIT as usize)
            ),
            format!(
                "<html>{}x{}</html>",
                "<div>".repeat(140),
                "</div>".repeat(140)
            ),
        ] {
            let (_temp, path) = illustrated_fixture(&text, 1);
            let pages = index(&path).unwrap();
            assert_eq!(pages.len(), 2);
            let error = chapter(
                File::open(&path).unwrap(),
                &pages[0].locator,
                pages[0].size,
                pages[0].crc,
            )
            .err()
            .unwrap();
            assert!(matches!(
                error.as_str(),
                "EPUB_CHAPTER_LIMIT" | "EPUB_STRUCTURE_LIMIT"
            ));
            assert!(chapter(
                File::open(path).unwrap(),
                &pages[1].locator,
                pages[1].size,
                pages[1].crc
            )
            .is_ok());
        }
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
    #[test]
    fn semantic_navigation_maps_spine_locators_and_safe_fragment_blocks() {
        for ncx in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("toc.epub");
            let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
            let nav_item = if ncx {
                "<item id='toc' href='toc.ncx' media-type='application/x-dtbncx+xml'/>"
            } else {
                "<item id='toc' href='nav.xhtml' properties='nav' media-type='application/xhtml+xml'/>"
            };
            let opf=format!("<package><manifest>{nav_item}<item id='one' href='one.xhtml' media-type='application/xhtml+xml'/><item id='two' href='two.xhtml' media-type='application/xhtml+xml'/></manifest><spine><itemref idref='two'/><itemref idref='one'/></spine></package>");
            let nav = if ncx {
                "<ncx><navMap><navPoint><navLabel><text>River</text></navLabel><content src='two.xhtml'/></navPoint><navPoint><navLabel><text>River</text></navLabel><content src='two.xhtml#part%20two'/></navPoint></navMap></ncx>"
            } else {
                "<!DOCTYPE html><html xmlns:epub='http://www.idpf.org/2007/ops'><nav epub:type='toc'><a href='two.xhtml'>River</a><a href='two.xhtml#part%20two'>River</a><a href='https://invalid.example/book'>Remote</a><a href='../../escape.xhtml'>Invalid</a></nav></html>"
            };
            for (name,text) in [("META-INF/container.xml","<container><rootfiles><rootfile full-path='OPS/book.opf'/></rootfiles></container>"),("OPS/book.opf",opf.as_str()),(if ncx {"OPS/toc.ncx"}else{"OPS/nav.xhtml"},nav),("OPS/one.xhtml","<html><body><p>First chapter</p></body></html>"),("OPS/two.xhtml","<html><body><p>River begins</p><h2 id='part two'>Section title</h2><p>River ends</p><script id='unsafe'>bad()</script></body></html>")] {zip.start_file(name,zip::write::SimpleFileOptions::default()).unwrap();zip.write_all(text.as_bytes()).unwrap();}
            zip.finish().unwrap();
            let pages = index(&path).unwrap();
            assert_eq!(pages[0].locator, "OPS/two.xhtml");
            assert_eq!(pages[1].locator, "OPS/one.xhtml");
            let nav = navigation(File::open(&path).unwrap()).unwrap();
            assert_eq!(nav.len(), 2);
            assert_eq!(nav[0], ("OPS/two.xhtml".into(), "River".into(), None));
            assert_eq!(nav[1].2.as_deref(), Some("part two"));
            let blocks = chapter(
                File::open(&path).unwrap(),
                &pages[0].locator,
                pages[0].size,
                pages[0].crc,
            )
            .unwrap();
            assert!(blocks
                .iter()
                .any(|b| matches!(b,EpubBlock::Anchor{id} if id=="part two")));
            assert!(!serde_json::to_string(&blocks).unwrap().contains("unsafe"));
        }
        let (_temp, path) =
            illustrated_fixture("<html><body><p>No table of contents</p></body></html>", 0);
        assert!(navigation(File::open(path).unwrap()).unwrap().is_empty());
    }
}
