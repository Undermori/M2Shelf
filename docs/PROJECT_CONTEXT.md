# M²Shelf 当前项目上下文

本文记录仓库当前实现，供新开发者和新 Agent 定位代码。产品行为以 `PRODUCT_SPEC.md` 为准，稳定约束以根目录 `AGENTS.md` 为准。

Bangumi 绑定标题可点击，在系统默认浏览器中打开对应官方条目页面。后端读取当前绑定的 Subject ID 并生成固定 https://bgm.tv/subject/{id} 地址，不接受任意外部 URL。

按最新界面要求，详情页不再渲染别称区块；自动补全、缓存、别称搜索及绑定跳转保持可用。

WorkDetailPage 的资源来源折叠区位于 detail-content 最后；来源数量只显示在该折叠区，不再占用顶部标题区域。

本次 PR 规则和可靠性修订已于 2026-10-06 完成 Windows x64 本地构建及 Portable 打包，并更新现有安装。主程序、辅助程序通过版本、身份、私有构建路径和复制后 SHA-256 检查，原生窗口启动正常；更新前保留程序和 SQLite 一致备份，升级后 quick_check 正常，10 类原始身份与用户数据摘要一致。旧库没有启动扫描设置时按规则保存 false，数据库迁移至 14。此构建保留 0.5.11，不更新公开 Release、tag、公钥或正式签名。当前验证结果见下文。

## PR 规则统一实现

本轮可靠性修订：WorkTarget 摘要排除观看记录、标签和无关时间戳，增加所属文件、独立边界和当前分组成员；成员索引预计算以避免大组重复计算。作品事务单独验证完整有序来源集，不使用普通批量编辑的 500 项上限。`change_work_binding` 在事务内返回 `WorkBindingChange`（旧缓存路径与封面快照），封面阶段不再提交后采集状态；`retry_work_bangumi_cover` 接受可选 `failedSourceNodeIds`，处理损坏缓存及浏览器解码失败，范围受当前 WorkTarget 约束。列表、目录浏览和普通/聚合详情共用 deferred 读取快照；单条分类和祖先刷新使用 immediate 事务。详情与浏览检查隐藏祖先，过期详情刷新后返回资源库；隐藏管理继续保留恢复所需数据。

设置“关于”显示原作者与 Juvenile_A 的维护角色。构建脚本设置实际 `M2SHELF_BUILD_DATE` 并恢复调用者环境；直接开发构建未提供日期时使用 `unknown`，界面显示不可用。新增回归涵盖千来源事务回滚、失效目标、损坏封面、读取并发和弹窗焦点。当前 PR 改动尚未作为正式 Release 发布；公开 v0.5.11 的状态见文末，不能把本地构建与已公开资产混为一谈。

`logical_works.rs` 从批量读取的 SQLite Node/文件/绑定计算逻辑归属，扫描、恢复分类、升级迁移、作品库及普通自动匹配共用。作品候选仅限拥有视频的 WORK/AUTO_WORK，系列仅保留手动绑定。保守编号分卷在 FOLDER 模式归属父作品，保留人工/绑定/隐藏边界，不新增作品实体或改变源身份。

`works.rs` 查询所属内容并生成 WorkTarget（有序来源 ID 与快照摘要）。普通/聚合 NodeDetail 共用 nestedMediaFiles、expandedFolderIds、recognitionWarnings；前端 P2 平铺视频，保留附件/目录入口、相对目录和来源，来源区在底部。Bangumi 整组命令事务核验来源及快照，复用 Subject 与封面下载，保护手工封面；其他聚合菜单进入 SourceChoiceDialog。

migration 0014 增加 library_scan_health；逐库结算成功基线与错误，listRoots 返回健康。无变化后台完成仅刷新 Root 状态。数据库初始化区分新库 true / 旧库无设置 false，并保留既有选择。alias_sync.rs 维护进程级32 Subject 预算与 provider 熔断，settings 游标轮换，前端启动空闲触发一次。

`.github/workflows/windows-pr.yml` 使用既有固定 Action 版本及 Node22/Python3.12/Rust1.88，在 PR 中执行 npm ci/check、rustfmt、locked tests 和严格 Clippy，不签名或发布。迁移验证摘要从迁移文件清单生成。

Vite 的开发 watcher 排除原生源码、Portable 打包、临时测试、工具及 Playwright 产物，避免 Windows 在创建或删除 EXE 时的文件锁使开发预览退出。前端源码继续正常热更新。

本次验证：35 项前端测试、230 项 Rust 测试和120项项目检查通过；5项既有外部网络测试按原配置忽略。TypeScript、生产前端构建、rustfmt 和 all-targets 严格 Clippy 通过。虚构资源完成四语言、深浅主题及1280×800/900×640共16组详情和关于页面布局检查；扫描健康与来源选择界面另行核对。真实生成的 Unicode/特殊字符测试视频通过原生播放器启动及精确 Explorer 选择，后者补齐线程 COM 初始化，并用回归测试验证初始化引用平衡。

## 当前状态

