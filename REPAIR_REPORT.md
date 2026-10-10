# M²Shelf 主返工与 TMDb v2 合并交付记录

最新书籍集合及窗口栏追加修复见 [ALL_RESOURCES_BOOK_FIX_20261010.md](ALL_RESOURCES_BOOK_FIX_20261010.md)，候选为 `bundle/local-rc-all-books-20261010/`。

后续五项 UI 定向修复及最新 0.5.12 候选见 [UI_TARGETED_FIXES_20261010.md](UI_TARGETED_FIXES_20261010.md)。本报告保留前一轮的范围、截图和旧候选摘要。

记录日期：2026-10-10。当前版本 **0.5.12**，schema **26**。代码、合成回归、截图及 Windows 本地 RC 已完成；**原生 Tauri Windows WebView 未实机验证**，真实提供方联网和私人书籍也未验收。以下按证据边界记录，不能据此宣布完整现场验收。

## 1. 执行范围和安全快照

本轮合并主返工与随后补充的 TMDb/识别选项 v2，未重启开发流程。v2 覆盖旧包关于不改 TMDb 弹窗的冲突部分；保留原生 TMDb、凭证、绑定和 SMART_MIXED 后台。没有进入全局视觉重设计、增加提供商、电视剧搜索或新阅读器。

- 本地分支 `ui/global-visual-refresh`，HEAD `f7f5c1e44e991aa7c7f0cd1b0f460921a18e15b6`，任务结束仍相同。使用现有工作树，不以远端替代。
- 修改前快照：`.tmp/repair-poster-reader/20261009-232510/worktree-before.zip`，500 个源文件，包含既有未提交修改及项目 Skill；SHA-256 `ce069ef2966ddd717907fa28ae0b795037c8b7588bc01e3fef0c70ff4aa68311`。同目录保存原清单、工作区/暂存 diff 和状态。
- 核对 37 个保护文件与快照字节相同：AGENTS、项目 Skill/reference、已有 1–25 迁移、TMDb 凭证实现、版本/锁文件、canonical 图标母版、公钥等。`tmdb.rs` 在测试区以前的生产实现逐字相同。证据：`docs/ui-repair/20261010/evidence/preservation-verification.json`。
- 未提交、推送、切分支、签名、发布、安装、替换当前用户程序；未读取、迁移、扫描或写入私人数据库/媒体，也未读写真实凭证和生产私钥。未删除旧 Phase 3 候选或旧 `DELIVERY_REPORT.md`。
- **数据库备份未执行**：本轮仅操作合成 SQLite，工作树快照不包含用户 AppData。Portable 与正式程序共享 AppData，不能安全充当隔离测试环境；首次运行候选前需由所有者正常关闭旧程序，备份完整应用数据和一致的 SQLite/WAL/SHM 状态。没有把“未触碰数据库”描述为“已完成数据库备份”。

## 2. 实际修改与根因

### 2.1 智能书籍复用标准海报/详情

旧 `SmartMixedBrowser` 自己绘制目录、切换入口和文件详情；逻辑组没有 `coverNode` 时直接占位，即使四本 EPUB 已有合法内嵌封面也会丢失。此问题属于 UI 数据契约，没有证据要求重写 SMART_MIXED 识别器。

现在 `BrowsePage` 保留原标题、数量、路径、筛选/排序、Grid/List 和编辑工具栏；`SmartMixedBrowser` 投影后台逻辑组，复用 `PosterGrid` / `MediaCard`。真实独立书通过 `BookPosterCard` 复用相同卡片形态、固定封面框和元信息行；独立单本直接进入已有 reader，系列复用 `ComicDetailPage` / `ComicBookList`，无锚点系列也使用相同 hero/table 样式。没有第二套常态“书籍目录”主页、两侧物理切换按钮或大块纠错栏。

分类目录来自同一次 SQLite catalogue snapshot 的真实 `directoryNodes`；没有为封面/标签/Explorer 捏造 Node 或磁盘路径。无锚点卡的 cover key 使用 `book-{id}`，避免与 Node 数字 ID 相撞。右键中保留人工分类、系列、归入系列、独立、恢复自动；事务 revision 和真实 book FK 保持。逻辑根数量仅是会话展示值，不重写物理索引计数。

**元数据编辑边界**：存在真实独立 Node 锚点时沿用标签/收藏/绑定等既有操作；多个根层文件共享一个父 Node 时，不能把父 Node 当成每本独立作品分别写入。此类卡支持阅读、原路径和归属纠错，不伪造独立 Node 编辑能力；现有原目录的 Node 级操作仍在。该边界保留原有身份保护，未新增书级标签/收藏 schema。

