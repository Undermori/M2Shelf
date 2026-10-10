# M²Shelf

**MORI MEDIA SHELF**

[English](./README.en-US.md) · [日本語](./README.ja-JP.md) · [한국어](./README.ko-KR.md)

M²Shelf 是 Windows 本地媒体收藏管理工具，支持动画、电影/剧集、漫画、电子书、同人本和设定集。添加硬盘、移动硬盘或 NAS 映射目录，扫描后即可用海报墙浏览、搜索、播放和阅读。

## 功能

- **资源库**：管理多个目录，递归扫描、增量更新；支持按文件夹或单个文件识别，书籍另有智能混合识别，可整理系列、独立书籍和分类目录。
- **浏览与搜索**：海报墙和列表、跨库浏览、媒体类型与标签筛选、多种排序；搜索本地名称、文件名及 Bangumi 多语言标题。
- **元数据**：按资源类型自动匹配或手动绑定 Bangumi；真人电影支持 TMDb，识别双语片名、发布年份和常见压制命名，使用原始语言海报。可手动纠正绑定、修改显示名称和封面。
- **整理**：自定义标签、命名收藏夹、批量编辑、隐藏与恢复条目。
- **播放**：调用已配置的外部播放器；查看字幕、音频等附属文件，在资源管理器中打开目录或定位文件。
- **阅读**：内置漫画与文字阅读器，保存阅读进度、书签和最近打开记录。
- **封面**：本地缓存和后台缩略图生成，按显示尺寸选用；设置中查看进度、失败原因并重试。
- **界面**：浅色、深色、跟随系统；简体中文、English、日本語、한국어；记忆窗口大小、排序及浏览位置。

## 内置阅读

支持图片文件夹、CBZ、PDF、EPUB、TXT、MOBI 和 AZW3。图片支持 JPG、JPEG、PNG、WebP、GIF、AVIF、BMP。

- 漫画：单页/双页、横向翻页、连续滚动、Webtoon、左右阅读方向、缩放、适应窗口、背景颜色和全屏。
- 文字书籍：翻页/滚动、目录、字体和字号、字重、行距、段距、正文宽度、边距和阅读配色。
- 各类书籍共用阅读进度与书签；视频和书籍统一显示在最近打开中。

MOBI/AZW3 支持未加密的 MOBI6/KF8，不支持 DRM；普通 ZIP、RAR、7Z 不作为内置书籍打开。

## 下载

**M²Shelf 0.5.13 · Windows x64**

- [安装包](https://github.com/Undermori/M2Shelf/releases/download/v0.5.13/M2Shelf-Setup-0.5.13-x64.exe)
- [Portable 压缩包](https://github.com/Undermori/M2Shelf/releases/download/v0.5.13/M2Shelf-Portable-0.5.13-x64.zip)
- [最新 Release](https://github.com/Undermori/M2Shelf/releases/latest) · [全部版本](https://github.com/Undermori/M2Shelf/releases)

Portable 请完整解压后运行 `M2Shelf.exe`，不要直接在压缩包内运行。数据库、设置和默认缓存保存在 Windows 应用数据目录。旧五文件 Portable 请完整解压新版升级。

0.5.13 请手动下载更新，暂不提供应用内更新。

运行需要 Microsoft Edge WebView2 Runtime。SmartScreen 可能提示未知发布者。

## 本地数据

媒体目录只读，不移动、删除、重命名或改写源文件。索引、显示名称、绑定、标签、收藏夹、历史记录和设置保存在应用自己的 SQLite 数据库中，封面保存在应用缓存中。

Bangumi/TMDb 查询及下载封面需要联网，本地浏览和阅读不依赖在线服务。M²Shelf 不上传媒体文件，不需要媒体服务器或云端账号。视频使用外部播放器，目前不提供在线视频、转码、跨设备同步或视频断点续播。

## 开发

Tauri 2 · Rust · React 19 · TypeScript · Vite · SQLite

```powershell
npm ci
npm run tauri dev
```

验证：`npm run check`，以及 `AGENTS.md` 中的 Rust 格式、测试与 Clippy 检查。Windows 发布使用 `scripts/build_windows_release.ps1`。

## 文档

- [运行机制说明](./docs/RUNTIME_GUIDE_zh-CN.md)
- [开发规则](./AGENTS.md)
- [产品规格](./docs/PRODUCT_SPEC.md)
- [当前实现](./docs/PROJECT_CONTEXT.md)
- [长期决策](./docs/DECISIONS.md)
- [快速项目说明](./PROJECT_DOCUMENTATION.md)
- [Windows 本地开发](./docs/WINDOWS_DEVELOPMENT.md)
- [更新与发布流程](./docs/UPDATE_RELEASE_PROCESS.md)

## 作者

[森下Undermori · Bilibili](https://space.bilibili.com/2903441)