- 版本：`0.5.11`
- 目标：Windows x64 桌面应用
- 前端：React 19、TypeScript 5.8、Vite 6
- 客户端：Tauri 2、Rust 2021
- 数据：SQLite（`rusqlite` bundled）
- 网络：`reqwest` + rustls native roots，使用系统代理
- 主要外部服务：Bangumi 官方 API / 封面主机，以及 M²Shelf GitHub Releases 更新清单与资产

仓库没有独立服务器。所有数据库访问在 Rust 中完成，前端通过类型化 Tauri 命令通信。

启动资源库扫描与作品聚合视图此前已完成 Windows x64 本地构建，尚未签名或发布新的分发版本。`scripts/build_windows_release.ps1 -Bundles none` 成功生成主程序和更新辅助程序，并通过其架构与构建路径检查；该次环境配置检查未执行 NSIS/Portable 打包。`works.rs` 在现有 Node 索引上生成只读分组，`AllResourcesResult.works` 与 `get_work_detail` 提供聚合卡片和来源详情；原目录查询与 Node 身份继续保留。`AppSettings.auto_scan_on_startup` 使用已有 settings 键值表持久化，新库缺省为 true、旧库无设置缺省为 false，明确值保留，该开关继续使用 settings；增量快照另以 migration 0011 保存。前端等待初始化及扫描监听就绪后仅尝试一次静默启动扫描，使用 `ScanProgress.background` 区分手动任务，仅在终态 `libraryChanged` 为 true 时重载索引；未变化时保留现有显示。绑定修改后重新计算作品分组，沿用请求代次保护。

前端回归测试入口为 `npm test`（Vitest、jsdom、Testing Library），测试使用虚构资源和 IPC mock；Rust 扫描测试补充了新增集数、季区分、深层目录归并、改绑拆分和元数据保留场景。Windows 原生环境已补齐 Rust/Cargo 1.98.1、rustfmt、Clippy 和 Visual Studio Build Tools 2022，Tauri 环境自检通过。首次原生验证修复了隐藏条目测试的分类准备，以及作品视图中显式 ` - 01` 集号和 `S02E01` 季集号的处理，保留裸数字续作标题。该次原生环境准备验证为199项 Rust 测试、15项前端测试及120项项目校验通过；本次 PR 修改的最新结果见上文。回归覆盖深层增量变更、文件夹/文件模式的未变化子树复用、BDMV、离线/取消基线保留及隐藏恢复。开发与构建命令见 [Windows 本地开发](WINDOWS_DEVELOPMENT.md)。本地构建不代表正式签名发布。

本轮增加 `modified-desc` / `modified-asc` 排序。`db.rs` 的 `hydrate_file_modified_times_conn` 用分批递归 SQL 查询视频与附件修改时间，排除隐藏后代，并随 Node DTO 返回 `latestFileModifiedAt`；`works.rs` 合并来源时取最大值。`format.ts` 使用真实时间戳比较、缺失置后，`FileModifiedTime` 统一显示；作品、目录及收藏夹可用，目录直属文件列表也支持。隐藏条目入口已从 Sidebar 移到 SettingsPage 的资源库区，对话框仍由 App 管理，恢复后的索引刷新和焦点返回继续保留。 本轮使用虚构 IPC/媒体数据完成四语言、深浅主题和 1280×800 / 900×640 窗口检查，并验证系统主题切换、列表时间显示及切换其他排序后隐藏。浏览器验证不访问真实媒体或应用数据库。

## 入口和目录

### 前端 `src/`

- `main.tsx`：React 入口和 i18n Provider；
- `App.tsx`：启动装载、页面切换、扫描事件、导航历史和跨页面操作；
- `types/media.ts`：前端 DTO、语言、主题、分类、排序和设置类型；
- `lib/api.ts`：唯一的 Tauri invoke 封装；
- `lib/i18n.tsx`：`zh-CN`、`en-US`、`ja-JP`、`ko-KR` 文案及标题选择；
- `components/UpdateBanner.tsx`、`UpdateDialog.tsx`：更新可用提示，以及紧凑的说明、下载进度和安装确认对话框；设置/About 页面只保留同层级的自动检查开关与手动检查按钮；
- `components/PosterImage.tsx`、`lib/poster.ts`、`hooks/useCoverDataUrl.ts`、`hooks/usePosterViewportLifecycle.ts`：缓存图加载以及主列表/详情共用的 DPR 对齐高质量重采样；128 项/约 32 MiB 源图 LRU 提供即时预览，128 项/128 MiB 最终位图 LRU 跨页面同步恢复清晰 Canvas，64 项挂载 Canvas 工作集独立受限，列表以内部滚动容器为观察根并使用预热区和保留区；
- `pages/`：Onboarding、All Resources、Browse、Search、Recently Watched、Favorites、Work Detail、Settings；
- `components/`：海报网格、文件列表、标签筛选、上下文菜单、编辑模式和对话框；
- `styles.css`：语义主题变量、布局和响应式样式。

前端没有 React Router 或独立全局状态库。`App.tsx` 维护轻量页面状态，并结合 `window.history` 与目的地快照实现返回和滚动恢复。扫描事件监听在应用生命周期内只注册一次，通过最新回调引用避免页面状态变化导致重复监听；异步注册结束后会用原生扫描快照补偿注册窗口内的终态事件。