真实书封面调用链：

```text
BookPosterCard / BookPosterCover
 → useCoverDataUrl（共享四请求门禁、LRU、预热和修订保护）
 → get_book_cover_data_url(real book ID, revision, selected tier)
 → comic_reader::cover_identity / 已校验源读取
 → poster_cache::thumbnail_data_url（既有磁盘衍生物）
 → 单个 PosterImage
```

继续使用 256/384/512/768 档、高质量 Rust 重采样、不放大小原图、原始图不改写、校验和及原子文件、原有缓存边界。缓存失败安全回退已验证原图。未增加 Canvas 替换、滚动重绘或第二个后台生成器。**无锚点书的缺失衍生图当前首次按需生成，之后复用磁盘结果；不能声称所有书扫描后均已预生成、首次打开完全零 I/O。** 无合法本地封面的 PDF 仍正常占位。

### 2.2 顶栏方向状态正确但 sticky 被父布局截断

原 `.content-scroll` 为 flex，主页面子项被限制在一屏高度，而内容继续溢出。方向 hook 确实改变隐藏状态，但 sticky 的父范围已滚过，导致中部上滚也看不见栏。不是把 8px 门槛再调小即可解决。

改为可随内容增长的块级滚动布局，保留真正内部滚动容器；详情工具栏使用同一 sticky/transform 思路。保留原 8px 上滚/24px 下滚滞回、占位、焦点可见和返回快照。Chrome 中在 500/1500 分别验证 down100 → up30 后真实 DOM 几何位置回到视口，再 down60 隐藏；资源库与系列详情分别检查。**这是 Chrome DOM 证据，不是原生 WebView 滚轮/触摸板验收。**

### 2.3 文字翻页、位置与语义目录

旧实现仅保留章节/段号，以瞬时屏号驱动翻页；章节切换、字体或窗口重排时无法保持正文锚点，部分翻页还会读取过期 React 状态。底栏把目录、数字输入、书签、状态堆在一起；EPUB 直接显示资源文件名。另发现后加载 `comics.css` 会把新三列底栏覆盖回 flex，已用明确的 document-reader 选择器修正。

现在使用“原章节/稳定原字节段 + 安全块索引 + UTF-16 字符偏移”，通过 DOM Range 捕获/恢复可见正文，实际 scrollLeft 决定下一列；不会把字号相关屏号存成永久阅读位置。字号/行距/宽度/模式、窗口/全屏变化、上一章末尾/下一章开头、书签、退出 flush 共用这一位置。旧记录没有块位置时仍回到原章节/段。迟到的章节结果受 generation 保护；本地插图继续使用有界懒加载和解码缓存。

底栏固定 56px 三列：上一页在左、短章节/段状态居中、下一页/目录/书签在右；目录展开不改变底栏高度。输入跳转和书签列表移入目录面板。鼠标按钮无持久焦点描边，Tab 焦点可见，Space/Enter 不重复激活；PDF 保留真实页面底栏，漫画页读取逻辑不变。

