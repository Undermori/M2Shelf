# SMART_MIXED Phase 1：当前架构审计

审计依据：2026-10-09 当前本地工作树，分支 `ui/global-visual-refresh`，HEAD `f7f5c1e44e991aa7c7f0cd1b0f460921a18e15b6`。HEAD 不是当前全部实现：已有大量未提交修改，本轮完整保留。本轮源码证据均为这些修改之后的文件，未用远端替代。

## 结论与证据等级

**由代码确认**：当前已支持 COMIC / EBOOK / DOUJIN / ARTBOOK；MOBI/AZW3 已进入索引、独立解析程序和文字阅读通路；正式识别方式仍只有 FOLDER / VIDEO_FILE。SMART_MIXED 仅存在于本轮新建实验的输入输出标识，正式程序没有接入。

**由本轮运行确认**：独立 Rust 原型可编译执行，结构、性质与 CLI 对照通过，性能数字见测试报告。

**尚未由本轮运行确认**：正式 WebView、真实书籍解码、真实数据库/重扫/媒体库、Bangumi、生产完整 gate。本轮按要求没有启动正式程序、运行生产扫描测试或触碰用户数据库。下面列出的生产测试是审查其代码，不冒充本轮已重跑。

历史 `M2SHELF_FULL_PROJECT_HANDOFF.md` / `M2SHELF_UI_FILE_MAP.md` 可用于定位架构；涉及 MOBI/AZW3、设定集、创建库和标题栏的旧快照不应覆盖当前实现。当前 `docs/PRODUCT_SPEC.md:7`、`:11`、`:13`、`:27` 与 `docs/PROJECT_CONTEXT.md` 的后续增量及 `docs/DECISIONS.md` D54 已记录扩展。稳定规则中仍有旧的 DOUJIN 完全禁用 Bangumi 表述；当前用户已授权的实现是默认关闭、允许手动及主动开启后自动关联，本轮不调整这些规则或代码。未来接入以真实 `auto_bangumi`、类型边界和最新决策为准。

## 现有类型、身份和存储

| 概念 | 当前事实 | 可核查位置 |
|---|---|---|
| 库类型 | Rust DTO 含 7 类（包括兼容 VIDEO 和 ARTBOOK）；书籍家族包含四类 | `src-tauri/src/models.rs:151` / `LibraryRecognitionMode`; `:158` / `LibraryMediaKind`; `is_book` |
| 识别方式 | 仅 Folder、VideoFile；未知 stored 值会 fallback Folder，不能只添加字符串假装接入完成 | `models.rs:151`, `LibraryRecognitionMode::from_db` |
| Root 持久化 | 存储家族 VIDEO/COMIC，book_library_kind、video_subject_scope、doujin_library、artbook_library 再映射 DTO；auto_bangumi 独立 | `db.rs:397` / `add_root_with_policy`; `:423` INSERT；migration 0019/0021/0024 |
| 模式不可变 | 0009 CHECK 只接受 FOLDER/VIDEO_FILE；0020 去除旧 COMIC/FOLDER-only 限制并加不可变 trigger | `src-tauri/migrations/0009_library_recognition_mode.sql:1`, `0020_book_file_recognition.sql:1` |
| 物理 Node | id、Root、parent、真实路径、NodeType、人工类型、名称、绑定/封面等应用元数据 | `models.rs:316` / `MediaNode`; `0001_initial.sql` / nodes |
| 册身份 | ComicBook 的 id/node_id/source_path/revision/source_kind/reader format/progress，不是作品名 | `comics.rs:250` / `ComicBook`; `0016_comics.sql:4` |
| 页与状态 | 页、进度、书签通过 comic_book_id 关联；源路径 NOCASE；unique(node_id,source_path) | `0016_comics.sql:19`, `:31`, `:37` |
| 旧附件书籍 | source_resource_id 唯一关联 ResourceFile，避免虚构书 ID；正式重扫可保留册 ID 后升级为核心书 | `0023_readable_resources.sql:4`; `comics.rs:746`; `comics.rs:2661` 测试 |

因此新逻辑类型不能替换既有 NodeType，也不能把一个逻辑系列键当作 comic_book_id。目录分类、文件是不是书、书属于哪一系列是不同层次。

## 扫描调用关系

```text
scanner::run_scan → run_scan_inner
  → validate_scan_root / canonicalize_within_library_root / cancel checks
  → media_kind.is_book() → comics::scan_library → ComicScan::directory
  → 非书籍：原 scan_video_file_library 或 scan_directory
                       → LogicalWorkIndex::reclassify（VIDEO 存储家族）
```

入口分支为 `scanner.rs:476` / `run_scan_inner`（书籍分支在 :514 附近）。`logical_works.rs:93` / `load_with_classification` 的查询限制 `library_roots.media_kind='VIDEO'`。它解决视频所有权与聚合，不是可以直接拿来给漫画加“智能识别”的通用书籍分类器。

`comics.rs:503` / `scan_library` 根据模式创建隐藏 Root/设置 flat_root；FOLDER 保留物理父子树，VIDEO_FILE 把每个图片集或文件书变成隐藏 Root 下独立 Node。文件读取仍按实际完整路径，扁平展示不等于移动文件。

`comics.rs:569` / `ComicScan::directory`：