### Rust `src-tauri/src/`

- `main.rs` / `lib.rs`：Tauri 入口、状态初始化、迁移、命令注册和窗口生命周期；
- `models.rs`：Rust 领域模型和序列化 DTO；
- `commands.rs`：资源库、浏览、搜索、设置、匹配、缓存、标签、收藏夹和原生操作命令；
- `db.rs`：连接、migration、查询、事务、自然排序和持久化设置；
- `incremental.rs`：启动目录项快照、差异目标规划与成功后基线保存；
- `scanner.rs`：只读目录遍历、视频/附件索引、BDMV 处理、分类和旧行清理；
- `title_extractor.rs`：标题、季度、年份、字幕组和文件噪声提取；
- `auto_match.rs`：有界多查询、候选合并、评分和置信度门禁；
- `bangumi.rs`：官方搜索、Subject 获取和封面请求；
- `cache.rs`：应用封面缓存校验、同目录原子提交、读写/清理屏障与拥有文件清理；
- `player.rs`：播放器测试、字面参数启动及带线程 COM 生命周期保护的精确 Shell 文件定位；
- `window_state.rs`：窗口尺寸校验、恢复和保存；
- `update.rs`：固定 GitHub 清单、规范稳定 SemVer、下载状态、有界网络、SHA-256 与 Ed25519 验证；
- `portable_update.rs`：Portable 事务准备、helper-ready、受限解包、替换、健康回执、SQLite/文件回滚和恢复提示；
- `single_instance.rs`：Windows 单实例互斥锁及已有窗口唤醒；
- `bin/m2shelf_updater.rs`：随 Portable 分发的更新 helper，同时为受控线下签名/发布提供严格的 identity、sign 和公钥 verify 命令。
- `tools/offline-key-init/`：独立 Cargo crate，仅用于隔离 Windows 账号首次建立或轮换生产 Ed25519 信任根；不会进入主 crate、CI、NSIS 或 Portable，分发 updater 也不再包含 keygen。

### 配置与脚本

- `src-tauri/tauri.conf.json`：窗口、CSP、asset scope、包标识和图标；
- `src-tauri/capabilities/default.json`：最小 Tauri 权限；
- `src-tauri/icons/`：用户确认母版及派生的 PNG、ICO、SVG；
- `scripts/validate_project.py`：跨源码和构建契约验证；
- `scripts/run_validate.mjs`：先探测可用 Python 解释器并跳过无效的 Windows Store 别名，再运行验证器；
- `scripts/build_windows_release.ps1`：公开 Windows 主程序、updater helper 与 NSIS 构建；验证稳定版本、x64 PE、产品元数据和隐私路径，并输出固定命名安装包及 SHA-256；
- `scripts/build_offline_key_init.ps1`：只编译独立密钥初始化工具；先执行该 crate 的 fmt/test/clippy，再做锁定依赖、路径重映射、x64 PE、隐私扫描和 SHA-256，并只写入全新的版本化交付目录；不会运行 `init`、覆盖旧交付或接触密钥材料；
- `tools/portable-key-tool/`：非分发的可移动密钥工具；提供一次性 `migrate-dpapi`、只读 `verify-key` 和长期 `sign-release`，直接复用 `m2shelf_lib::update` 的签名与验签实现；
- `scripts/build_portable_key_tool.ps1`：对可移动密钥工具执行锁定 fmt/test/clippy/release 构建和 x64/隐私/校验和验证，不执行迁移或签名；
- `scripts/build_portable.ps1`：Portable 目录和 ZIP；
- `scripts/generate_update_manifest.ps1`、`scripts/sign_update_offline.ps1`：保留的旧 DPAPI 签名兼容实现，不再作为普通发版入口；
- `scripts/sign_update_from_usb.ps1`：日常生产签名入口；只定位唯一 USB 加密密钥和当前候选，由 Rust 工具内部交互解密、签名、自验并生成固定八项返回包；
- `scripts/publish_signed_release.ps1`：在本地和远端条件全部匹配后创建、核对并发布不可变 Release；
- `.github/workflows/windows-release.yml`：固定 action commit 的质量门禁和无签名 Windows 候选构建，只上传资产、校验和与来源证明；
- `scripts/generate_icons.ps1`、`build_brand_assets.ps1`：从固定母版生成图标。

## 数据库

`db.rs` 在应用启动时按序执行 `src-tauri/migrations/`，并记录 schema 版本。迁移必须只增量升级。

当前 migrations：

