# M²Shelf 匹配、设置与自动 TMDb 交付记录

## 当前结果与范围

基于任务开始时的最新本地工作树完成五项定向修改，保留前面书籍、SMART_MIXED、海报和阅读器修改。版本仍为 **0.5.12 / schema 26**。候选在 `bundle/local-rc-polish-autotmdb-20261010/`；未安装、签名、发布、提交或推送，没有访问用户正式数据库或媒体源。

用户后续明确撤回包内“阻止 F5”要求：**保留原生 F5，Debug 重载标注 F5，将其他已有快捷键说明放进 Debug**。本轮按此最新要求执行。

## 五项实际修改

| 项目 | 根因与最终修改 | 主要位置 |
| --- | --- | --- |
| 匹配结果悬停边界 | 旧结果行底描边与不同按钮状态叠加，造成生硬底线。删除该规则，统一 116px 最小高度、12px padding、16px gap、60px 封面、8px 圆角与透明边框，中性 hover。标题省略、按钮不被挤动。 | `src/styles/workspace.css`；`src/components/MatchingResults.tsx` |
| 整行点击及状态 | 原独立选择按钮仅自身可点。保留一个真实按钮及原回调，通过伪元素扩展至结果行；按钮设 static，按下时取消旧 brightness filter，防止滤镜改变包含块、命中范围缩回按钮而丢失点击。增加已绑定、绑定中、禁用与 ARIA 状态；键盘焦点保留。 | `MatchingResults` / `BangumiModal`；`.match-choice` / `.is-bound` / `.is-binding` |
| 重载与快捷键 | F5 来自原生 WebView 默认行为。没有添加拦截、新 native hook、依赖或权限。Debug 标注 F5，显示返回和阅读器现有按键。原 Ctrl+R 与菜单回调的设置保存/禁用保护保留。 | `src/components/WindowTitlebar.tsx`；`src/lib/phase3Messages.ts` |
| 真人电影 TMDb 备用匹配 | 原自动流程只有 Bangumi，提供方请求失败会结束后续自动检查。增加严格 LIVE_ACTION 单电影备用路径；Bangumi 故障后停止继续请求 Bangumi，但允许后续合格电影检查 TMDb。原手动 provider 选择、官方 API、Credential Manager 和缓存继续复用。 | `src-tauri/src/auto_match.rs`；`src-tauri/src/tmdb.rs`；`src-tauri/src/db.rs` |
| 设置精修 | 删除主标题上方的小品牌字；维护操作统一轻边框、圆角、内边距、动作容器和按钮高度，修正路径组合控件层级。不改选项、保存、缓存统计、清理或重建行为。 | `src/pages/SettingsPage.tsx`；`src/styles/workspace.css` |

四语言 Debug 文案在现有 typed resources 内；没有新增快捷键注册。已有阅读器按键包含方向键、PageUp/PageDown、Space、Home/End、F、B、Esc；说明明确为阅读器作用域。未宣称 F11 是应用现有全屏快捷键。

### 自动备用来源的边界

调用关系：`run_match_nodes_with_tmdb` → `auto_match_node_with_tmdb` → 既有 Bangumi 路径 → `tmdb_fallback` → `tmdb::AutomaticRun::fallback` → 原生 search / detail / cover。

