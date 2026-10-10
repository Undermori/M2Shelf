# MOBI/AZW3 阅读、资源库与窗口栏扩展验收（2026-10-09）

状态：当前本地工作树正式实现，版本仍为 0.5.11；私有测试产物，未提交、推送、签名或发布。当前分支 `ui/global-visual-refresh`，开始时 HEAD `f7f5c1e44e991aa7c7f0cd1b0f460921a18e15b6`。原工作树存在大量未提交/未跟踪功能，本轮直接增量实现，没有 reset、clean 或切换分支。

## 1. 可恢复快照

任务前已有快照 `.tmp/library-refresh-20261009/20261009-105455/`：`worktree-source.zip` 保存 316 个文件，SHA-256 `bda4a319afdc78e66802b8f2cc0d6666bc2bc94be19f74793b9cd2cc6898f936`；被忽略的项目 Skill 单独保存为 `project-skills.zip`，SHA-256 `05a5b9df74c266df5a2c756cfbec1cfbcfe74b94fe03e328bee6c2bbb9bca436`。`snapshot.json` 记录各文件摘要。它不是 Git 提交。

真实应用首次启动新版前，使用 SQLite 只读连接和 Backup API 创建一致副本 `.tmp/mobi-azw3-20261009/native-before.db`，`quick_check` 通过，SHA-256 `0c1db39ea38144d34af7f2fea561eca3eb90824df75659874dac25f893069e2c`。个人目录、库名、书籍内容和数据库均仅留在忽略的私有输出中。恢复应先正常退出程序，在独立暂存目录核对快照，再按文件选择恢复；不能直接覆盖当前整个工作树或活动 WAL 数据库。

## 2. 解析方案、真实能力与许可