1. `0001_initial.sql`：`library_roots`、`nodes`、`media_files`；
2. `0002_mvp.sql`：视频统计、`metadata_bindings`、`settings`、`scan_runs`；
3. `0003_resources_and_cover_status.sql`：`resource_files` 和封面错误；
4. `0004_multilingual_metadata.sql`：英、日、韩 Bangumi 标题；
5. `0005_user_tags.sql`：`tags`、`node_tags`；
6. `0006_watch_history.sql`：`watch_history`；
7. `0007_favorite_folders.sql`：`favorite_folders`、`node_favorite_folders`。
8. `0008_bangumi_subject_type.sql`：为绑定增加 `provider_subject_type`；旧记录兼容默认动画 type 2，新绑定只允许动画 type 2 或真人影视 type 6。
9. `0009_library_recognition_mode.sql`：为每个 Root 保存 `FOLDER` / `VIDEO_FILE` 识别方式；旧 Root 默认 `FOLDER`。
10. `0010_confirmed_title_aliases.sql`：保存用户手动确认产生的本地标题别名观察，以来源 Node 外键级联清理，并按规范化别名建立查询索引。
11. `0011_incremental_scan.sql`：每个 Root 的目录项快照 JSON，删除 Root 时级联清理；旧索引与绑定不变。
12. `0012_provider_aliases.sql`：向 metadata_bindings 增加 provider_aliases_json，旧绑定默认空数组。
13. `0013_alias_sync.sql`：保存按 Subject ID/type 去重的官方别称缓存与完成状态。
14. `0014_scan_health.sql`：逐库扫描健康及最后尝试/成功时间；应用迁移事务同时从已有索引重算自动分类，保留用户元数据。

关键关系：

- 一个 Library Root 有多棵 Node 树；
- Node 通过 `parent_node_id` 形成同 Root 层级；
- 视频和附件分别落入 `media_files` 与 `resource_files`；
- 每个 Node 最多一个 Bangumi 绑定；
- 人工确认别名按来源 Node 保存；同一规范化文本只有在全部观察一致指向同一 Subject 时才可复用；
- 标签、收藏夹通过关联表实现多对多；
- 最近观看每个 Node 一行，删除 Node 时外键级联；
- `settings` 同时保存 AppSettings（含 `auto_check_updates`）、窗口尺寸和分作用域排序键；更新器运行态和下载进度不写入 SQLite。

## 核心调用流程

### 启动

Rust 先取得 Windows 单实例锁，再以隐藏状态创建主窗口，创建应用数据目录、打开 SQLite、执行 migrations，并在首次显示前恢复经过 DPI/工作区校验的窗口尺寸。第二个普通启动会唤醒已有窗口后退出。前端先独立应用持久化语言和主题，在 React 首屏提交后通知原生窗口显示；其余 bootstrap、资源库、排序、扫描状态、全部资源和最近观看继续并行装载。若上一轮 Portable 更新已自动回滚或需要人工恢复，bootstrap 会持续返回尚未确认的恢复状态，且该结果独立于其他并行初始化请求落地；前端以本地化模态警告持续展示，只有用户明确确认且原生清理成功后才移除持久通知。

### 扫描

启动扫描先通过 `incremental.rs` 只读枚举元数据并比较 SQLite 快照，使用 SHA-256 摘要保存各目录直接子项，不读取视频内容或仅依赖根目录 mtime。Folder 模式规划最小已索引祖先目标，保留未变化子树并更新祖先计数；VideoFile 模式保留 Root 清理范围，但只遍历变化分支，并批量读取 SQLite 旧条目来保留未变化物理子目录中的 Node、附件及计数。扩展名、识别方式和隐藏路径纳入快照配置，手动扫描使相关 Root 基线失效。索引过程无新增错误且未取消才保存成功 Root 在扫描前采集的快照；读取失败的 Root 保留索引与基线，不阻碍其他可访问 Root 建立基线。快照差异决定终态 `libraryChanged`，后台进度事件不展示横幅或 toast，无差异不触发集合/详情重载。


前端添加 Root 时先用四语言模态框选择识别方式，再把选择随 Root 写入 SQLite。`scanner.rs` 的 `FOLDER` 分支原样保留目录树、分类和局部扫描；`VIDEO_FILE` 分支递归只读遍历，把普通视频以真实文件路径创建为隐藏 Root 下的扁平 `AUTO_WORK` Node，每个 Node 挂一个媒体文件并独立进入绑定，BDMV 则按一套结构聚合为一个 Work。非视频资源挂在隐藏 Root，不成为作品。文件 Node 的局部重扫请求由 Rust 升级为整 Root 扫描；只有完整无错误遍历才清理未见旧 Node。`db.rs` 更新 Node、文件、计数和分类。Rust 以扫描生命周期互斥锁和独立 worker 活跃标记串行化扫描、匹配现有资源、清缓存、删除 Root 与更新退出准备；活跃状态持续到后台线程完整退出，不能在终态事件与线程收尾之间启动第二个 worker。完成后，未绑定且合格的 Node 可进入自动匹配；人工分类和应用元数据不被普通扫描覆盖。自动匹配在每个 Node 开始时发布活动项，并在结果落定后立即发布累计的成功、未匹配和错误数；旧 `auto_match_pending` DTO 字段仅为前后端兼容保留且始终为零，当前界面不再展示待确认列。

### 自动匹配

`title_extractor.rs` 生成结构化证据，并仅对具有年份/技术段的多点发行名把点号视为分隔符。影视发行名中的全角方括号、嵌套 HDR/字幕等技术组使用平衡括号清洗；相邻 CJK 与拉丁标题会保留组合证据并拆成独立候选，电影、影视、Movies 等通用父目录不占查询名额。显示名与真实名不同的情况下，真实文件夹或文件标题仍紧随主标题进入三查询计划。`auto_match.rs` 公平合并最多三个官方搜索的候选，依据多语言标题、季度、年份、版本和类型评分。高置信结果只补全胜出项以自动保存官方别称，其余结果最多补全五项，详情以两路小批并发执行。单次匹配运行最多新发起 256 个 Subject 详情请求，同一 Subject 复用本轮缓存；预算允许时为每个后续 Node 预留一次未缓存详情机会，避免前部 Node 独占预算。详情服务出现提供方级故障后本轮停止新详情请求并继续以搜索元数据评分。

