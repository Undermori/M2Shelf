# M²Shelf 电影自动 TMDb 与调试菜单 v4 交付记录

日期：2026-10-10。对象为本轮开始时的本地最新工作树，版本仍为 **0.5.12 / schema 26**。没有提交、推送、签名、发布或安装。

## 结果与原因

本轮完成未绑定真人电影的扫描/重新匹配兜底、共用片名解析、可查询的诊断记录，以及调试菜单中五个真实快捷操作。保留此前所有未提交工作。

以下原因由本轮开始前的代码快照确认，而不是根据截图猜测：

1. `auto_match_node_with_tmdb` 的初始快照和 `allow_tmdb` 都要求 `MatchWriteMode::IfAbsent`，使显式重新匹配的未绑定作品无法进入 TMDb。本轮移除此入口限制，但仍禁止普通自动流程替换任何已有绑定。
2. `tmdb::automatic_evidence` 在发请求前要求强年份，并通过一般标题安全检查排除纯数字标题。由文件推断的年份、无年份电影及 `1917` 等作品可能根本没有请求机会。现在搜索与自动绑定分层：允许安全的无年份/数字片名搜索；自动绑定仍要求强年份和其他可靠性条件。
3. 旧 TMDb 并非完全没有复用 Bangumi：它调用通用清洗器，再在 TMDb 内追加发布尾缀处理。但是两种识别模式没有共用、明确的电影片名来源选择，通用年份处理也不能可靠区分数字片名和发布年份。本轮将电影查询入口集中到 `title_extractor`，让真人电影 Bangumi 证据、TMDb 自动查询和人工默认预填共享它。
4. 旧界面缺少逐条可查的备用来源原因。未查询、缺凭证、认证失败、限流和没有结果容易被理解成同一种“未匹配”。现在设置中的 TMDb 区域可展开“自动匹配记录”，按需读取最后的清洗片名、年份和原因。

**无法据此判断用户此前的具体未匹配作品究竟是哪一种原因。** 本轮没有读取私人数据库、令牌或媒体库，也没有调用真实在线 TMDb/Bangumi；生产链路使用合成数据库与确定性提供方响应验证。

## 当前生产路径

```text
首次/手动重新扫描
  scanner::run_scan_with_auto_match
  → run_scan_with_matcher（相同扫描生命周期）
  → auto_match::run_auto_match / run_auto_match_using
  → run_match_nodes_with_tmdb

显式匹配现有资源/重新匹配
  scanner 现有任务入口 → auto_match::run_match_nodes
  → run_match_nodes_with_tmdb

共同执行
  auto_match_node_with_tmdb
  → movie_evidence_for_node（仅 LIVE_ACTION）
  → 既有 Bangumi 候选判断
  → 没有可靠绑定/请求失败时 tmdb_fallback
  → AutomaticRun::fallback → NativeAutomaticProvider
  → 原 movie 搜索、详情、事务绑定及应用封面缓存
```

`run_scan_with_matcher` / `run_auto_match_using` 只提供依赖注入边界，测试替换提供方，使用相同生产扫描体和匹配体。没有另写“测试专用扫描器”。后台启动增量检查继续跳过未改变目录；手动重扫和显式匹配提供旧未匹配条目的重试机会，不在每次打开详情时联网。

### 共用解析与请求

可核查入口：

| 文件 / 符号 | 位置 | 行为 |
|---|---:|---|
| `src-tauri/src/title_extractor.rs` / `movie_query_title` | 82 | 复用原分隔符、括号、技术标记清洗；从后向前识别发布年份，保留数字片名 |
| 同文件 / `build_movie_match_evidence` | 118 | 人工自定义名优先；FOLDER 取具体作品目录，VIDEO_FILE 取具体视频名；泛用名回退；年份一致性与有界别名 |
| 同文件 / `movie_evidence_for_node` | 209 | 泛用文件名可取索引源路径中具体作品目录，排除库根；不重新枚举磁盘 |
| `src-tauri/src/auto_match.rs` / `auto_match_node_with_tmdb` | 420 | 当前数据快照、策略、已有绑定、Bangumi 优先及 TMDb 兜底 |
| `src-tauri/src/tmdb.rs` / `movie_search_arguments` | 352 | 生产 HTTP 和模拟 HTTP 共用最终参数构造 |
| 同文件 / `automatic_evidence` | 670 | LIVE_ACTION、单视频 WORK、无集数/季数等电影证据 |
| 同文件 / `reliable_movie` | 717 | 精确自身片名/别名、可靠年份、唯一候选、类型及详情复核 |
| 同文件 / `AutomaticRun::fallback` | 811 | 原限流、预算、取消、过期快照和封面绑定路径 |
| `src-tauri/src/commands.rs` / `tmdb_match_diagnostics` | 88 | 类型化诊断命令；前端无 SQL/凭证访问 |

