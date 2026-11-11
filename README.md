# M²Shelf

**MORI MEDIA SHELF**

[English](./README.en-US.md) · [日本語](./README.ja-JP.md) · [한국어](./README.ko-KR.md)

M²Shelf 是 Windows 本地媒体收藏管理工具，支持动画、电影/剧集、漫画、电子书、同人本和设定集。添加硬盘、移动硬盘或 NAS 映射目录，扫描后即可用海报墙浏览、搜索、播放和阅读。

## 功能

- **资源库**：管理多个目录，递归扫描、增量更新；支持按文件夹或单个文件识别，书籍另有智能混合识别，可整理系列、独立书籍和分类目录。支持拖入文件夹添加和更新已搬迁的资源库路径。
- **浏览与搜索**：海报墙和列表、跨库浏览、媒体类型与标签筛选、多种排序。全局搜索支持名称、文件名、Bangumi 多语言标题和笔记，并可组合筛选资源库、媒体类型、绑定、观看／阅读状态和文件格式。
- **元数据**：按资源类型自动匹配或手动绑定 Bangumi；真人电影和剧集支持 TMDb，可使用默认服务或个人令牌；识别双语片名、年份、季集和常见压制命名，使用原始语言海报。可手动纠正绑定、修改显示名称和封面。
- **整理**：自定义标签、命名收藏夹、观看／阅读状态和本地笔记。支持批量编辑、隐藏与恢复条目、控制资源库是否出现在「全部资源」，以及在「待整理」查看匹配和封面问题。
- **播放**：使用系统默认或自定义播放器。动画详情分开显示正片、SPs 和其他视频，可按集记录进度。配置 mpv 后可同步切集与播放位置。可查看附属文件并在资源管理器中定位。
- **阅读**：内置漫画与文字阅读器，保存阅读进度和书签，支持从列表继续读下一卷。「最近打开」统一显示最近 30 项，可移除记录。
- **封面**：本地缓存和后台缩略图，可在设置中查看进度、失败原因并重试。PDF 缺少封面时可用首页，也可从本机图片、可用的在线候选或书页更换封面。
- **界面**：浅色、深色、跟随系统；简体中文、English、日本語、한국어；记忆窗口大小、排序及浏览位置。

## 内置阅读

支持图片文件夹、CBZ、PDF、EPUB、TXT、MOBI 和 AZW3。图片支持 JPG、JPEG、PNG、WebP、GIF、AVIF、BMP。

- 漫画：单页/双页、横向翻页、连续滚动、Webtoon、左右阅读方向、缩放、适应窗口、背景颜色和全屏。
- 文字书籍：翻页/滚动、目录、字体和字号、字重、行距、段距、正文宽度、边距和阅读配色。
- 漫画和 PDF 支持页面缩略图，文字书籍和有文本层的 PDF 支持正文查找。

MOBI/AZW3 支持未加密的 MOBI6/KF8，不支持 DRM；普通 ZIP、RAR、7Z 不作为内置书籍打开。

## 下载

**M²Shelf 0.5.17 · Windows x64**

- [安装包](https://github.com/Undermori/M2Shelf/releases/download/v0.5.17/M2Shelf-Setup-0.5.17-x64.exe)
- [Portable 压缩包](https://github.com/Undermori/M2Shelf/releases/download/v0.5.17/M2Shelf-Portable-0.5.17-x64.zip)
- [最新 Release](https://github.com/Undermori/M2Shelf/releases/latest) · [全部版本](https://github.com/Undermori/M2Shelf/releases)

Portable 请完整解压后运行 `M2Shelf.exe`，不要直接在压缩包内运行。数据库、设置和默认缓存保存在 Windows 应用数据目录。旧五文件 Portable 请完整解压新版升级。

可在设置中检查更新，也可手动下载安装包或 Portable 压缩包。

运行需要 Microsoft Edge WebView2 Runtime。SmartScreen 可能提示未知发布者。

## 本地数据

媒体目录只读，不移动、删除、重命名或改写源文件。索引、显示名称、绑定、标签、收藏夹、历史记录和设置保存在应用自己的 SQLite 数据库中，封面保存在应用缓存中。可在设置中备份和恢复收藏数据及手动封面。

Bangumi/TMDb 查询及下载封面需要联网，本地浏览和阅读不依赖在线服务。M²Shelf 不上传媒体文件，不需要媒体服务器或云端账号。视频使用外部播放器，目前不提供在线视频、转码或跨设备同步。

## 开发

Tauri 2 · Rust · React 19 · TypeScript · Vite · SQLite

```powershell
npm ci
npm run tauri dev
```

验证：`npm run check`，以及 `AGENTS.md` 中的 Rust 格式、测试与 Clippy 检查。Windows 发布使用 `scripts/build_windows_release.ps1`。

## 文档

- [Windows 开发与构建](./docs/WINDOWS_DEVELOPMENT.md)
- [更新与发布流程](./docs/UPDATE_RELEASE_PROCESS.md)

## 作者

[森下Undermori · Bilibili](https://space.bilibili.com/2903441)