最终排序后的最高候选只要达到 `direct_threshold`（当前默认 60）、没有强季度/年份/版本/类型冲突，就直接写入绑定；不再要求与次高候选保持分差，也没有用户可见的 Pending 结果。主关键词的 Bangumi 第一项在本地证据足够具体时可提升到该直接门槛，但全部硬冲突仍在提升后执行；普通匹配候选仅为逻辑作品，系列可手动绑定。`automatic_threshold`（当前默认 82）继续用于识别可跳过更多详情补全的高置信快路径，不是唯一写入门槛。候选同分时以提供方排名稳定决胜。`bangumi.rs` 只接受动画 type 2 与真人影视 type 6，其他 Subject 类型保持强冲突；`provider_subject_type` 随绑定持久化，使真人影视可在重启后重新读取详情和重试封面。普通路径不替换绑定或手工封面，显式批量重匹配仍经过相同直接门槛和硬冲突保护。

`commands.rs` 在用户明确选择并绑定 Bangumi 条目时，从当前 Node 自身的显示名、真实文件夹或文件名和代表性视频标题生成有界清洗别名，与绑定一起事务写入 `confirmed_title_aliases`；父目录上下文、路径和自动绑定结果不会进入表，入库不会再次剥离已提取出的季数或年份。`auto_match.rs` 按证据顺序查询这些本地观察：采用第一个有记录且其全部来源一致指向一个受支持 Subject 的别名；若该高优先级别名自身有歧义就回退普通官方搜索。唯一 Subject 获得官方详情补全优先级，同时仍执行普通有界官方查询，因此别名过期、详情失败或硬冲突时可继续选择搜索候选。确认别名只提供标题精确信号，不能反向伪造官方季数、年份或版本证据；复用结果仍需通过当前 Node 的硬冲突和系列精确标题保护。清除或替换来源绑定及删除来源 Node 会移除对应观察。该机制不引入新的第三方标题 API，也不把用户本地标题上传到 Bangumi 之外的服务。

目录或显示名没有年份时，提取器会从视频文件名补取唯一占优年份，并区分标题数字、后出现的真实发行年以及 `1920x1080`、`2048×1080` 等分辨率；文件名提供后续真实年份时，会恢复 `Blade Runner 2049`、`2001 A Space Odyssey` 一类位于标题任意词位但被误作年份的四位数字。包含多个不同年份的发布名或仅由文件名推断的年份作为排序证据而不构成硬冲突；明确写在目录或显示标题中的单一年份仍可阻止错误绑定。纯四位数字片名只有在文件名还提供独立发行年时才受限放行，单个歧义数字文件保持不查询。普通纯标题查询之后，只有有效主标题且未占满三次查询预算时才补充“作品名 + 年份”，所以不会替换文件原名、真实文件夹名或多语言独立候选。

### 封面

Bangumi 封面下载到活动应用缓存，Node 保存实际缓存路径和失败原因。本地手工封面也复制到缓存。两类写入均使用目标目录内 UUID 临时文件并原子替换，并在提交前校验格式、字节数和像素尺寸；旧缓存读取也复验像素尺寸。失败时保留旧成品和有效绑定。同一匹配运行遇到封面 CDN 的提供方级故障后停止继续放大请求。清缓存或物理文件丢失后，扫描候选会沿用绑定中保存的 Subject 与图片 URL 只恢复封面，不重新搜索、改绑或覆盖手工封面。普通读取/写入共享缓存操作屏障，显式清理独占屏障并在扫描或自动匹配运行时被后端拒绝，因此文件变化和 SQLite 路径更新不会交错。切换缓存目录只影响新写入；旧路径继续可读，清理范围仍受应用拥有目录与文件名限制。

### 浏览与导航