最终请求将 `query` 与可靠的 `primary_release_year` 分开。首次没有结果时，最多尝试三个现有候选片名，每个计入原每轮 128 次搜索预算；遇到候选歧义、年份/类型/详情冲突，不通过不断换词挑一张海报。缺年可以请求，但不会自动绑定。未知发布标签不会被一刀切删除。

手动搜索仅默认预填使用共用解析。用户编辑后的关键词直接进入现有 TMDb API，不恢复原发布文件名。动画及书籍继续走原证据构造；没有修改 SMART_MIXED、逻辑归属、解析器、识别模式或迁移。

### 诊断与保护

`tmdb_auto_diagnostics` 是原 settings 表中的独立有界键，最多 128 条 / 64 KiB，独立于 AppSettings 快照。保存 Node ID、最多 200 字符的清洗查询、年份、固定结果码及时间，不保存令牌、请求 URL、完整路径或原始错误内容。

可区分：未配置凭证、401、429、请求失败、没有结果、缺可靠年份、候选/详情不确定、预算延后、不符合电影条件、处理中、绑定成功和绑定成功但封面失败。诊断面板只在展开或刷新时读取，不增加设置初次加载的请求。

保留原 Root 自动策略/全局开关、现有任一提供方绑定（包括停用的人工 TMDb 选择）、人工封面、取消与过期结果拒绝、provider breaker、应用缓存安全边界。Bangumi 成功时不查 TMDb；备用来源失败不清除已有数据。是否具备真实可用凭证仍需用户在当前应用的设置中确认。

## 调试菜单

`WindowTitlebar.tsx` 的 Debug 保留“重新加载 F5”，新增五个真正可点击的操作：

| 操作 | 现有键位 | 复用动作与状态 |
|---|---|---|
| 返回 | Alt + ← | App 当前返回动作；无可返回目的地禁用 |
| 上一页 | PgUp | 当前 comic `turn(-1)` / document `step(-1)`；按边界禁用 |
| 下一页 | PgDn | 当前 comic `turn(1)` / document `step(1)`；按边界禁用 |
| 全屏 | F | 当前阅读器原全屏回调；非阅读器禁用 |
| 添加/移除书签 | B | 原书签回调；内容未就绪/写入中禁用 |

`ReaderMenuActions` 通过 App 注册当前阅读器回调，退出时注销；没有伪造键盘事件或复制另一套翻页算法。文字和键位使用 grid 对齐，中性 hover、可读 disabled、键盘焦点和关闭后焦点恢复保留。打开菜单时阅读器不处理菜单内部导航按键，避免选菜单却翻页；原阅读器按键不变。

**F5 的准确边界：** 用户此前明确要求“不拦截”，基线 F5 是 WebView 原生重载，而菜单和 Ctrl+R 才经过 App 的保存/禁用回调。本包“保留受保护 F5”的描述与真实基线不同。本轮保留 F5 和原 Ctrl+R，不新加原生拦截；不能宣称 F5 具备菜单的未保存/阅读器保护。菜单重载继续保存设置，并在阅读器、模态、变更/未保存状态下禁用。此差异已记录于 D60，未暗中改变之前的按键决定。

## 验证结果

| 检查 | 本轮结果 |
|---|---|
| TypeScript / `npm run build` | 通过（包含 typecheck；Vite 原大 chunk 提示仍存在） |
| `npm test` | 26 文件，160 通过 |
| `npm run validate` | 135 通过 |
| `cargo fmt -- --check` | 通过 |
| `cargo test --locked` | 318 通过，13 项既有忽略 |
| 全目标 `cargo clippy --locked -- -D warnings` | 通过 |
| Windows release / NSIS / Portable | 标准脚本构建通过 |
| 本轮任务差异与保留检查 | 见 `docs/ui-audit/movie-debug-v4-20261010/preservation.json` |

### 生产扫描集成与失败场景

`src-tauri/src/auto_match/movie_pipeline_tests.rs` 的 `movie_production_import_rescan_rematch_final_http_arguments_and_persistent_cover` 覆盖 **12 样本 × 两识别模式 × 首扫/重扫/显式重匹配 = 72 组**最终本地 HTTP 请求参数核对。请求实际经过共用 HTTP 参数构造及请求方法；模拟服务接收并解析 query string。不是只断言解析函数返回值。

八个必需样本全部覆盖：Superbad、Ernest And Celestine、1917、Se7en、2001 A Space Odyssey、Big Hero 6、Movies 下具体文件、具体电影目录下 film.mkv；另含中文/发布组前缀、日文、无年份与 2046。自动候选不是靠先后排名绑定；数字正确并不意味着动画片可以绑定真人电影。另测括号别名、源名优先级、年份冲突及原动画季数查询对照。