- 只允许 LIVE_ACTION 的 Work/AUTO_WORK，拥有一个视频及一条归属文件证据；两识别模式 FOLDER / VIDEO_FILE 都覆盖。隐藏 Root、系列、Mixed、补充内容和其他媒体类型不增加自动 TMDb。
- 全局匹配与 Root 自动关联必须都已开启。没有凭证时不查询、不弹配置，不更改开关。
- Bangumi 合格绑定优先，成功后不请求 TMDb。正常无候选、无安全结果、请求故障在分支和日志中分别记录；请求故障计入错误，不能报成正常空结果。
- 强年份限定 1900–2200；清理自身标题/文件名中的发布标签，归一化后与候选标题或原名精确相等。年份一致、唯一 ID、非空官方 genres 且不含动画 16 / 纪录片 99 / TV movie 10770，再请求官方详情重新核验。首搜索页已满 20 条时不自动选择，避免隐藏同名候选。
- 含季、集、纪录片或不兼容版本信号的来源拒绝。没有年份、证据弱、类型不明、模糊近似或多个可靠候选均不自动绑定，人工搜索仍可用。
- 单轮最多 128 次备用搜索；凭证/请求错误使 TMDb 在本轮停止后续请求。复用原客户端 TLS、超时、JSON 大小、代理、官方 host 和全局并发边界。日志仅固定阶段码，不含 token 或传输 URL。
- 已有 Bangumi 或任意 TMDb 行（含停用记录）不重新绑定。人工封面优先，允许安全元数据绑定但不下载替换该封面。
- 起始节点状态、Root revision 和归属文件快照校验；归属索引缓存记录其 revision，失效则重读，不把旧文件证据与新节点混用。网络后及 IMMEDIATE 写事务内再验证指纹、取消、代际；过期结果不写回，未引用下载只清理应用缓存。
- Bangumi 的 `save_binding_if_absent` 也排除已存在 TMDb 行，防止两提供方异步竞态覆盖。

没有更改非电影匹配评分、SMART_MIXED、扫描归属、阅读器、海报缩略图管道、更新器、数据库迁移或密钥。

## 验收结果

| 验收 | 结果 / 证据 |
| --- | --- |
| TypeScript、前端单测、生产前端构建 | `npm run check`：25 文件 / **156 tests passed**；含 typecheck 和 build。`.tmp/polish-autotmdb-20261010/frontend-full.log` |
| 项目 validate | **135 passed, 0 failed**。同一日志。校验器仅对明确命名的按钮 loading spinner keyframes 排除海报禁止 transform 检查，其余规则保持。 |
| Rust 全量测试 | **316 passed, 0 failed, 13 ignored**；忽略项保持既有范围。`rust-tests.log` |
| 最终备用来源分支日志 | 六组测试全通过，包含两识别模式、Bangumi 命中/空/请求故障、TMDb 歧义/年份/类型/凭证/HTTP 错误、人工封面/绑定、取消/过期、Root 变化和非电影排除。`automatic-tests-final.log`；初次失败日志留作过程证据，以 final 和完整测试为准。 |
| HTTP fixture | 实际本地 HTTP 响应测试覆盖 TMDb 响应类型和错误/有界读取。不是在线 TMDb 请求。见 `tmdb::tests` 与 Rust 全量日志。 |
| 格式与静态检查 | `cargo fmt --check` 和 `cargo clippy --all-targets --locked -- -D warnings` 通过。`clippy.log` |
| UI 浏览器矩阵 | 真实当前组件 + 合成 IPC，四语言 × 深浅色 × 900×640 / 1280×800 / 1920×1080。**249 检查通过，0 页面脚本错误**；包含整行点击只提交一次、hover 几何不变、可见键盘焦点、绑定/禁用、菜单边界、Settings、系统主题 Light→Dark。`ui-evidence/matrix-results.json` |
| 前后对照 | 从任务前源码快照恢复合成基线，保存 8 张 before / 8 张 after；不是设计稿。`ui-evidence/before-*.png` / `after-*.png` |
| 原生 Windows WebView2 | 运行完整当前 App 和真实 IPC，独立 audit 应用标识、空数据库；实际 F5、Ctrl+R 和菜单 Reload 将文档计数 **1→2→3→4**。Debug 显示 F5 和其他按键。见 `native-before-F5` / `native-after-F5` / `native-after-CtrlR` / `native-debug` / `native-after-menu-reload` 截图与可访问树。 |
| Windows 构建 | 标准 `build_windows_release.ps1 -Bundles nsis` 和 `build_portable.ps1 -SkipBuild` 成功；主程序、Updater 版本 0.5.12；8 文件载荷、7 项内部摘要、x64 PE、helper identity、公钥一致及构建路径隐私扫描通过。`artifact-verification.json` / `windows-build.log` / `portable-build.log` |