EPUB 在安全、句柄/修订验证下读取 nav/NCX，将语义标题和片段映射原 spine，不重排章节。目录解析仍受 XML 4 MiB、30,000 节点、128 深度和条目上限保护，拒绝外链/不安全路径；无有效目录时使用本地化章名。该定位是当前安全 IR 内的锚点，**不是完整 EPUB CFI、Kindle 原书 CSS 或所有出版物排版保证**。内容位置做法参考成熟阅读器的章节内位置与重排思路：[Calibre 阅读器文档](https://manual.calibre-ebook.com/en/viewer.html)、[KOReader 用户指南](https://koreader.rocks/user_guide/)、[Foliate FAQ](https://github.com/johnfactotum/foliate/blob/gtk4/docs/faq.md)，未引入它们的新运行时。

### 2.4 TMDb/Bangumi 共享匹配 v2

`BangumiModal` 持有唯一 provider、keyword、results、generation 状态；`MatchingResults` 渲染共享结果，`TmdbMatch` 是严格区分 Bangumi Subject 与 TMDb Movie 的适配层。标题随提供方改变，统一海报、主标题、原名、日期和直接“选择”。移除 TMDb 年份输入、已配置常驻条、必须先开详情才能绑定的分叉；无凭证/认证失败才显示紧凑配置入口。

TMDb 调用仍使用真实 Node ID、movie ID、locale 和原生 snapshot；搜索年份参数传 null，没有改后台年份能力。Bangumi 保留真实 Subject 写入。切换提供方、修改关键词/语言、关闭、卸载会使旧请求失效，不能把迟到结果带进另一提供方或 Node。仅 LIVE_ACTION 与兼容 VIDEO 中合规 WORK/AUTO_WORK 且非动画绑定的单来源作品显示入口；FOLDER/VIDEO_FILE 均有检查。动画、书籍、Container 不扩权。

原生 TMDb 网络/缓存/凭证/绑定/封面保护实现和 Credential Manager 未改。已有双提供方 ID 隔离、手工封面优先、取消/过期快照和解除回退继续由测试覆盖。没有清令牌、清绑定表、删除旧迁移或要求重新填写密钥；About 署名保留。**本轮未使用真实凭证查询 TMDb/Bangumi，故在线成功、认证和系统代理热切换仍未实测。** Bangumi type 2/6 范围按原生代码及测试确认，未把 Superbad 无结果推断为 API 故障。

### 2.5 新建识别和主题勾选框

四类书籍库初始 selection=null，选择智能/文件夹/单本后才启用“创建资源库”；点击卡片不直接创建，换类别清空选择。智能卡和非点击说明在上，下方传统两卡等宽；hover/selected/focus 使用中性灰，无暖色下划线。不自动关联策略继续默认勾选，且与识别模式独立。动画/真人原二选一即选即建保持。

普通复选框统一 checked/unchecked/hover/disabled/active/keyboard 样式，实心 Logo 红与白色勾保证辨认；不影响 switch。状态保存仍走原 native Root policy 回调，浏览器合成检查包含回读、保存失败回滚和 pending disabled，原生数据库策略测试保留。

## 3. 主要代码入口

| 范围 | 文件 / 符号 | 变化 |
|---|---|---|
| 标准库页面 | `src/pages/BrowsePage.tsx:63`、`src/App.tsx:197` | shared toolbar、logical presentation、独立展示计数 |
| 智能投影 | `src/components/SmartMixedBrowser.tsx:26` | 真实目录/组映射到旧网格与详情，导航/纠错 |
| 真实书海报 | `src/components/BookPosterCard.tsx:11`、`src/hooks/useCoverDataUrl.ts:124` | 与 Node 共用图片管道，书 ID 命名空间 |
| 原生封面入口 | `src-tauri/src/commands.rs:1659`、`src-tauri/src/poster_cache.rs:435`、`src-tauri/src/comic_reader.rs:153` | 复用 source/revision 校验及持久衍生物 |
| 合并匹配 | `src/components/BangumiModal.tsx:20`、`MatchingResults.tsx:5`、`TmdbMatch.tsx:6` | shared provider 状态及 typed result/direct bind |
| 识别选择 | `src/components/LibraryRecognitionModeDialog.tsx:15` | 四书类无预选/明确创建；视频旧流程 |
| 正文锚点 | `src/lib/textPosition.ts:4`、`src/pages/DocumentReaderPage.tsx:17` | capture/restore、翻页/书签/重排 |
| 安全语义目录 | `src-tauri/src/ebooks.rs:127`、`src/components/EpubContent.tsx:14` | nav/NCX、片段、原 spine |
| 位置持久化 | `src-tauri/migrations/0026_text_reader_positions.sql`、`comic_reader.rs:641`、`comic_reader.rs:692` | additive progress/bookmark 字段与原安全检查 |
| 样式根因修复 | `src/styles/workspace.css:690`、`:750`、`:770`、`:788` | scroll containing block、checkbox、识别卡、固定底栏 |
| Shadow schema | `tools/smart-mixed-shadow/src/adapter.rs`、`fixture.rs`、`tests/adapter_shadow.rs` | production 接受 25/26，默认 24，future-schema fail closed 测试随之更新 |

完整相对快照修改清单见 preservation JSON；当前 durable docs 同步了产品、上下文及 D56，旧历史记录保留且注明被替代范围。AGENTS 和 Skill 未改。

## 4. 数据库兼容和原安全边界

- 0026 仅给 progress/bookmarks 增加可空 block 与默认 0 的 offset，范围分别 0..100000 / 0..4194304；旧 chapter/segment/page 索引、PK/FK 不改。没有把库降为 24、删除 0025 或重建来源。
- 合成当前 schema 工厂构造 25 数据，升级至 26 并再次 migrate，验证幂等、foreign_key_check，以及 Bangumi/TMDb、设置、附件 FK、书 ID、进度/书签保留。
- 4 个真实自建 EPUB 原生测试：无 coverNode 的四逻辑作品、合法内嵌封面、持久缩略图跨 Database 重开复用、旧 revision 拒绝、进度/书签写入/重扫保存、源字节不变。不是用户四本原书。
- Kindle reindex 继续按原 locator 保留位置；ResourceFile 身份和原 stamp 不被 UI 推广为新 Work。
- 页面/原文/封面读取保留 Root、canonical/opened handle、revision、格式/维度/容量、归档路径/CRC 和 IPC 限制。没有为需求放宽媒体权限、解密 DRM、启用书内脚本/网络或写回源文件。
- 新 schema 的降级不受支持；回退需匹配旧程序与升级前完整数据库备份。

## 5. 验证结果（本轮执行）

所有命令和原日志保留于 `.tmp/repair-poster-reader/`。需要 Windows 本地子进程的命令经系统沙盒批准后运行；没有自动审核拒绝或绕过权限。

| 检查 | 结果 | 日志 |
|---|---|---|
| `npm run typecheck` | 通过 | `frontend-build.log` |
| `npm run build` | 通过 | `frontend-build.log` |
| 前端完整测试 | 134 通过 / 21 文件 | `frontend-tests.log` |
| `npm run validate` | 135 通过 | `validate.log` |
| `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` | 通过 | `fmt.log` |
| `cargo test --manifest-path src-tauri/Cargo.toml --locked` | 308 通过、13 默认忽略、0 失败 | `native-tests.log` |
| `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings` | 通过 | `clippy.log` |
| Phase 1 lab | 35 invariant/unit + 49 golden | `lab-tests.log` |
| Shadow 默认 | 1 unit + 41 integration | `shadow-tests.log` |
| Shadow production feature | 1 unit + 41 integration | `shadow-production-tests.log` |
| React 浏览器功能 | 107 检查 | `evidence/ui-results.json` |
| 窗口/主题/语言/旧组件矩阵 | 306 检查 | `evidence/matrix-results.json` |
| 补充提供方/checkbox/分类/列表 | 49 检查 | `evidence/extra-results.json` |
| 历史截图及安全插图导航补充 | 4 检查 | `evidence/final-screens-results.json` |
| 延迟插图保持内容锚点 | 3 检查 | `evidence/late-image-results.json` |
| Windows release / NSIS | 通过 | `windows-build.log` |
| 精确八文件 Portable | 通过 | `portable-build.log`、`evidence/artifact-verification.json` |

五组浏览器合计 **469 检查、0 React runtime error**。运行当前实际组件，旧画面动态导入任务开始快照源码；Tauri IPC 使用合成对象，海报/插图是自建图，TMDb 远程图片被拦截为合成图，不访问真实 provider。窗口 900×640 / 1280×800 / 1920×1080，light/dark、四语言、system 在打开时切换均覆盖；1920×1080 是浏览器大窗口检查，不冒充原生最大化。

阅读测试使用 16 章长文夹具，第 5 章到真实中段后连续前进 20 列、后退 20 列，检查每次真实列宽、scrollLeft 和内容位置；字号 18/28/24、行距 2.2、最大宽度 620、窗口/全屏、翻页/滚动、书签、关闭重开保持旧锚点可见。另测章节 7 迟到不能覆盖 9、跨章节首尾、Space 单次、输入不抢键和安全插图解码后翻页。另外延迟 1.1 秒读取正文前方插图，在读取完成前翻到正文中段，完成后原内容锚点仍可见（`after-reader-late-image.png`）。复杂出版物所有迟到图片重排组合仍非完全覆盖。

库夹具包含四类书库、四独立 EPUB、分类下 2 本、根层系列/分类/98 独立作品共 100 张卡，系列 80 行、筛选空态/List/历史返回及最小窗口。真实 native 测试覆盖 cover/progress/rescan 身份；未使用私人库重扫，不能把合成数量当作私人库验收。

Vite 有非阻断性大 chunk 提示；Windows 链接器有静态库工具提示，构建退出 0，Clippy 无警告失败。13 个忽略测试保留原忽略设置；不将其算作通过。

## 6. 前后截图与复现

统一目录：`docs/ui-repair/20261010/screenshots/`。以下均为浏览器夹具，主题/语言/窗口由文件名或表格给出；画面只有合成内容，不含私人书库。

| 问题 | 修改前 1280×800 / dark / zh-CN | 修改后 |
|---|---|---|
| 四独立 EPUB 灰占位/另起目录 | `before-four-epubs.png` | `after-four-epubs.png` |
| 智能目录单独视觉 | `before-smart-library.png` | `after-library-COMIC.png`、`after-smart-category.png`、`after-smart-list.png` |
| 识别默认/传统卡片 | `before-book-modes.png` | `after-mode-unselected-dark.png`、`after-mode-selected-light.png` |
| 匹配弹窗冗余/并行 UI | `before-bangumi-modal.png`、`before-tmdb-modal.png` | `after-bangumi-results-FOLDER.png`、`after-tmdb-results-FOLDER.png`、`after-tmdb-results-VIDEO_FILE.png` |
| 底栏和原始 XHTML 名 | `before-reader-footer.png` | `after-reader-toc-1280.png`、`after-reader-toc-900.png`、`after-reader-toc-1920.png` |
| 中部 sticky 位置 | 旧用户截图仅作问题依据 | `after-header-up-500.png`、`after-header-up-1500.png`、`after-detail-header-up-1500.png` |
| 勾选/未选/悬停/禁用 | 旧问题截图 | `after-checkbox-checked-*.png`、`after-checkbox-unchecked-*.png`、`after-checkbox-hover-*.png`、`after-checkbox-disabled-*.png`、`after-checkbox-tab-*.png` |
| 其他 reader | — | `after-reader-TXT.png`、`after-reader-MOBI.png`、`after-reader-AZW3.png`、`after-reader-PDF.png`、`after-reader-illustrations.png` |

`matrix-modes-*`、`matrix-reader-*` 展示 24 个窗口/主题/语言组合；`after-tmdb-minimum-*` 是最小窗口 provider 组合。完整实际文件以截图目录为准。

**原生待验收复现步骤**：先关闭旧程序并备份完整应用数据，使用候选打开足够长的标准库和详情；在实际 `.content-scroll` Y≈500、≈1500 执行 down100 → up30，确认同一顶栏立即出现且不需回顶，再 down60 确认隐藏。分别用鼠标滚轮和可用触摸板，截图记录实际窗口大小/主题/版本。不要把 body/window 滚动或状态 class 变化单独当作成功证据。当前工具原生桌面控制不可用；未为截图启动会使用私人 AppData 的默认 EXE。

## 7. Windows 本地 RC

目录：`bundle/local-rc-repair-20261010/`。标准构建入口 `scripts/build_windows_release.ps1 -Bundles nsis`，再 `scripts/build_portable.ps1 -SkipBuild`。以下摘要对应实际文件：

| 文件 | 字节数 | SHA-256 |
|---|---:|---|
| `M2Shelf-Setup-0.5.12-x64.exe` | 9,844,380 | `812c404350399864cd9bac04277e073f2b30aee799e7aa2176091121a8a5679d` |
| `M2Shelf-Portable-0.5.12-x64.zip` | 13,058,657 | `12095720ad2b34cd83123c86db7741d2be0f2c86c102d78327bb2b59b2eb08f2` |

已解包核对精确八文件：M2Shelf.exe / M2ShelfUpdater.exe / M2Shelf.portable.json / README_zh-CN.txt / SHA256SUMS.txt / M2ShelfMobi.exe / M2ShelfMobi-source.zip / THIRD-PARTY-NOTICES.txt。7 项载荷摘要、marker、主程序/helper/worker x64 PE、发布二进制字节、EXE/NSIS 版本、helper appId/version/当前公钥一致，用户目录/工作区路径扫描包括 worker 源码归档通过。只运行 updater 的只读 identity，不运行主 GUI 或安装器。

版本仍 0.5.12，本地新旧候选不通过自动更新区分；未生成 latest.json、签名或正式发布授权。旧五文件 updater 不能安装当前八文件包；该 RC 不声明旧更新器兼容。候选使用说明与完整性摘要随目录提供。

## 8. 尚未确认和验收边界

1. 原生 Tauri WebView 真正中部 sticky 滚轮/触摸板、原生最大化/全屏与热主题仍未实机验证。
2. 真实 TMDb/Bangumi 在线搜索、真实令牌认证/配置、系统代理热切换未操作；原生生产实现未改及合成回归通过不等于联网成功。
3. 用户原问题 EPUB/私人库、完整 AppData 25→26 迁移及备份未执行；合成兼容性通过，首次真实运行仍需升级前一致备份。
4. 未执行 NSIS 安装、当前程序替换、正式签名/发布；没有因此清理或关闭用户现有进程。
5. 内容锚点、原生安全预算、现有缓存仍有明确范围；任意出版物特殊排版、所有图片加载时序以及 1 万本真实磁盘压力不在本轮已确认结论内。

此记录只说明当前修复与可交付候选，不授权后续全局设计、扫描或自动上线。