Rust 返回已 hydrate 的 Node DTO；列表所需的 Bangumi 绑定与用户标签按最多 500 个 Node 分块批量查询，避免全部资源和目录浏览退化为每项两次附加 SQL。`Database::search` 也在同一个 deferred SQLite 读事务中完成 Node 命中、文件命中及其所属 Node 补齐、批量绑定/标签 hydrate、自然排序与最终截断，确保本地搜索结果来自同一快照且不出现逐项查询。前端按当前语言选择标题；主列表、详情与搜索结果共用 `PosterImage`，封面 IPC 保持 4 路并发，并用 128 项/约 32 MiB 字符预算的 LRU 缓存精确版本 data URL。页面切换时，已排队读取继续完成并预热此跨页缓存；同一 Node 的旧 revision 即使晚完成也不能重新写入 LRU。命中时先同步显示普通源图预览。渐进重采样后的 ImageBitmap 以 128 项和 128 MiB 双上限另行跨页缓存，命中时在 `useLayoutEffect` 中为实际可见卡片同步画回 Canvas，因此返回页面无需再次解码和缩放；render generation 阻止旧异步任务覆盖新封面。主列表以 `.content-scroll` 为显式观察根，在上下 1000 px 内预热、1800 px 内保护；搜索结果分别使用 800/1400 px。挂载 Canvas 另保留在跨列表共享的 64 项目标工作集；超限时只淘汰最旧且位于保护区外的 Canvas，保护区内项目允许暂时软超限，Canvas 不在时仍显示源图预览。源图 LRU 达到容量后淘汰最旧项，并向已经释放的挂载卡片发送一次性精确失效通知，防止 data URL 与监听引用无界积累。大图重采样首级即裁剪，之后最多约 2:1 渐进缩小。全部资源、搜索、最近观看、收藏夹和每个 Root 各有会话快照。集合与 Root 加载器分别维护请求代次，只有最新响应可提交数据或关闭 loading；收藏夹 folders/nodes 共享 epoch 与 pending 集合，避免一个旧请求提前结束另一个请求的 loading。历史返回恢复快照；资源库与收藏夹恢复会重新查询当前行，避免把旧业务数据写回 UI。

Bangumi 手动弹窗为每次预填、搜索和绑定维护 Node ID 与请求代次。关闭弹窗、切换 Node 或手动修改关键词会使旧异步结果失效，旧预填不会覆盖用户输入，旧搜索结果不会显示到新关键词或新作品，旧绑定回调也不会关闭或刷新新作品的弹窗；绑定提交期间输入保持锁定。

### 播放

`player.rs` 以程序路径启动，并只把媒体绝对路径作为一个独立字面参数传入，不经过 shell，也不附加 mpv 专属的 `--`。mpv/VLC 的显式测试可执行有界 `--version`；其他 GUI 播放器只校验已选择的 `.exe`，避免测试参数反而唤起空窗口。只有成功 spawn 后，`db.rs` 才 upsert 最近观看记录。

### 稳定更新

前端启动后仅在 `auto_check_updates` 开启时静默检查一次，也可从设置页同层级按钮手动检查；自动检查不下载、不安装。手动检查无更新或失败时只显示本地化 toast，只有发现有效新版本才打开紧凑更新对话框。`update.rs` 读取固定的 `https://github.com/Undermori/M2Shelf/releases/latest/download/latest.json`，严格解析四语言说明和 Portable/NSIS 两个平台，拒绝预发布、build metadata、相同/更低版本、未知字段、错误文件名或非固定版本 Release URL。若该地址 404，只有最终 URL 精确指向本仓库规范的 `v{version}/latest.json` 且版本不高于客户端时才返回无更新；更高版本缺清单仍失败，且此分支不提供任何下载候选。

下载使用应用更新缓存中的唯一 partial 文件并限制大小、主机、重定向和超时。更新缓存根、版本目录和 Portable 事务目录在写入、枚举或清理前都验证为应用目录中的普通非重解析目录，并再次与 Library Root 比较真实路径；旧缓存清理只删除严格命名且确认普通的更新文件，不递归穿过链接。完成下载后核对长度和 SHA-256，再用仓库编译进主程序的 Ed25519 公钥验证绑定 `appId + version + platform + size + digest` 的签名；执行前对打开的文件再锁定并复验。检查、下载和安装由后端互斥，前端也抑制重复操作。

NSIS 分发启动已验证的固定版本安装器。Portable 分发先确认当前目录标记、更新缓存及安装目录的 Library Root 隔离，复制已安装的可信 `M2ShelfUpdater.exe` 到应用数据事务目录，并在取得 SQLite 单写入屏障后通过 online backup 建立一致快照及长度/SHA-256 记录；该屏障保留到旧进程退出，避免快照后的写入在回滚时丢失。helper 全程持有 Windows 命名更新互斥锁，在旧程序退出前锁定、复验并密封 ZIP，原子写入 helper-ready；普通手动启动会先等待该锁。只有新版子进程携带的规范事务 ID 能同时认证精确活动事务、当前可执行文件、目标版本和 `Launched` 阶段时才允许绕过等待。helper 严格验证 ZIP 的扁平固定文件集、Portable 标记和 Windows 产品版本，在同卷暂存/备份后替换且最后处理 `M2Shelf.exe`，启动新进程并等待精确版本健康回执及完整三秒存活观察。成功后先原子写入仍保留事务材料的终态 `Completed`，完成受校验清理后再清除保留标记；下次启动会续作中断的 `Completed` 清理。失败时先确认新进程终止、逆序恢复文件，再以长度、SHA-256、SQLite `quick_check` 复验快照，预检并隔离 WAL/SHM 后原子恢复主库；无法安全恢复时保持当前数据库并保留材料。helper 异常终止留下的非终态事务会在下次启动严格识别并标为 `RECOVERY_REQUIRED`，不执行缺少可靠文件日志的猜测式回滚。回滚和人工恢复提示都持久显示到用户明确确认。

