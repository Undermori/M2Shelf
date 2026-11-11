# M²Shelf

**MORI MEDIA SHELF**

[简体中文](./README.md) · [日本語](./README.ja-JP.md) · [한국어](./README.ko-KR.md)

M²Shelf is a Windows app for local anime, movies/TV, comics, ebooks, doujinshi and artbooks. Add a folder on a local drive, removable drive or mapped NAS, then browse, search, play and read your collection.

## Features

- Multiple libraries with recursive and incremental scans. Recognize folders or individual files; smart mixed recognition also organizes books into series, standalone books and categories. Drop folders to add libraries and update a library path after moving it.
- Poster and list views, cross-library browsing, media and tag filters, and sorting. Search names, filenames, multilingual Bangumi titles and notes, with filters for library, media type, binding, watch/read status and file format.
- Automatic or manual Bangumi matching within each library's media type. TMDb supports movies and TV series with the default service or a personal token. It handles bilingual titles, years, season/episode information and common release filenames, with original-language posters. Correct bindings, display names and covers manually.
- Custom tags, named favorites, watch/read status, local notes and batch editing. Hide or restore individual items, and choose which libraries appear in All Resources. Review matching and cover issues on the organization page.
- Open videos in the system default or a custom player. Anime details separate main episodes, specials and other videos. Track episodes and sync episode changes and playback position with a configured mpv. Browse attachments and reveal files in Windows Explorer.
- Built-in comic and text readers with progress, bookmarks and next-volume reading. Recently opened holds up to 30 entries and supports removing them.
- Persistent local covers and background thumbnails, with progress and retry controls in Settings. PDFs can use the first page as a fallback. Choose covers from local files, available online candidates or book pages.
- Light, dark and system themes; Chinese, English, Japanese and Korean; remembered window size, sorting and browsing position.

## Reading

Image folders, CBZ, PDF, EPUB, TXT, MOBI and AZW3. Supported images: JPG, JPEG, PNG, WebP, GIF, AVIF and BMP.

- Comics: single/double pages, horizontal paging, continuous scrolling, Webtoon, reading direction, zoom, fit, background colors and fullscreen.
- Text: paged/scrolling reading, contents, font family/size/weight, line and paragraph spacing, content width, margins and colors.
- Page thumbnails for comics and PDFs. In-book search for text books and PDFs with a text layer.

MOBI/AZW3 supports unencrypted MOBI6/KF8. DRM, ordinary ZIP, RAR and 7Z are not supported as built-in books.

## Download

**M²Shelf 0.5.17 · Windows x64**

- [Installer](https://github.com/Undermori/M2Shelf/releases/download/v0.5.17/M2Shelf-Setup-0.5.17-x64.exe)
- [Portable ZIP](https://github.com/Undermori/M2Shelf/releases/download/v0.5.17/M2Shelf-Portable-0.5.17-x64.zip)
- [Latest release](https://github.com/Undermori/M2Shelf/releases/latest) · [All releases](https://github.com/Undermori/M2Shelf/releases)

Extract the complete Portable ZIP before running `M2Shelf.exe`. The database, settings and default cache use Windows application data. Replace the complete payload when upgrading an older five-file Portable build.

Check for updates in Settings, or download the installer or Portable ZIP manually.

Microsoft Edge WebView2 Runtime is required. SmartScreen may warn about an unknown publisher.

## Local data

Source media stays read-only. The app stores its index, display names, bindings, tags, favorites, history and settings in its own SQLite database, and covers in its cache. Back up and restore collection data and manually chosen covers from Settings.

Bangumi/TMDb queries and cover downloads need a connection. Local browsing and reading do not. Media files are not uploaded, and no media server or cloud account is required. Video playback uses an external player. Online streaming, transcoding and cross-device sync are not provided.

## Development and documentation

Tauri 2 · Rust · React 19 · TypeScript · Vite · SQLite

```powershell
npm ci
npm run tauri dev
```

Run `npm run check` and the Rust gates in [AGENTS.md](./AGENTS.md). Build Windows releases with `scripts/build_windows_release.ps1`.

- [Windows development and builds](./docs/WINDOWS_DEVELOPMENT.md)
- [Release process](./docs/UPDATE_RELEASE_PROCESS.md)

## Author

[森下Undermori · Bilibili](https://space.bilibili.com/2903441)