封面通过现有图像验证、标记应用缓存与原子写入，重开数据库仍有效；保留绑定后再次匹配不会继续请求；合成源文件字节未变。旧未绑定场景通过只清除合成绑定后重扫/重新匹配验证。其他生产 runner 回归覆盖 Bangumi 成功优先、无结果/暂时失败、同名歧义/错年/错类型、缺凭证/401/429/超时、关闭策略、人工绑定/封面、取消、过期结果与运行预算。人工改词由 `BangumiModal.test.tsx` 核对最终 API 参数。

### 浏览器与真实 Windows 窗口

证据目录：[`docs/ui-audit/movie-debug-v4-20261010/`](docs/ui-audit/movie-debug-v4-20261010/)。可打开 [`index.html`](docs/ui-audit/movie-debug-v4-20261010/index.html) 查看截图。

- 浏览器使用当前完整组件与合成 IPC，83 项交互/布局检查，四语言、light/dark、900×640 / 1280×800 / 1600×1000；无记录到的 JS 错误。普通页禁用项、漫画 RTL/双页的菜单及键位同向、菜单内键盘导航/关闭/返回/全屏/书签均验证。
- 另测 TXT / EPUB / MOBI / AZW3 / PDF 当前 DocumentReader：菜单翻页与 PgUp/PgDn 一致，书签鼠标/按键均生效；系统主题运行中切换及设置诊断原因可显示。文档内容来自合成 IPC，不将其冒充真实文件解码验证。
- 原生 WebView2 用独立应用标识及隔离数据库，真实 App 入口（没有覆盖 Tauri internals）、真实扫描索引和六页合成 PNG 漫画。实际菜单下一页 1→3、PgDn 3→5、PgUp 5→3、菜单上一页 3→1；B 添加/菜单移除书签；菜单全屏/F 退出；菜单返回详情/Alt+← 返回浏览。重载计数从 **1→2（菜单）→3（F5）→4（Ctrl+R）**，最终恢复正常页面。原生测试窗口已关闭。
- 原生普通页和阅读器菜单截图分别为 `native-normal-debug.jpg`、`native-reader-debug.png`；状态结果见 `native-results.json`。原生为隔离 debug 构建；候选 release 主程序没有用私人默认数据库启动，安装器未运行。

没有将浏览器夹具、模拟 HTTP、原生 debug 窗口或未签名打包等同于真实在线 TMDb / 私人库 / 已安装版本验收。

## 本地交付与保留范围

安全快照：`.tmp/movie-autotmdb-v4-20261010/snapshot/worktree-before.zip`，644 个基线文件和 SHA-256 清单；起始分支 `ui/global-visual-refresh`，HEAD `f7f5c1e44e991aa7c7f0cd1b0f460921a18e15b6`。本轮 task-only diff、保留结果位于同一任务临时目录。根目录 Git diff 含早前未提交工作及既有空白问题，不能作为本轮新引入差异；未为“整理格式”覆盖这些旧修改。

除电影匹配/诊断、标题栏回调连接、相应测试和三份持久文档外没有改其他基线文件；没有改 AGENTS、Skills、schema、依赖、公钥或 Logo。媒体只用本轮生成的合成夹具，私人数据库/媒体/凭证未读取或写入。原 `bundle/` 根目录四份候选/摘要文件构建后恢复到本轮开始字节；新候选独立放置。

最新候选：`bundle/local-rc-movie-autotmdb-v4-20261010/`。

| 文件 | 字节 | SHA-256 |
|---|---:|---|
| `M2Shelf-Portable-0.5.12-x64.zip` | 13,113,961 | `16ab734250735ad355cd26e397f95a95d9d53b3ad485167582ea8b7f1de386a4` |
| `M2Shelf-Setup-0.5.12-x64.exe` | 9,885,849 | `65b654f2155f4e6d27c461faeb5590dec4b74ce12c11e751a0d1d114dc53c182` |

标准 Windows 构建脚本和 Portable 脚本执行成功。核验八文件精确 allowlist、七个 payload 摘要、release 二进制一致、x64 PE、主程序/Updater 版本及 helper 身份/公钥、包含 worker 源码包的路径隐私扫描。摘要记录见证据中的 `artifact-verification.json`。未制作新签名授权，不可用于正式自动更新；当前候选仍使用正常应用数据目录，隔离验收配置不是其发布配置。

未确认：真实 TMDb 当前认证及网络、用户具体电影候选命中情况、用户私人库的最终匹配结果。没有为了填写“通过”而访问私人数据或在线服务。
