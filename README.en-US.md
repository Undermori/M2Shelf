# M²Shelf

**MORI MEDIA SHELF**

[简体中文](./README.md) · [日本語](./README.ja-JP.md) · [한국어](./README.ko-KR.md)

M²Shelf is a Windows app for local anime, movies/TV, comics, ebooks, doujinshi and artbooks. Add a folder on a local drive, removable drive or mapped NAS, then browse, search, play and read your collection.

## Features

- Multiple libraries with recursive and incremental scans. Recognize folders or individual files; smart mixed recognition also organizes books into series, standalone books and categories.
- Poster and list views, cross-library browsing, media/tag filters, sorting, and search across local names, filenames and multilingual Bangumi titles.
- Automatic or manual Bangumi matching within each library's media type. TMDb movie matching handles bilingual titles, release years and common release filenames, with original-language posters. Correct bindings, display names and covers manually.
- Custom tags, named favorites, batch editing, and hide/restore actions.
- Launch an external video player; browse attachments and reveal files in Windows Explorer.
- Built-in comic and text readers with progress, bookmarks and recently opened items.
- Persistent local covers and background thumbnail preparation, with progress and retry controls in Settings.
- Light, dark and system themes; Chinese, English, Japanese and Korean; remembered window size, sorting and browsing position.

## Reading

Image folders, CBZ, PDF, EPUB, TXT, MOBI and AZW3. Supported images: JPG, JPEG, PNG, WebP, GIF, AVIF and BMP.

- Comics: single/double pages, horizontal paging, continuous scrolling, Webtoon, reading direction, zoom, fit, background colors and fullscreen.
- Text: paged/scrolling reading, contents, font family/size/weight, line and paragraph spacing, content width, margins and colors.
- Reading progress and bookmarks across formats; books and successfully launched videos share Recently opened.

MOBI/AZW3 supports unencrypted MOBI6/KF8. DRM, ordinary ZIP, RAR and 7Z are not supported as built-in books.

## Download

**M²Shelf 0.5.13 · Windows x64**

- [Installer](https://github.com/Undermori/M2Shelf/releases/download/v0.5.13/M2Shelf-Setup-0.5.13-x64.exe)
- [Portable ZIP](https://github.com/Undermori/M2Shelf/releases/download/v0.5.13/M2Shelf-Portable-0.5.13-x64.zip)
- [Latest release](https://github.com/Undermori/M2Shelf/releases/latest) · [All releases](https://github.com/Undermori/M2Shelf/releases)

Extract the complete Portable ZIP before running `M2Shelf.exe`. The database, settings and default cache use Windows application data. Replace the complete payload when upgrading an older five-file Portable build.

Download 0.5.13 manually; in-app updates are not available for this release.

Microsoft Edge WebView2 Runtime is required. SmartScreen may warn about an unknown publisher.

## Local data

Source media stays read-only. The app stores its index, display names, bindings, tags, favorites, history and settings in its own SQLite database, and covers in its cache.

Bangumi/TMDb queries and cover downloads need a connection. Local browsing and reading do not. Media files are not uploaded, and no media server or cloud account is required. Video playback uses an external player; online streaming, transcoding, device sync and video resume are not provided.

## Development and documentation

Tauri 2 · Rust · React 19 · TypeScript · Vite · SQLite

```powershell
npm ci
npm run tauri dev
```

Run `npm run check` and the Rust gates in [AGENTS.md](./AGENTS.md). Build Windows releases with `scripts/build_windows_release.ps1`.

- [Runtime guide (Chinese)](./docs/RUNTIME_GUIDE_zh-CN.md)
- [Product specification](./docs/PRODUCT_SPEC.md)
- [Current implementation](./docs/PROJECT_CONTEXT.md)
- [Decisions](./docs/DECISIONS.md)
- [Project overview](./PROJECT_DOCUMENTATION.md)
- [Windows development](./docs/WINDOWS_DEVELOPMENT.md)
- [Release process](./docs/UPDATE_RELEASE_PROCESS.md)

## Author

[森下Undermori · Bilibili](https://space.bilibili.com/2903441)
