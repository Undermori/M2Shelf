# SMART_MIXED Phase 2：当前索引适配审计

调查日期：2026-10-09。本轮以本地工作树为准，新增独立只读实验；不是正式智能模式上线。执行结果见 `02_shadow_results.md`，接口见 `02_shadow_protocol.md`。

## 1. 基线与证据等级

- 现场 Git 元数据实际位于 `.git-codex-local`，分支 `ui/global-visual-refresh`，HEAD `f7f5c1e44e991aa7c7f0cd1b0f460921a18e15b6`。HEAD 不能代表大量未提交开发内容。
- 改动前将 467 个原有 tracked、非忽略 untracked，以及项目内被忽略的 Skill 文件纳入私有快照。快照和 SHA 见结果报告。Phase 1 的源码、fixtures、已有报告也属于本轮保护基线。
- **代码确认**：下文的 schema、查询、类型、扫描与阅读边界来自当前源码。
- **实际执行确认**：合成数据库执行全部 24 份真实 SQL migration，运行适配器、识别器、SQL 对照、故障测试和性能测试。
- **没有实际确认**：未读取真实个人数据库，未枚举或解码用户媒体，未运行正式 WebView 验收。本轮 synthetic 的 PDF/EPUB 等是索引行，不是可以用于声称真实解码成功的假书文件。

## 2. 当前产品能力，不能照抄旧交接结论

`src-tauri/src/models.rs:151` 的 `LibraryRecognitionMode` 只有 `FOLDER` / `VIDEO_FILE`；没有正式 `SMART_MIXED`。`models.rs:158` 的 `LibraryMediaKind` 已有 VIDEO、ANIMATION、LIVE_ACTION、COMIC、EBOOK、DOUJIN、ARTBOOK。

当前四种书籍类别已共用图片目录、CBZ、PDF、EPUB、TXT、MOBI、AZW3 的索引/阅读基础。MOBI/AZW3 适配与附件阅读已存在于 `src-tauri/src/kindle_books.rs`、`comic_reader.rs` 和迁移 0023。ARTBOOK 与独立自动匹配开关已存在于迁移 0024。这些是**现有本地实现**，本轮没有新增、修复或验收其解码器。

`docs/DECISIONS.md:5` 的 D54 明确替代旧版 MOBI 暂不支持、同人本完全禁止绑定等结论。`M2SHELF_FULL_PROJECT_HANDOFF.md` 的早期能力说明需要按 D54 与实际代码解释，不能作为当前不支持新格式的依据。本轮不改写旧历史文档。

数据库的 Root 存储与 API enum 并非一一同名：

| 产品类别 | 当前 Root 存储字段 | Shadow 映射 |
|---|---|---|
| COMIC | `media_kind='COMIC'`，普通 comic 子类型 | COMIC |
| EBOOK | `media_kind='COMIC'`，`book_library_kind='EBOOK'` | EBOOK |
| DOUJIN | 书籍 Root，`doujin_library=1` | DOUJIN |
| ARTBOOK | 书籍 Root，`artbook_library=1` | ARTBOOK |
| 动画/真人/旧视频 | `media_kind='VIDEO'` 与 `video_subject_scope` | 适配器直接拒绝，不查询书籍表 |

新建书籍库默认 `auto_bangumi=false`；旧库按现有迁移兼容策略保留。此开关独立于不可变类别、识别模式。本轮只读取 policy 作为快照摘要，不搜索、不匹配、不绑定 Bangumi。

## 3. schema 的真实结构

工厂在 `tools/smart-mixed-shadow/src/fixture.rs:7` 通过 `include_str!` 使用全部 24 份当前 SQL migration；没有新增生产 migration，没有将伪 enum 插入正式表。工厂在新建私有文件内执行这些已有 migration，不调用生产启动时的数据重分类 hook。

