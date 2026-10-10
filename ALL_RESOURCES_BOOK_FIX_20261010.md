# M²Shelf 书籍集合与窗口栏追加修复

日期：2026-10-10。版本 **0.5.12**，schema **26**。最新本地 unsigned RC：`bundle/local-rc-all-books-20261010/`。保留主返工、TMDb v2 和五项 UI 定向修复；旧候选与报告保留。

## 根因与结果

1. 语言按钮的地球图标已经在按钮内部正常排列，但 CSS 仍保留旧绝对定位图标的 `padding-left:28px`。仅删除这一条旧规则，左右恢复 8px；中文按钮宽度从 120px 回到 100px。提示、语言保存、菜单和窗口控制不变。
2. SMART_MIXED 原先只接到资源库首页，全部资源仍读取物理顶层 Node，并把持有直属书籍的隐藏 Root 当作品。现在跨库返回已有 Catalogue，全部资源与 SmartMixedBrowser 使用同一个 `bookCollectionEntries` 投影；独立书籍、系列、分类并列，不再出现一张代表整库的作品海报。普通 FOLDER 的直属书籍各自展示，VIDEO_FILE 保持原独立 Node。四类书库和全部资源两种总览均适用，同名跨库书籍不合并。
3. 扩查发现搜索用书名命中父 Node，最近打开按父 Node 折叠。这两处也改为携带和使用原 ComicBook ID。搜索保留文件夹/绑定/标签/视频入口，单本 Node 与本书去重；视频历史仍按 Node，书籍按各本书。标题、封面、源位置与点击阅读都指向本书，时间显示保留。

真正的系列仍为一个系列入口，分类仍可进入下级；没有全局拆散物理文件夹。打开单本进入原 reader，打开系列/分类进入原智能库，返回恢复原集合和筛选。手工归属纠错通知跨库刷新，元数据补丁同步真实锚点。

## 实现与边界

| 文件/符号 | 变更 |
|---|---|
| `src-tauri/src/db.rs` / `Database::list_all_resources` | 同一 SQLite read snapshot 返回物理卡片和书籍目录；移除隐藏 Root 卡片及智能库重复物理卡片；LEGACY 只补充直属核心书籍 |
| `src-tauri/src/smart_mixed.rs` / `catalogue_conn` | 复用已有逻辑关系与来源安全边界，在调用者快照内读取；新增跨类型/模式回归 |
| `src-tauri/src/db.rs` / `search`, `list_recently_watched` | 有界书名命中、共享批量 metadata hydration、精确隐藏 Root 判断；逐书历史 DTO，稳定排序 |
| `src-tauri/src/models.rs`, `src/types/media.ts` | 书库 Catalogue、可选真实 book payload、`COMIC_BOOK` 搜索结果；无持久 schema 改动 |
| `src/lib/bookCollection.ts` | 资源库与全部资源共享逻辑投影，跨库稳定 key；不伪造可编辑 Node |
| `src/pages/AllResourcesPage.tsx`, `src/components/SmartMixedBrowser.tsx` | 共用 MediaCard/BookPosterCard；类型、文本、标签、排序、网格/列表、数量与真实来源操作 |
| `src/App.tsx` | 原导航 history 携带逻辑目的地，原 reader 打开各书；数量及 Catalogue 元数据同步 |
| `src/pages/SearchPage.tsx`, `src/pages/RecentlyWatchedPage.tsx` | 按原书籍身份展示和打开；源封面沿用有界缩略图路径 |
| `src/components/BookContextMenu.tsx`, `BookPosterCard.tsx` | 无独立 Node 的书籍仅提供源位置操作，保留键盘关闭和焦点；最近打开时间及合法独立 Node 封面优先 |
| `src/styles/workspace.css` | 本次追加仅删除旧的语言按钮左占位 |
| `scripts/validate_project.py` | 两条源码形状检查适配 Node/book 双来源和逐书历史；原封面生命周期、四语言、视频时间契约继续检查 |

不需要重扫、重新匹配或重建索引。旧逻辑关系未应用时，已索引书籍仍以独立 fallback 可访问。已忽略子树不重新出现，核心文件不会因本次读取变成附件或复制身份；可读附件保持原 Resource FK。