生产 build 有既有大 chunk 提示；Windows linker 输出生成 import library 的信息性 warning。严格 Clippy 已通过，没有遗留编译错误。

### 白屏验收窗口的原因与处置

早期原生测试夹具覆盖 `window.__TAURI_INTERNALS__` 等只读原生对象，导致测试窗口白屏；不是通过生产程序修补绕过。已弃用该原生 mock 入口，原生验收改为直接导入正常 `src/main`、真实 Rust commands，使用独立 app identifier 与空数据库。完整界面和重载均正常。正式 `main`、Tauri 配置、权限及应用数据库没有为验收被改动。隔离窗口验完已关闭。

浏览器矩阵曾因夹具在 App 的主题 layout effect 后强制写 `data-theme=system` 失败，修正夹具并使 Vite 使用新模块后，真实 App 的系统主题解析和实时切换通过；没有为此修改生产主题代码。

### 未实测范围

- 未访问用户私人索引、媒体文件或生产凭证；未做真实在线 Bangumi/TMDb 匹配。
- 原生快捷键验收是当前代码的隔离 debug WebView2；最终 release Portable 主程序没有使用默认应用数据目录启动，未执行安装包或更新替换。
- F5 按用户要求保留原生默认行为，**不经过**受保护 Ctrl+R/菜单回调；不能将这些回调的防护声明用于 F5。

## 本地候选

目录：`bundle/local-rc-polish-autotmdb-20261010/`，含安装包、Portable ZIP、展开载荷、README 与 SHA256SUMS。只是本地 unsigned RC，不是签名更新。

| 文件 | 字节 | SHA-256 |
| --- | ---: | --- |
| `M2Shelf-Setup-0.5.12-x64.exe` | 9,897,404 | `2886d9b6cf33a4fcd2fd901563de29f7175e5fefdb652d693b63c1987a9a0a52` |
| `M2Shelf-Portable-0.5.12-x64.zip` | 13,099,783 | `e7324c90bd310bc2312a5b40df5c2ab58c86ae174164045b1e45a49004c38ecc` |

Portable 精确载荷：M2Shelf.exe、M2ShelfUpdater.exe、M2ShelfMobi.exe、M2ShelfMobi-source.zip、THIRD-PARTY-NOTICES.txt、M2Shelf.portable.json、README_zh-CN.txt、SHA256SUMS.txt。无新增字体、密钥或私人路径。

## 文件与保护证据

本轮软件变更为三个 Rust 文件、六个 UI/文案/样式文件及 `scripts/validate_project.py`；新增 `MatchingResults.test.tsx` / `WindowTitlebar.test.tsx`。同步 PRODUCT_SPEC / PROJECT_CONTEXT / DECISIONS（D59）。

任务前快照 `.tmp/polish-autotmdb-20261010/snapshot/worktree-before.zip` 保存 641 文件及 SHA-256 清单；差异和保护报告见 `preservation.json` / `task-only.diff`。AGENTS、Skill、Cargo/package 依赖、迁移、图标源/ICO、生产 app 配置、reader、scanner、updater、生产公钥均与任务开始一致。旧 bundle 根部四个候选文件及摘要已从本轮备份逐字节还原，新的候选只在独立目录保留。

分支保持 `ui/global-visual-refresh`，HEAD 保持 `f7f5c1e44e991aa7c7f0cd1b0f460921a18e15b6`。用户原有未提交内容没有 reset/checkout/清理。没有改动系统级或用户媒体配置。

截图索引：`.tmp/polish-autotmdb-20261010/ui-evidence/index.html`。截图仅合成数据或隔离空库，未提交到 Git。