| 生产来源 | 与本轮有关的结构 | 适配方式 |
|---|---|---|
| `migrations/0001_initial.sql` | Root、Node、父子 FK、绝对路径、手动类型 | 保留 Node ID/父 ID/Root 归属；派生相对路径 |
| `0002_mvp.sql`、`0008_bangumi_subject_type.sql` | metadata bindings、scan runs、Subject type | 读取既有绑定边界与最近运行状态 |
| `0003_resources_and_cover_status.sql` | ResourceFile | 保留资源身份、路径、大小、时间戳、扩展名与类型 |
| `0009_library_recognition_mode.sql`、`0020_book_file_recognition.sql` | 两种旧 recognition mode 与书籍文件模式兼容 | 原值只读，不重解释或更新 |
| `0011_incremental_scan.sql`、`0014_scan_health.sql` | 增量基线、逐库扫描健康 | 读取健康/scan run；不把目录 digest 当全局索引 revision |
| `0016_comics.sql` | comic_books/pages/progress/bookmarks | 保留 book/page ID、revision、原始页序、来源 locator |
| `0018_document_books.sql` | document_format | 与 reader_format、旧 TXT 回退共同解析 |
| `0019_ebook_library.sql`、`0021_doujin_and_text_books.sql` | EBOOK/DOUJIN、text_encoding | 映射真实子类型，支持旧 TXT 元数据 |
| `0023_readable_resources.sql` | reader_format、source_resource_id/stamp | 将附件阅读状态与核心书籍严格区分 |
| `0024_artbook_matching_policy.sql` | ARTBOOK、auto_bangumi | 映射设定集与独立 policy |

`comic_books.source_kind` 受历史 CHECK 限制，不能仅按字符串把它当最终阅读格式。例如文档书可沿用 `ZIP_ARCHIVE` 兼容存储，单图片附件可沿用 `IMAGE_FOLDER`。正确格式需要结合 `reader_format`、`document_format`、`text_encoding`、Resource FK、源后缀与索引状态。

## 4. 本轮实际读取的表和字段

入口：`tools/smart-mixed-shadow/src/adapter.rs:186`，`ReadIndex::open` / `read`。

| 表 | 读取字段/用途 |
|---|---|
| `mediashelf_schema_migrations` | 版本集合严格为 1–24；缺失、洞或未来 schema 拒绝 |
| `library_roots` | id 对应的 path、media_kind、book_library_kind、doujin_library、artbook_library、recognition_mode、auto_bangumi |
| `nodes` | id、parent_node_id、absolute_path、node_type、manual_type_override，限定 library_root_id |
| `metadata_bindings` | Subject ID/type；初始 Node 注释读取 BANGUMI，旧 ownership SQL 使用原生产 binding 条件 |
| `comic_books` | id、node_id、source_resource_id、source_path、revision、source_kind、reader_format、document_format、text_encoding、file_size、modified_at、source_resource_stamp、page_count、index_error |
| `comic_pages` | comic_book_id、id、page_index、page_name、source_locator、file_size、crc32、modified_at；按原 page_index/id 查询 |
| `resource_files` | id、node_id、absolute_path、file_size、modified_at、extension、resource_type；SQL 派生是否核心书/图片页重复 |
| `library_scan_health` | outcome、error_count、last_success_at、last_auto_attempt_at |
| `scan_runs` | 最近 status、started_at、finished_at、errors |

适配器不读取用户设置、书签、进度、收藏、标签、观看历史、封面内容来推断归组。测试工厂会植入这些状态，并对**整个合成 DB 的逻辑内容与主库字节**做前后比较，证明 Shadow 不改动它们；这不等于真实个人状态已被查询。

## 5. 生产调用链与 Shadow 的隔离

当前生产链：

```text
scanner::run_scan_inner (scanner.rs:476)
  → Root 校验、取消和健康生命周期
  → comics::scan_library (comics.rs:503)
  → 目录/书籍索引
  → store_book_rows (comics.rs:876)
  → refresh_counts (comics.rs:983)

commands::browse_library (commands.rs:628)
  → hidden_root_node_id (db.rs:503)
  → read_snapshot → children / 直属 books / resources

commands::get_node_detail (commands.rs:667)
  → works::node_detail (works.rs:454)
  → Database::read_snapshot (db.rs:73)
  → comics::populate_detail (comics.rs:1110)
  → owned_nodes / books_for_nodes / remaining children / resources
  → 前端 readingFiles (src/lib/readingFiles.ts:8)
```