采用 [libmobi 官方源码](https://github.com/bfabiszewski/libmobi/tree/906274205c11944b628da1c553b255acb1af7c55)和[官方项目说明](https://www.fabiszewski.net/libmobi/)。固定提交：`906274205c11944b628da1c553b255acb1af7c55`。许可 LGPL-3.0-or-later。

解析器作为独立、可替换 `M2ShelfMobi.exe` 分发，应用不静态链接 libmobi；只通过有界二进制协议交互。每个 Portable/安装器包含对应完整源码 `M2ShelfMobi-source.zip`、LGPL/GPL3 全文、notice、整合代码和独立 MSVC 重建脚本。已从分发源码 ZIP 独立重建 worker，并用 MOBI6、KF8、CP1252、HuffDic 和加密样本检查；四个正常样本成功，加密样本以专用状态拒绝。项目没有引入 Calibre、Kindle 安装要求或在线转换。

真实支持：MOBI6、KF8/AZW3、UTF-8/CP1252、未压缩/PalmDOC/HuffDic、NCX 目录、章节、内嵌栅格图片/封面、基本标题/段落和粗体/斜体 runs。NCX 映射回当前重建内容的本地锚点，按原始源码顺序切章；缺少 NCX 时按 pagebreak 和标题回退。章节 locator 包含原书摘要和原始位置，字号/窗口变化不改变位置身份。排除 `<title>` 元数据，避免图像书前面出现空标题屏。章节适配改变时按相同原始 locator 保留进度与书签。

错误分为 DRM、未知 Kindle 变体、压缩、编码、损坏、预算/超时和组件缺失，均有四语言提示。加密功能未编译，不尝试绕过 DRM；字典、Print Replica、未知新格式明确拒绝。支持安全内容，不声称完整还原任意 Kindle CSS、字体、交互或复杂版式。

## 3. 安全读取与调用关系

`open_resource_file` → `comic_reader::open_resource` → Root/索引/打开句柄校验 → `ebooks::index_file` → `kindle_books` → 受限 worker → `ComicBook`/稳定 chapter locators → 原 `ComicReaderPage` / `DocumentReaderPage` → 安全文字与本地图片块。

`open_comic_book` 和附件打开使用阻塞工作线程，不把解压解析压在异步命令线程上。worker 只接收已验证的只读文件句柄，加入 Windows Job 后才释放 stdin 启动门；限制单进程、384 MiB、20 秒和 96 MiB 输出。源文件 128 MiB、文字 32 MiB、单章 4 MiB，上限检查同时覆盖 PalmDB 偏移、混合文件头、章节/目录、图片格式/维度和 safe HTML 遍历深度。一个进程仅缓存一册，按已打开文件身份失效。

书内脚本、外链图片、任意 CSS、字体、iframe 不执行；不解包到媒体目录、不调用外部安装程序、不把文件路径拼进 shell。PDF、TXT、CBZ 和图片仍走各自既有有界读取/修订/CRC/句柄边界，没有退化为整个大文件 IPC。

## 4. 统一阅读列表与旧索引兼容

所有 Root 的 CBZ/PDF/EPUB/TXT/MOBI/AZW3 文件进入共享阅读表。新增 `src/lib/readingFiles.ts` 在现有 ComicBook 与 ResourceFile 之间按 Windows 来源路径去重，详情顶部统计、阅读表、资源库页直接文件数量共用该集合；旧 MOBI 附件无需重扫、无需首次打开即可显示为可阅读内容。不会伪造 ComicBook ID：未登记文件的阅读与定位按钮使用原 ResourceFile 回调，登记后复用真实书籍 ID 和进度。

`ComicDetailPage`、`WorkDetailPage`、`BrowsePage` 复用 `ComicBookList`。真正剩余的普通 ZIP、未知文件与其他目录继续在其他资源中。后端详情仍只查 SQLite 快照，不遍历媒体盘；打开的附件书籍可随同一快照显示进度，不能在两张表重复出现。

迁移 `0023_readable_resources.sql` 增加 `reader_format`、`source_resource_id`、`source_resource_stamp`，不破坏旧 document_format CHECK/册 ID/外键。旧 ResourceFile 秒级时间戳与打开文件纳秒级修订分开核验；解析后最终短写事务再次确认当前来源，拒绝迟到、删除或变更后的结果。

跨类型附件打开不改变其 Node 类别、原 ResourceFile、核心项目计数或 Bangumi 候选范围。书籍库后续完整重扫把旧附件正式识别为核心书籍时，在清理原资源之前保留原册 ID/进度/书签；单文件识别模式还将同册转移到真正的文件 Node。两种模式有专门回归。书籍增量配置版本为 5，旧版本基线不能阻止新格式重识别。返回仍用原导航快照及滚动恢复机制。

## 5. ARTBOOK 与独立自动关联策略

真实 API 枚举 `ARTBOOK`；UI、Root DTO、扫描/搜索/筛选/详情/阅读/标签/收藏/最近打开均识别。SQLite 通过 `0024_artbook_matching_policy.sql` 的独立不可变 artbook_library 子类型保存，兼容旧 COMIC 存储家族，不用 UI 假别名代替分类。新增库六类；全部资源七项 Tabs（含全部）；旧混合 VIDEO 兼容。

新 COMIC/EBOOK/DOUJIN/ARTBOOK 默认 `autoBangumi=false`，对话框中“不自动关联 Bangumi（推荐）”位于原两种不可变识别方式之前；设置页可独立编辑策略，不切换类型或识别方式，不通过 AppSettings 快照覆盖。旧 COMIC/EBOOK/视频保持启用，旧 DOUJIN 保持关闭，旧绑定/手工 metadata 不清理。

本轮明确替代历史“同人本完全禁止 Bangumi”的规则：四类书籍均可手动 type 1 绑定；DOUJIN/ARTBOOK 只有主动 opt-in 才自动匹配，额外要求精确主标题/别名证据，取消首候选强制提分，并保留原年/卷/版/类型硬冲突。查询、缓存、评分、绑定写入均保持 Root scope。自动关联关闭不触发新搜索/绑定，已有绑定封面恢复仍按原保护边界进行。

本地封面复用既有 application cache：图片目录自然顺序首个有效页；EPUB/MOBI/AZW3 内嵌封面；PDF 首页面直接嵌入 JPEG 可提取。当前没有通用矢量 PDF 首页面栅格化，没有安全可用封面时正常占位。手工和已缓存封面优先，未更改海报显示/持久缩略图路线。

## 6. 局部 UI 调整

窗口栏直接从“文件”开始，已按用户追加要求删除前置小 Logo；侧栏品牌、Windows 应用图标及原 canonical artwork 不变。文件/显示/转至/帮助只复用真实动作：添加库、打开当前库、网格/列表、主题、原导航目的地、关于、更新检查、项目链接。没有假调试菜单或第二层标题栏。下拉支持键盘、Esc、外部点击；阅读期间禁用会绕过退出 flush 的浏览动作。右侧主题/语言继续共享 SettingsStore 序列 writer、失败回滚及系统主题监听。

Settings 仅删除 `.poster-cache-progress` 底部的重复线，保留下一分区边界。文字阅读器沿用全部原字号/字体/字重/对齐/间距/宽度/边距/颜色/滤镜、分页或滚动、键盘、全屏、进度、书签；新增同一底部控制区的章节目录选择。

## 7. 验证证据和边界

- `npm run typecheck`、`npm run build`：通过。
- `npm test -- --run`：123 项通过，18 文件。
- `npm run validate`：135 项通过。
- Rust fmt、严格 all-targets Clippy：通过；完整 Rust tests：287 项通过，12 项专用/外网默认忽略。
- 显式真实解析：6 个未加密 MOBI6/KF8/编码/压缩/NCX 样本通过，另 2 个 DRM 样本拒绝；读取前后源 SHA-256 一致。13 本所有者中文 MOBI 图片书只读验证全部通过，每章真实图片可读，文本元数据块为 0。
- 显式原生集成：六类 Root × MOBI/AZW3，扫描、读取、源修订、进度、书签、封面、重扫保护通过。
- 浏览器真实 React 组件 + 模拟 IPC：332 检查通过，覆盖四语言、浅/深主题、900×640 / 1280×800 / 1920×1080 及 MOBI/AZW3 滚动/分页、目录、25px 字号、书签、返回。Kindle 内容 IR 由真实原生解析器导出公版样本，非伪造正文。
- Windows：最终 Portable 原生 WebView 的旧 MOBI 作品直接显示 13 项可阅读内容、13 条阅读记录，无重复其他资源区；首章显示真实图片，翻页、添加后移除书签、返回与进度显示正常。窗口栏无前置 Logo，文件菜单、Esc、双击最大化、还原与最小化后激活正常。正常窗口 1448×992，最大化 2560×1392。真实用户数据库增量升级由应用执行，只读对照确认原库、Node、绑定、标签、收藏、册 ID 与原进度全部保留，未通过脚本直接修改真实库。
- 标准 Windows 构建、主程序/两 helper 的私有路径扫描、NSIS 构建及 Portable 解包/逐项摘要/源码构建配方一致性检查均记录在私有日志。NSIS 构建成功不等于已安装验收，本轮没有安装 NSIS。

私有证据目录 `.tmp/mobi-azw3-20261009/`；详见 `frontend-tests.log`、`rust-tests-final.log`、`clippy.log`、`validate-final.log`、`kindle-test-final.log`、`kindle-real-final.log`、`chinese-real-final.log`、`browser.log`、`source-rebuild-checks.json`、`delivery-checks.json`、`native-data-checks.json`。当前外部 Bangumi 实时候选和真实 AZW3 Windows 窗口未另建用户库验收；不把 Rust/浏览器夹具称为此项实机接受。

## 8. 截图

均在 `docs/ui-audit/screenshots/reader-extension-20261009/`，保持私有，不能自动提交公开历史。

| 文件 | 来源与状态 |
| --- | --- |
| reference-before-01_Settings_Duplicate_Divider.png | 用户任务包中的修改前参考，非本轮运行截图 |
| reference-before-02_Titlebar_Current.png | 用户任务包中的原窗口栏参考 |
| native-before-titlebar.jpg | 本轮中间测试包的实际原生窗口，前置 Logo 尚在 |
| native-main-final.jpg / native-mobi-detail-final.jpg | 最终程序实际全部资源 / 旧 MOBI 索引阅读列表与统计 |
| native-mobi-image.jpg / native-menu-final.jpg | 最终程序实际中文 MOBI 图片 / 窗口菜单 |
| native-settings-dark.jpg | 原生设置及已有资源库策略 |
| native-cache-divider-final.jpg / native-maximized-final.jpg | 最终原生缓存区域 / 最大化窗口 |
| settings-light.png / settings-dark.png | 浏览器实际设置组件，两主题 |
| artbook-dialog-light-900.png / artbook-dialog-dark-900.png | 浏览器添加设定集对话框，最小桌面宽度 |
| reader-MOBI-PAGED.png / reader-MOBI-SCROLL.png | 浏览器现有阅读器 + 原生公版 MOBI IR |
| reader-AZW3-PAGED.png / reader-AZW3-SCROLL.png | 浏览器现有阅读器 + 原生公版 AZW3 IR |

浏览器组合尺寸与来源在 `checks.json`。原生窗口测量记录在 `native-checks.json`。原生截图只在成功取得目标应用画面时保存，不用浏览器夹具替代。

## 9. 产物、保护项与限制

最新私有 Portable：`bundle/local-test-20261009-124745/M2Shelf-Portable-0.5.11-x64.zip`，SHA-256 `43623DDFD19FE312713CF35AEECC261395BBA1CF1583DB6E6089532C849C5F7C`；同目录提供 NSIS 安装器。ZIP 精确包含主程序、可信 updater、marker、README、SHA256SUMS、独立 MOBI worker、完整对应源码 ZIP、THIRD-PARTY-NOTICES，共八文件。摘要清单覆盖其余七个文件，校验解包字节与构建来源一致。

正式 `bundle/M2Shelf-Portable-0.5.11-x64.zip`、正式 sidecar、公钥、canonical icon-source 摘要保持本轮开始值；Skill 原文未变。更新器依然严格拒绝缺 worker/源码/许可或额外文件的载荷，签名、大小、SHA-256、事务、helper ready、回滚、主程序最后替换保护不弱化。

旧精确五文件 updater 无法安装新八文件载荷。未来公开版本必须按正式流程安排升级路径，本轮不签名、改版本、发 Release 或声称它是公开更新。DRM/字典/Print Replica/未知 Kindle 变体、任意原书 CSS/字体和通用矢量 PDF 封面栅格化不在本轮支持内。文字进度仍定位章节/稳定段，不持久化受字号影响的段内屏号。媒体源只读，没有复制完整用户书籍或个人路径进公共项目文档。