`v0.5.8` 仅保留为 CI 失败的不可变历史 tag，`v0.5.9` 仅保留为最终修复前创建且未公开的不可变历史 tag，`v0.5.10` 仅保留为最终匹配修复和生产密钥轮换前创建的不可变未发布 tag；三者都没有 Release、资产或 `latest.json`，不授权更新。`0.5.11` 是第一个携带该更新器、Portable helper 与新生产信任根的引导版本，其正式签名 Release 已存在并公开。`0.5.7` 及更早用户和任何旧公钥测试包用户都需要手动安装 `0.5.11` 一次，之后客户端才具备兼容的更新能力。

## 持久化设置

`AppSettings` 保存播放器、默认视图、视频扩展名、Bangumi 开关、封面缓存、语言、主题和 `auto_check_updates`。设置页采用序列化自动保存；该布尔值只决定启动检查，不授予自动下载或安装权限。

窗口尺寸使用独立 `settings` 键，由原生生命周期保存；原生窗口隐藏创建，恢复尺寸和前端主题首帧完成后再显示。全部资源、资源库浏览和收藏夹排序也使用独立键。完整 AppSettings 更新不得覆盖这些键。

## 安全边界

- Library Root 只读；应用写入仅限 SQLite、应用缓存和构建输出；
- Library Root 添加命令先 canonicalize 并检查重叠，数据库在 `BEGIN IMMEDIATE` 事务内再次拒绝等于、祖先或子孙 Root；扫描命令与 worker 入口还会复核登记路径和旧数据库中的重叠 Root，防止 Node 归属漂移；
- 扫描会对根、局部目标、递归目录和文件重新 canonicalize，并拒绝 Library Root 外的链接或重解析目标；BDMV/STREAM 探测同样先验证真实路径边界并跳过 symlink，不能通过局部扫描或人工重置分类读取 Root 外目录；
- Tauri capability 只开放所需能力，前端无直接 SQL；
- CSP 限制资源和连接来源；
- Bangumi 请求有官方主机白名单、TLS、连接/总超时、搜索/详情响应体上限和有界重试；HTTP 429 的数字 `Retry-After` 最多等待两秒，一次提供方级搜索失败会停止本轮剩余在线匹配；
- 更新请求有独立的 GitHub HTTPS 主机/重定向白名单、严格清单 schema、固定资产 URL、大小上限、SHA-256 和内置 Ed25519 公钥验证；404 兼容只可证明旧版/同版无更新，不能产生安装候选；
- 更新缓存与事务目录拒绝 symlink、junction 和 Windows reparse point，按真实路径保持在应用目录且位于所有 Library Root 之外；清理仅处理严格识别的普通文件与空目录；
- 文件和播放器路径不经过 shell；
- 自定义缓存不可位于 Library Root；
- 手工封面在原子写入缓存前校验 15 MiB 上限、格式签名和像素尺寸；播放器测试有 5 秒超时；持久化 IPC 文本有后端长度上限；
- 批量标签、收藏夹和分类操作先验证 Node 集并事务提交；
- Portable 安装目录不得与 Library Root 重叠；更新 ZIP 仅接受固定的五个扁平普通文件，拒绝目录、链接、未知项、路径穿越、大小写重名和越界尺寸；
- 生产 Ed25519 私钥不在源码或 GitHub；CI 仅产出无签名候选和仅供核对的 `candidate-provenance.json`。新增独立工具可在再次获得明确确认后，把现有 seed 一次性迁移为 Argon2id + XChaCha20-Poly1305 加密的可移动 `.m2key`，不绑定 Windows 用户或单机；旧 DPAPI 文件不会删除。密码和解密 seed 只在非分发 Rust 工具内存中存在，不通过参数、环境、日志或临时文件。Agent 只能使用合成 seed 测试，真实生产迁移或轮换必须重新取得所有者明确确认；公钥、updater、签名格式和八项 Release schema 保持不变；
- 仓库不得包含密钥、个人路径、真实索引数据库或私密截图。

## 自动别称和搜索顶栏修订

手动别称同步已移除。识别补全、首次启动的可续跑后台补全、完成缓存和已完成条目的新来源复用均已有回归覆盖。搜索页复用 page-toolbar/page-title-row/browse-controls，高度和 36 像素控件已在四语言、深浅主题、1280×800 / 900×640 虚构 IPC 浏览器场景验证。

本次验证为 20 项前端测试、202 项 Rust 测试、120 项项目检查通过，5 项外部环境 Rust 测试按原配置跳过，rustfmt 和严格 Clippy 通过。本地构建使用独立的 `src-tauri/target/auto-alias-build` 输出目录，避免覆盖已运行的旧程序。

## 排序和搜索补充

本轮完整验证结果：20 项前端测试、200 项 Rust 测试通过，5 项依赖外部环境的 Rust 测试按既有配置跳过，120 项项目验证通过，rustfmt/严格 Clippy 通过。旧主程序运行时默认输出文件被 Windows 锁定，因此通过 `scripts/build_windows_release.ps1 -Bundles none -TargetDirectory src-tauri/target/local-build` 生成独立主程序与辅助程序，并通过同一 x64/构建路径检查；未终止或替换正在运行的程序。构建脚本新增可选 TargetDirectory，也支持已有 CARGO_TARGET_DIR，检查实际输出并在退出时恢复环境。