新增独立链：

```text
显式指定 SQLite 文件与 Root ID
  → SQLite READ_ONLY + query_only 的单个事务
  → IndexSnapshot
  → unchanged Phase 1 recognize()
  → compare() + physical fallback + 脱敏 summary
```

独立 crate 有自己的 `[workspace]` 和 Cargo.lock，生产 crate 不依赖它，也没有新 Tauri command、后台任务、设置入口或模式选项。它不调用生产 `Database::connect` / `migrate`，避免生产初始化的写入、WAL 设置与迁移副作用。

## 6. 身份、版本与格式证据

1. `ComicBook.id` 是真实阅读源身份。Shadow 同时保留 Node ID、ResourceFile FK、原始 source_path、revision 和 source_resource_stamp；不会用推案组 ID 替换它。
2. `page_index` 与原 locator 原样保留，不重新自然排序、不压紧缺口。有效核心文件书要求非空 revision、无 index_error、页数与行数一致、原索引从 0 连续；否则保留来源并诊断。
3. 文档格式镜像当前 `comics.rs:1174`：`COALESCE(reader_format, CASE WHEN text_encoding IS NOT NULL THEN 'TXT' ELSE document_format END)`。另有 CBZ 的既有 ZIP_ARCHIVE + `.cbz` 组合。普通 ZIP 永不提升为书。
4. Shadow 的 `INDEXED_FORMAT_EVIDENCE` 仅表示**当前索引记录已有一致的格式证据**，不是本轮重新打开书籍或确认磁盘文件仍然可读。
5. 开过的 ResourceFile 仍有原资源身份：其 book 行只是附件阅读状态，不成为独立 Work；未打开的支持格式只按当前 UI 规则进入待打开阅读集合，不凭后缀伪造 verified source。

### 图片证据缺口

当前图片目录扫描保存页序、locator、大小等索引信息，实际格式/尺寸验证在读取边界进行。`comic_pages` 没有持久 binary-validation attestation；“扩展名 PNG”不能证明 binary 真的是 PNG，也可能是现有受支持的 JPEG 内容。

因此本轮不扫描或解码图片来补证据，所有核心 IMAGE_FOLDER 保留真实 book ID、原始 page_index 和 locator，标记 `INDEXED_IMAGE_BINARY_VALIDATION_UNKNOWN`，进入 physical fallback；含此类后代的系列标记 REVIEW（S02）。这会使合成混合系列出现“6 个已提案 + 1 个物理回退”，而不是照搬 Phase 1 手工 verified 输入中的 7 个提案。

单图片附件是兼容特例：`source_kind='IMAGE_FOLDER'` 但 source_path 是文件。适配器先判断 Resource FK，按文件保存并验证 locator 与 source_path 对应；不会伪造图片文件夹。生产参考 `comic_reader.rs:261`。

## 7. 当前 ownership、阅读表与附件去重

`comics::owned_nodes`（`comics.rs:1054`）以详情锚点递归：

- 排除 IGNORED 后代；
- 不穿透任何手动分类后代；
- 无 binding 的后代可归入；有 binding 的后代只有与**锚点** Subject ID/type 相同才归入。

`populate_detail` 根据实际返回 books 将其祖先标记 expanded；remaining children 保留未展开的导航目标。`db::list_resources_for_nodes_conn`（`db.rs:3380`）排除同 Node 核心来源和核心图片页。`readingFiles.ts` 又按规范化路径去掉已列书籍，再将六种文档扩展名划入阅读表，其余进入 Other。

