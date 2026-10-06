<p align="center">
  <img src="./src-tauri/icons/128x128.png" width="96" height="96" alt="M²Shelf Logo">
</p>

<h1 align="center">M²Shelf</h1>

<p align="center"><strong>MORI MEDIA SHELF</strong></p>

<p align="center"><a href="./README.en-US.md">English</a> · <a href="./README.ja-JP.md">日本語</a> · <a href="./README.ko-KR.md">한국어</a></p>

<p align="center">完全由 GPT 完成，面向 Windows 本地媒体收藏的本地优先、媒体源只读浏览器。</p>

M²Shelf 把本地硬盘、移动硬盘或 NAS 映射目录中的动画、电影及相关资源建立为独立索引，提供海报墙浏览、Bangumi 元数据、标签、收藏夹、观看记录和外部播放器入口。

想了解软件怎样扫描、分类、匹配动画和同步信息，可以阅读面向使用者的[运行机制说明](docs/RUNTIME_GUIDE_zh-CN.md)，其中包含目录示例和常见问题的处理方法。

简单来说，可以让你在资源管理器中由于语言、编码、字幕组等差异以及文件名过长而难以区分的动漫合集，一键转换为易于阅读的海报视图，如下所示：

<img width="1445" height="1226" alt="ff3acd6cec8b9a3b76d2e1d4b7737b29" src="https://github.com/user-attachments/assets/4e1a235c-ea75-4996-b7a7-5c890e0b0803" />

在动画“作品”的详情页也依旧可以看到文件名显示，并可一键在资源管理器中打开：

<img width="1445" height="1226" alt="8fcf460277f5bca26062e347984387fd" src="https://github.com/user-attachments/assets/9bac84e4-af4e-4b2a-8c5f-ecd05b06b34b" />

动画“系列”的详情页展示：

<img width="1445" height="1226" alt="08a0af8ef5a719090efdfa33cd88f944" src="https://github.com/user-attachments/assets/9bc04a4c-4184-4dd3-82dc-094ab32049f6" />

**软件不会移动、删除、重命名或修改源媒体文件，也不要求整理现有目录。**

## 主要功能

- 管理多个媒体资源库并递归扫描任意深度目录；
- 使用海报墙或列表浏览全部资源、单个资源库和真实目录层级；
- 搜索本地名称、文件名、Bangumi 多语言标题和用户标签；
- 自动识别作品、系列和其他资源，并保留人工分类；
- 高置信度自动匹配 Bangumi，亦可手动搜索、纠正或重试封面；
- 展示视频，以及字幕、图片、音频、文档、压缩包等附属资源；
- 使用已配置的外部播放器播放，并在 Windows 资源管理器中定位文件；
- 使用标签、一层命名收藏夹和编辑模式批量整理；
- 按最近时间展示由 M²Shelf 成功启动播放的作品；
- 支持简体中文、English、日本語、한국어；
- 支持跟随系统、亮色和暗色主题；
- 记忆窗口尺寸、排序选择和各浏览分区的会话内位置；
- 支持自定义应用封面缓存位置。

## 本地优先与隐私

媒体目录始终视为只读。M²Shelf 的索引、显示名称、Bangumi 绑定、标签、收藏夹、观看记录和设置保存在应用自己的 SQLite 数据库中，封面保存在应用缓存中。

Bangumi 搜索和封面下载需要联网；本地索引浏览与打开本地文件不依赖 Bangumi。项目不需要媒体服务器或云端账号，也不会把媒体文件上传到远程服务。

## 下载

当前版本：**M²Shelf 0.5.11**（Windows x64）

- [下载 Portable 免安装版](https://github.com/Undermori/M2Shelf/releases/download/v0.5.11/M2Shelf-Portable-0.5.11-x64.zip)
- [查看最新 Release](https://github.com/Undermori/M2Shelf/releases/latest)
- [查看全部版本](https://github.com/Undermori/M2Shelf/releases)

Portable 使用方法：

1. 完整解压 ZIP，不要在压缩包内直接运行；
2. 双击 `M2Shelf.exe`；
3. 添加媒体目录并扫描；
4. 按需设置外部播放器路径。

Portable 表示应用本体无需安装。数据库、设置和默认封面缓存仍会写入 Windows 应用数据目录。当前构建未进行代码签名，Windows SmartScreen 可能提示“未知发布者”；运行界面依赖 Microsoft Edge WebView2 Runtime。

## 当前边界

M²Shelf 当前不提供内置播放器、在线视频、转码、媒体服务器、账号同步、自动字幕、续播进度，也不会自动移动或重命名媒体文件。

## 开发


技术栈：Tauri 2、Rust、React 19、TypeScript、Vite 和 SQLite。

```powershell
npm install
npm run tauri dev
```

提交前验证：

```powershell
npm run typecheck
npm run build
npm run validate
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml --locked
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings
```

面向 Windows 的正式产物通过 `scripts/build_windows_release.ps1` 构建。

## 项目文档

- [开发规则](./AGENTS.md)
- [产品规格](./docs/PRODUCT_SPEC.md)
- [当前实现](./docs/PROJECT_CONTEXT.md)
- [长期决策](./docs/DECISIONS.md)
- [快速项目说明](./PROJECT_DOCUMENTATION.md)

## 作者

- [森下Undermori · Bilibili](https://space.bilibili.com/2903441)

## 作品库与启动更新

默认作品库跨目录平铺具有视频的作品，并按相同 Bangumi 条目聚合来源；系列负责目录组织，可切回文件夹浏览。FOLDER 模式下，明确的 CD1/CD2、Disc1/Disc2 等分卷按结构和标题证据归属外层作品；冲突绑定和人工分类保持独立。作品详情默认在“作品中的其他资源”中递归平铺所属子目录的视频，保留真实路径和播放、定位入口。

聚合作品的 Bangumi 绑定、改绑、清除和自动封面重试作用于整组来源；分类、改名、隐藏、标签和收藏需选择明确来源。设置中的“已隐藏条目”支持搜索与恢复。启动自动更新的新库默认开启，旧库缺少保存设置时默认关闭，已有选择保留；首次开启可能需要完整枚举目录。扫描结果和最后成功时间可在资源库管理中查看，失败不清理未读到的索引，也不推进该库基线。历史别称补全每次应用运行最多尝试 32 个不同条目，失败消耗预算，下次启动轮换续跑。源媒体始终只读。

前端与项目检查使用 `npm run check`，Rust 格式、测试和严格 Clippy 由独立 Windows PR workflow 验证。开发配置见 [Windows 本地开发](docs/WINDOWS_DEVELOPMENT.md)。