`CollectionSortControl` 将四种字段和方向分开，继续使用兼容的 `field-direction` settings 值，增加 `watched-asc/desc`。Node 批量时间聚合查询同时读取 watch_history，并返回 `lastWatchedAt`；聚合作品取所有来源最大时间。`FileModifiedTime` 使用 Intl.RelativeTimeFormat 和一个共享分钟计时器，title 保留完整时间。

侧栏 navigate/search 清除隐式 root 限制，保留关键词；目录内 openSearch 仍明确传 root，SearchPage 提供始终可见的范围选择。后台查询保留请求代次保护。`MetadataBinding.providerAliases` 通过 migration 0012 持久化，写入限制 32 条、每条 256 字符并清理空白/控制字符。搜索 SQL 使用 json_each 和已有 LIKE 字面转义；顶栏使用 nodeMatchesQuery 匹配各语言标题、别称和标签，聚合视图同时检查所有来源。`sync_pending_bangumi_aliases` 和 `alias_sync.rs` 在启动扫描空闲后静默补全未完成 Subject（每进程最多32个不同条目）；不提供手动入口。migration 0013 的 provider_alias_sync 同时保存完成状态和别称缓存，成功空数组也完成，新绑定复用缓存。每次应用进程最多尝试32个唯一 Subject，使用持久游标轮换读取官方详情；单项 404 继续其他项，提供方整体故障中止本轮，已成功记录保留。只在别称变化时重载索引显示，并通过最新刷新回调保护导航。

新增回归覆盖跨 Root 查询、侧栏重入范围、LIKE 通配符字面值、别称持久化/同步并发保护、观看时间聚合和双向缺失置后。四语言、深浅主题及 1280×800 / 900×640 使用虚构 IPC 数据检查控件等高和相对时间。仅生成本地未签名的 exe，不代表正式发布或真实媒体环境中的原生 UI 验证。

## 修改路由

- UI / 页面：先看对应 `pages/`、`components/`，再看 `App.tsx`；
- 前后端接口：同步改 `types/media.ts`、`lib/api.ts`、`models.rs`、`commands.rs`；
- 数据结构：新增 migration，并补 `db.rs` 兼容测试；
- 扫描：检查完整/局部扫描、清理、取消、分类和只读边界；
- 匹配：同步检查 extractor、scorer、Bangumi client 和现有测试；
- i18n：四种语言一起更新；主题：检查 light、dark、system；
- 更新协议：同步检查 `update.rs`、`portable_update.rs`、helper、四语言 UI、验证器和发布脚本；签名消息或 `latest.json` schema 属于兼容协议，不能单边更改；
- 发布：严格按 `docs/UPDATE_RELEASE_PROCESS.md` 使用构建、线下签名和 fail-closed 发布脚本，不要手工拼装或替换已发布资产。

## 验证

```text
npm run typecheck
npm run build
npm run validate
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml --locked
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings
```

发布还需验证 NSIS、Portable、helper 的版本/身份、架构、多帧图标、隐私扫描、校验和、签名、清单/来源证明和启动。生成目录、依赖目录、数据库副本、本机缓存和线下密钥文件不属于源码，不能提交。

## 当前发布状态

- M²Shelf 新增 `list_hidden_nodes` Rust/IPC 查询及 `HiddenNodesDialog`；设置页打开跨库隐藏列表，复用 `reset_node_type` 恢复自动分类，并刷新当前页面、全部资源、最近观看和 Root 统计。加载使用请求序号防止旧响应覆盖恢复结果，包含键盘焦点管理及四语言文案。本功能源码尚未发布；前端回归覆盖路径筛选、恢复、错误重试、过期响应，Rust 数据层的嵌套隐藏、无视频条目、离线读取和元数据保留用例已通过本机原生验证。

- `v0.5.11` 已完成正式签名并作为公开稳定 Release 发布；正式集合仍严格为 Portable/NSIS 各自资产、SHA-256、Ed25519 签名边车，加 `latest.json` 和 `candidate-provenance.json` 共八项。README 已指向该公开版本；
- `0.5.11` 是首个携带当前 updater 信任根和 Portable helper 的引导版本。`0.5.7` 及更早用户和任何旧公钥测试包必须手动安装它一次；后续版本才能沿用内置更新链；
- `v0.5.8`、`v0.5.9`、`v0.5.10` 继续作为不可变但未发布的历史标记，不得移动、复用或视为更新授权；
- 当前 production seed 与 `src-tauri/update-public-key.txt` 自 `v0.5.11` 起长期使用。新增 USB 流程只改变私钥静态存储和人工发版入口，不改变已发布客户端的信任链。真实 DPAPI 到 `.m2key` 的迁移尚未由本轮执行，必须等待项目所有者再次明确确认；
- `.github/workflows/windows-release.yml` 继续只生成无签名候选和 attestation；后续发版用 `scripts/sign_update_from_usb.ps1` 生成签名返回包，再由既有 `scripts/publish_signed_release.ps1` 完成最终 helper 验签和公开发布；
- 任何后续 Agent 应以仓库和公开 Release 的实际状态更新本节，不得把未运行的迁移、签名、测试或发布写成已完成。