无独立 Node 的书籍不挪用父节点标签/收藏/绑定，也不参加伪造 Node 的批量编辑。既有 Node、book、Resource、进度、书签、手工封面、绑定和识别方式不变。本轮未修改扫描器、迁移、原生 reader、提供方/凭证、封面缓存管道、更新器、图标、依赖、版本、Skill 或 AGENTS。

## 最终验证

| 检查 | 结果 |
|---|---|
| TypeScript / 生产构建 | 通过 |
| 前端测试 | 23 文件，150 通过 |
| 仓库校验 | 135 通过 |
| Rust fmt / 严格 Clippy | 通过 |
| Rust 测试 | 310 通过，13 原有忽略 |
| Chrome React + 合成 IPC | 323 项检查，0 React 运行错误 |
| 窗口栏间距/菜单 | 24 组：两主题 × 四语言 × 900/1280/1920px；左右 8px，菜单未越界 |
| 标准 Windows x64 构建 | 主程序、NSIS、updater、MOBI worker、八文件 Portable 通过 |
| 载荷验证 | 精确八文件、内部七项 SHA-256、三枚 x64 PE、helper identity/版本/公钥、release 二进制匹配、构建私有路径扫描通过 |

合成 SQLite 矩阵覆盖 COMIC/EBOOK/DOUJIN/ARTBOOK × SMART_MIXED/FOLDER/VIDEO_FILE，PDF/EPUB/TXT/MOBI/AZW3/CBZ，系列、分类、未应用关系回退、忽略子树、搜索 Root 范围、每书历史与源行保留。浏览器覆盖两种全部资源总览、两主题、四语言、三窗口，筛选/排序/列表、系列和分类进入返回、单本阅读、搜索与历史准确 book ID。既有视频、别名、绑定、标签、扫描、reader 与更新器回归一起通过。

首轮扩查测试暴露了隐藏 Root 过滤过宽，随后用精确 Root 路径判断修正，并修复 SQL 作用域；最终全部回归通过。源码形状校验的两条旧断言同步到新的双来源契约，没有删除安全校验。Vite 仍有既有大 chunk 提示，Windows linker 的中文 stdout warning 保留。

源码保护证据：`.tmp/ui-targeted-20261010/caption-spacing/preservation.json`、`task-only.patch`；原私有工作树快照保留，分支 `ui/global-visual-refresh`、HEAD `f7f5c1e44e991aa7c7f0cd1b0f460921a18e15b6` 未变。未提交或推送。

## 截图与本地候选

截图索引：`.tmp/ui-targeted-20261010/caption-spacing/index.html`，共 14 张，排除诊断图。代表图：`before-dark.png` / `after-dark.png`，`books-SMART_MIXED.png` / `books-FOLDER.png` / `books-VIDEO_FILE.png`，`series-detail.png`，`book-reader.png`，`search-books.png`，`recent-books.png`。均为合成数据的真实组件渲染，保存在忽略目录。

| 文件（位于 `bundle/local-rc-all-books-20261010/`） | 字节 | SHA-256 |
|---|---:|---|
| `M2Shelf-Setup-0.5.12-x64.exe` | 9,839,535 | `7359cce799b1dfe27ee1e570d366370d14deedbb46a1b7636f02da25d7b69021` |
| `M2Shelf-Portable-0.5.12-x64.zip` | 13,078,833 | `aa3d03e303c6226631c518c93b142e37d3f4157291d82ebe3b6c133437902c78` |

候选未签名、发布或安装。Portable 精确八文件，解包主程序位于候选目录的 `portable/`。本地摘要不是正式更新授权；没有生成或复用正式签名清单。

## 验收限制

未启动会访问真实 AppData 的默认 Tauri 主程序，未访问私人数据库或媒体。没有把 Chrome 合成 IPC 验证写成原生 Windows WebView、Windows DPI、真实 TMDb/Bangumi 或私人书籍验收。候选保持既有 schema 26，首次从更旧程序运行仍应遵循既有数据备份要求；本轮没有迁移或覆盖用户数据。
