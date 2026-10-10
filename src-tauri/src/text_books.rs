//! Plain-text books. Stable byte segments keep bookmarks independent of typography.
use crate::{
    comics::{IndexedPage, MAX_PAGES},
    db::AppResult,
    ebooks::EpubBlock,
};
use encoding_rs::{Encoding, GB18030, UTF_16BE, UTF_16LE, UTF_8};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
};

pub const MAX_TEXT_BYTES: u64 = 32 * 1024 * 1024;
const SEGMENT_BYTES: usize = 8192;
fn codec(name: &str) -> AppResult<&'static Encoding> {
    match name {
        "UTF-8" => Ok(UTF_8),
        "UTF-16LE" => Ok(UTF_16LE),
        "UTF-16BE" => Ok(UTF_16BE),
        "GB18030" => Ok(GB18030),
        _ => Err("COMIC_DOCUMENT_INVALID".into()),
    }
}
fn decode(bytes: &[u8], name: &str) -> AppResult<String> {
    codec(name)?
        .decode_without_bom_handling_and_without_replacement(bytes)
        .map(|value| value.into_owned())
        .ok_or_else(|| "COMIC_DOCUMENT_INVALID".into())
}
#[cfg(test)]
pub fn index(path: &std::path::Path) -> AppResult<Vec<IndexedPage>> {
    let file = File::open(path).map_err(|_| "COMIC_READ_FAILED")?;
    index_file(file)
}
pub fn index_file(file: File) -> AppResult<Vec<IndexedPage>> {
    if file.metadata().map_err(|_| "COMIC_READ_FAILED")?.len() > MAX_TEXT_BYTES {
        return Err("COMIC_PAGE_LIMIT".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_TEXT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "COMIC_READ_FAILED")?;
    if bytes.len() as u64 > MAX_TEXT_BYTES {
        return Err("COMIC_PAGE_LIMIT".into());
    }
    let (name, bom) = if bytes.starts_with(&[0xff, 0xfe]) {
        ("UTF-16LE", 2)
    } else if bytes.starts_with(&[0xfe, 0xff]) {
        ("UTF-16BE", 2)
    } else if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        ("UTF-8", 3)
    } else if std::str::from_utf8(&bytes).is_ok() {
        ("UTF-8", 0)
    } else {
        ("GB18030", 0)
    };
    let text = decode(&bytes[bom..], name)?;
    if text.trim().is_empty() {
        return Err("COMIC_NO_PAGES".into());
    }
    if text.contains('\0') {
        return Err("COMIC_DOCUMENT_INVALID".into());
    }
    let mut pages = Vec::new();
    let mut start = bom;
    while start < bytes.len() {
        let mut end = (start + SEGMENT_BYTES).min(bytes.len());
        // At most four bytes need backing off to preserve an encoded character.
        while end > start && decode(&bytes[start..end], name).is_err() {
            end -= 1;
        }
        if end == start || pages.len() >= MAX_PAGES {
            return Err("COMIC_PAGE_LIMIT".into());
        }
        let digest = format!("{:x}", Sha256::digest(&bytes[start..end]));
        pages.push(IndexedPage {
            name: format!("{}.txt", pages.len() + 1),
            locator: format!("{name}:{start}:{}:{digest}", end - start),
            size: (end - start) as u64,
            modified: String::new(),
            crc: None,
        });
        start = end;
    }
    Ok(pages)
}
pub fn read(file: &mut File, locator: &str, size: u64) -> AppResult<Vec<EpubBlock>> {
    let parts = locator.split(':').collect::<Vec<_>>();
    if parts.len() != 4 || size > MAX_TEXT_BYTES {
        return Err("COMIC_PAGE_LIMIT".into());
    }
    let begin = parts[1].parse::<u64>().map_err(|_| "COMIC_PAGE_CHANGED")?;
    let length = parts[2].parse::<u64>().map_err(|_| "COMIC_PAGE_CHANGED")?;
    if length == 0
        || length > SEGMENT_BYTES as u64
        || begin.checked_add(length).is_none_or(|end| end > size)
    {
        return Err("COMIC_PAGE_LIMIT".into());
    }
    file.seek(SeekFrom::Start(begin))
        .map_err(|_| "COMIC_READ_FAILED")?;
    let mut bytes = vec![0; length as usize];
    file.read_exact(&mut bytes)
        .map_err(|_| "COMIC_PAGE_CHANGED")?;
    if format!("{:x}", Sha256::digest(&bytes)) != parts[3] {
        return Err("COMIC_PAGE_CHANGED".into());
    }
    Ok(decode(&bytes, parts[0])?
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .split('\n')
        .map(|line| EpubBlock::Text {
            text: line.to_owned(),
            tag: "p".into(),
            runs: Vec::new(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encodings_segments_and_hashes_are_bounded_and_lossless() {
        let temp = tempfile::tempdir().unwrap();
        let text = "中文小说与标点，<script>只是文本</script>。\n".repeat(1000);
        for (name, bytes) in [
            ("UTF-8", text.as_bytes().to_vec()),
            ("GB18030", GB18030.encode(&text).0.into_owned()),
            (
                "UTF-16LE",
                [
                    vec![0xff, 0xfe],
                    text.encode_utf16().flat_map(u16::to_le_bytes).collect(),
                ]
                .concat(),
            ),
            (
                "UTF-16BE",
                [
                    vec![0xfe, 0xff],
                    text.encode_utf16().flat_map(u16::to_be_bytes).collect(),
                ]
                .concat(),
            ),
        ] {
            let path = temp.path().join(format!("{name}.txt"));
            std::fs::write(&path, &bytes).unwrap();
            let pages = index(&path).unwrap();
            assert!(pages.len() > 1);
            let mut restored = String::new();
            for page in pages {
                assert!(page.locator.starts_with(name));
                let blocks = read(
                    &mut File::open(&path).unwrap(),
                    &page.locator,
                    bytes.len() as u64,
                )
                .unwrap();
                let lines = blocks
                    .into_iter()
                    .map(|b| match b {
                        EpubBlock::Text { text, .. } => text,
                        _ => panic!(),
                    })
                    .collect::<Vec<_>>();
                restored.push_str(&lines.join("\n"));
                let bad = page
                    .locator
                    .replace(page.locator.rsplit(':').next().unwrap(), "bad");
                assert!(read(&mut File::open(&path).unwrap(), &bad, bytes.len() as u64).is_err());
            }
            assert_eq!(restored, text);
        }
    }
}