Shadow 的旧视图镜像这套集合，并用**直接抽取当前生产 owned_nodes SQL 与 browse predicate**在合成 SQLite 上执行对照。不是仅比较页面文案。三个集合分开记录：旧 book IDs、待打开 readable Resource IDs、真正 other Resource IDs。

Root Browse 与递归 Detail 不同：Root 首页是直属目录 + 直属 books；Root 详情投影可以递归取得 owned books。两个系列 + 100 PDF 的首页原来就是 2 个目录与 100 个直属书籍，不能声称旧首页只有 2 项，或把 104 册递归总数当首页卡片数。

Shadow 新归组比旧详情更保守：任意已有绑定（包括同 Subject）都形成 no_merge fence。旧详情同绑定穿透结果仍如实展示在对照里，不能把两种行为说成完全等价。人工/绑定边界不能被模拟 override 撤销。

旧完整 detail 集合仅计算隐藏 Root 与最多 64 个首页 browse 锚点，以限制重叠递归成本。全部来源与物理树仍映射；未比较的 detail 标记为未比较，不能按 0 差异处理。

## 8. 一致性、健康、路径与失效

当前生产 `Database::read_snapshot` 已提供 SQLite 读快照，但没有统一、单调、覆盖所有书籍与覆盖操作的 Root index revision。`incremental.rs` 的配置/目录摘要是扫描基线，不能冒充该 revision。

Shadow 在一次 deferred transaction 内先 SELECT 固定 SQLite 快照，再读取所有参与表。并发 WAL 写入测试证明：读中提交新索引时，本次仍看到旧一致视图；下一次内容 SHA 改变，传入旧 expected_version 会拒绝。`index_snapshot_version` 是所读 Root 内容 SHA-256，非全库事务号、非当前媒体磁盘真实性证明。

健康为 SUCCESS、error_count=0，且最近 run 没有未完成/失败/取消，才标 complete。其他情况保留全部索引来源，提案降为 REVIEW；没有任何删除/解绑建议。即使 complete，freshness 仍为 `INDEX_ONLY_CURRENT_DISK_UNKNOWN`，因为本轮不接触当前磁盘。

路径来自已索引 Root/Node/source，不接受 UI 拼接路径。`relative_index_path`（`adapter.rs:98`）检查 Windows 磁盘/UNC/verbatim 的**词法**范围、相对路径组件与 Root containment，保留 Unicode 和原始 locator。它不 canonicalize、不验证当前重解析点或 opened handle；正式 reader/scanner 的句柄安全边界没有被此实验替代。

## 9. 指令与 Skill 状态

已审查当前 AGENTS、`.agents/skills/m2shelf-ui/SKILL.md` 及其 product constraints，项目产品/上下文/决策、旧交接和 Phase 1 文档/源码/fixtures。本轮为只读数据适配，不是 UI 工作；UI Skill 的 Sidebar、颜色、海报和布局偏好未进入识别规则，不需要在此轮进行视觉探索。

本轮没有修改 AGENTS、Skill、现有产品上下文、正式源码、正式 migrations 或 UI。下一阶段如正式改变产品行为，再按仓库规则同步 durable context；当前只新增本目录报告，防止把实验当成已上线产品。

## 10. 后续接入必须先解决的缺口

- 图片验证证据怎样由现有安全读取/索引过程提供，而不靠后缀或新 Shadow 磁盘枚举。
- 真正的 Root 级索引/覆盖 revision 与写入前 CAS；本轮摘要只能检测所读投影变化。
- 持久 group/member/override 以及虚拟组与原 Node metadata 的关系；现在都不存在。
- 同绑定 descendant 的旧详情兼容与 Shadow 保守 fence，需要明确上线策略。
- 100k 完整流程约 514 MiB 峰值；取消只在纯 recognize 前后检查，尚不支持识别器内部中断。
- 与生产查询副本长期保持同步：已有 SQL parity 测试，但未来 UI/API 接入仍需使用共享受测读模型。
- 全部用户数据/native reader/GUI 验收尚未执行，需要另外授权的后续阶段。