1. canonical containment 与访问去重；忽略/未变化目录保留已索引行。
2. 完整读取当前目录项目后再考虑清理。生产代码现在以直属图片候选判断图片书，不包含本轮的 numbered-page/素材语义规则。
3. 已有策略只排除 `__macosx`、`.ds_store`、`thumbs.db`、`desktop.ini`；跳过链接。不按隐藏/临时/回收站名称擅自扩大排除范围。
4. `is_image` (`:292`) 支持 JPG/JPEG/PNG/WebP/GIF/AVIF/BMP；页面排序复用 `db::natural_cmp`，读取内容再验证实际签名。`is_archive` (`:298`) 只有 CBZ。
5. `ebooks::format` (`ebooks.rs:13`) 当前支持 PDF/EPUB/TXT/MOBI/AZW3，`index_file` (`:27`) 对 Kindle 调用 `kindle_books::index_file`，TXT 独立有界处理。
6. `store_book_rows` (`comics.rs:876`) UPSERT 既有来源，更新 revision、页面并维护进度/书签位置。不是每次新建书籍身份。
7. VIDEO_FILE 和目录子树清理均由错误/取消边界控制；失败不把未见条目当删除证据。`refresh_counts` (`:983`) 派生 direct/branch/total 册数及既有自动 Node 分类，人工覆盖优先。

本实验不复制上述扫描器、排序器、格式解码或数据库事务；只接收它们将来可提供的已验证结果。实验的 `verified` 不可由 UI 或后缀直接伪造。

## 增量与错误边界

`incremental.rs:19` 的 Snapshot 记录配置摘要和目录元数据摘要；`Plan` 有 targets、unchanged、failed_roots。它不是书籍语义归属模型。将来可在现有索引完成后派生完整 Root 清单，不另扫全盘。规则/覆盖版本应参与未来逻辑结果失效控制；不能因新规则强制改变旧模式。

失败、取消和掉盘仍由原扫描生命周期、last_seen 与基线保护负责；原型的 `prior_units` 只证明“不提出删除”，没有替代这些事务机制。

## 详情、核心内容和附件

`commands.rs:628` / `browse_library` 和 `:667` / `get_node_detail` → `works.rs:454` / `node_detail` → **同一个 SQLite read_snapshot** 内调用 `comics::populate_detail`。

- `owned_nodes` (`comics.rs:1054`) 递归已有 Node 所有权，遇到 IGNORED、manual_type_override 或不同绑定停止。
- `books_for_nodes` (`:1075`) 每批 ≤500 个 Node，按真实来源自然排序并排除 ZIP。
- `populate_detail` (`:1110`) 用已呈现书籍计算 expanded 祖先；同时返回阅读表、未展开子目录和剩余附件。打开详情不枚举媒体盘。
- `db::list_resources_for_nodes_conn` (`db.rs:3386` SQL) 排除已索引核心文件和已呈现图片页；同一本书不应再次进入附件。
- `books_for_resources` (`comics.rs:1140`) 保留旧附件入口；`works.rs:409` 的 video work_detail 使用视频聚合，不应被本轮书籍规则改变。
- `src/lib/readingFiles.ts:8` 是表现分区：核心 comicBooks + 已支持的旧附件文件 → reading table；剩下 other。`BrowsePage.tsx:71`、`ComicDetailPage.tsx:45` 共同使用，回调继续传原 ID/路径。

当前所有权规则比 SMART_MIXED 提案更宽松：它会穿透符合边界的已有物理子树，不根据作者/不同作品名做本实验的 Category 判断。未来不能只在前端改变标题或隐藏目录来冒充完成归属接入。

## 已有生产测试审查

| 测试符号（均在 comics.rs） | 证明其测试代码覆盖的保护 |
|---|---|
| `individual_books_flatten_and_preserve_identity_progress_and_failures` :1410 | 文件模式、同书 ID/Node 名称/书签/进度、取消/损坏源保留 |
| `book_detail_flattens_owned_subfolders_but_respects_explicit_boundaries` :1489 | 人工独立/IGNORED 边界 |
| `complete_book_detail_keeps_counts_books_and_independent_continuations_consistent` :1518 | 正篇 14 册与独立 :re 194 册边界、hero 数量 |
| `book_details_remove_expanded_format_folders_but_keep_nested_attachments_and_hidden_boundaries` :1563 | 已展开格式目录不重复、附件/隐藏边界保留 |
| `nested_volumes_partial_refresh_cancellation_and_deleted_pages` :1988 | 局部扫描、祖先计数、取消和页面删除后的进度 |
| `png_folders_and_mislabeled_supported_images_read_by_content` :2113 | PNG 及后缀与真实图片签名不一致 |
| `matching_and_poster_scope_share_detail_boundaries_without_changing_books` :2309 | 折叠册不额外匹配/制图，独立边界保留 |
| `artbook_modes_persist_independent_matching_policy_and_reader_metadata` :2418 | ARTBOOK 两模式和独立匹配设置 |
| `rescan_promotes_previously_opened_book_attachment_without_cascading_state` :2661 | 附件转核心仍保留阅读状态 |
| `kindle_reindex_preserves_chapter_locators_in_progress_and_bookmarks` :2718 | Kindle 修订后章节定位 |

这些现成测试应在未来接入时继续全量执行；本轮新测试验证的是纯逻辑清单，不会伪装成生产验收。

## 本轮实际改变

只新增 `tools/smart-mixed-lab/` 和 `docs/smart-mixed/`；私有 `.tmp/smart-mixed-phase1/` 保存安全快照。没有改任何上述源码、migration、前端、i18n、CSS、Skill、AGENTS 或原有项目文档。具体 SHA/状态比较见 `scope-verification.json` 与测试报告。
