# M²Shelf 设置收口与匹配弹窗清理

日期：2026-10-11。应用于当前 0.5.14 / schema 27 未提交工作树；版本未变。本轮是独立增量，未重做前一轮开发。ZIP 与单独 EXECUTE_IN_CODEX.md 内容逐字节一致。

## 完成内容

- 关于：移除 TMDb 渲染及占位，保留 M²Shelf 品牌、版本/构建日期/架构、更新、作者及感谢。
- 浏览与扫描：默认文案为“使用默认 TMDb 服务”，个人为“使用个人 API Read Access Token（推荐）”。状态靠近相应选项，只显示一次；连接失败可再次检查，操作失败可重试。原生凭证输入、删除确认、来源切换、优先级和后台匹配保持。
- TMDb 官方 Logo、必需英文声明及许可说明保存在该分组的“服务说明与署名” Credits 披露区，不在关于复制。核查依据：[TMDb 官方 FAQ / Attribution requirements](https://developer.themoviedb.org/docs/faq)。未修改官方 Logo。
- 播放器：按用户最新补充只保留“使用自定义播放器”和“系统默认播放器”；删除安装/PATH 自动搜索、自动检测选项和整段引导。新配置为 SYSTEM；旧 AUTO/BUNDLED 兼容到 SYSTEM，不删除路径；旧无模式的非空自定义路径保持 CUSTOM。
- 正常进设置不启动播放器。主动测试结果就近本地化显示，失败才提供可展开测试日志；输入、选择、测试等高，900/1100/1400 宽度适配。
- 所有手动匹配入口共用 BangumiModal，“识别依据”调试披露区、专用翻译和 UI IPC 字段已移除，原始名称/自动提取/其他候选及搜索选择保留。无该块专属 CSS 或展开 state 残留。底层解析、自动匹配保护、持久诊断与既有自动匹配记录保留。

## mpv 报错来源与处理

对截图相关外部 mpv 配置进行只读定位，确有 `screenshot-flags` 配置行；仓库没有为播放生成此参数。旧设置页自动调用版本探测，原生把 stdout 第一行当版本，导致外部配置报错被放到正常页头。现移除进页自动探测，只主动测试；版本只接受已知播放器前缀，stdout/stderr 各限 8192 字节、并发读取，仍为 5 秒超时。即使进程退出成功，出现配置解析错误也明确判测试失败。真实播放的字面路径参数及失败处理未改；没有修改用户外部配置。

## 变更文件

- `AGENTS.md`
- `docs/DECISIONS.md`
- `docs/PRODUCT_SPEC.md`
- `docs/PROJECT_CONTEXT.md`
- `scripts/validate_project.py`
- `src-tauri/src/commands.rs`
- `src-tauri/src/db.rs`
- `src-tauri/src/models.rs`
- `src-tauri/src/player.rs`
- `src-tauri/src/title_extractor.rs`
- `src-tauri/src/tmdb.rs`
- `src/components/BangumiModal.test.tsx`
- `src/components/BangumiModal.tsx`
- `src/components/TmdbCredentialSettings.tsx`
- `src/lib/nextReleaseMessages.ts`
- `src/pages/SettingsPage.tsx`
- `src/styles/workspace.css`
- `src/types/media.ts`
- `src/components/PlayerSettings.tsx`
- `src/components/ServiceSettings.test.tsx`

新增 PlayerSettings 是原设置播放器区的同源组件提取，继续使用唯一 SettingsStore、现有控件和 CSS tokens；没有新框架或全站改版。原生变动只在模式兼容、主动测试输出及删除 UI 专用 DTO，未改 scanner、schema、reader、搜索评分或绑定事务。

## 校验

| 命令/验证 | 最终结果 |
| --- | --- |
| npm run typecheck | 通过 |
| npm run build | 通过；既有大 chunk 提醒保留 |
| npm test | 173 项通过 |
| npm run validate | 135 项通过 |
| cargo fmt --manifest-path src-tauri/Cargo.toml -- --check | 通过 |
| cargo test --manifest-path src-tauri/Cargo.toml --locked | 340 项通过，13 项既有忽略 |
| cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings | 通过 |
| Tauri debug --no-bundle --locked，独立应用标识与 WebView/SQLite 数据 | 构建及启动通过；MSVC linker 的正常库创建消息被工具标为 warning，非编译失败 |
| 真实 Tauri 操作 | 27 项通过 |
| Edge 合成 IPC 布局矩阵 | 245 项通过，24 组：900/1100/1400 × light/dark × 四语言 |

真实 Tauri 验收使用 1280×800 窗口：关于/浏览扫描/播放器双主题、默认 TMDb 官方验证、来源切换、自定义 mpv 成功、在私有 MPV_HOME 合成过时选项后的失败与日志、系统默认、TMDb 电影和 Bangumi 动画真实搜索/封面解码/手动绑定、媒体 SHA-256 不变。电影使用中英/年份/编码杂讯合成目录；绑定到 TMDb #360605，动画到 Bangumi #253。没有进入私人资源库。

新增回归覆盖旧 AUTO/BUNDLED/缺省模式与路径保留、字面播放参数、stdout 误作版本、stderr 配置失败、有界输出、凭证状态/取消/网络失败/重试、StrictMode 过期初始响应、播放器迟到响应，以及两个 provider 无调试区且搜索绑定正常。

## 截图与证据

私有目录 `.tmp/settings-polish-20261011/screenshots/native/`（真实 Tauri）：

- `about-dark.png` / `about-light.png`
- `tmdb-dark.png` / `tmdb-light.png`
- `credits-dark.png` / `credits-light.png`
- `player-dark.png` / `player-light.png`
- `player-config-error.png` / `player-config-log.png`
- `TMDb-matching.png` / `Bangumi-matching.png`

对应附件：01 About 区块消失；02 推荐位于个人选项且状态不重复；03 裸错误消失且测试行对齐；04/05 共享弹窗调试区及间距删除，候选封面正常。

`.tmp/settings-polish-20261011/screenshots/fixture/` 是 Edge 的合成 IPC 矩阵及错误状态截图，不能当作原生验收。日志、检查 JSON、单轮 diff、417 文件 CRC 校验快照在同一私有目录。截图可能带测试机缓存路径，仅供本地复核，不提交公开仓库。

## 验证边界与保留项

- 真实个人 Token 的原生输入和删除确认没有实际保存/删除：该凭证目标由所有实例共享，为保护现有配置，仅用合成回调测试配置、取消和删除流程；原生验证了来源切换和默认服务。真实个人凭证未修改、删除或回显。
- 本轮原生预览未注入发行构建日期，因此关于页日期显示“—”；发行脚本的日期注入保留。
- 没有构建或发布新的安装/便携包。bundle 中已有 0.5.14 包保持原摘要，属于前一轮候选，不含本轮修改。当前交付为源码、前端构建、隔离原生预览和验收证据。
- 未修改 Skill、生产数据库、用户媒体或外部 mpv 配置；无提交、推送、签名、发布、覆盖安装。HEAD、未暂存状态、公钥及原 Logo 保持。


## 后续打包完成（2026-10-11）

此前包状态属于历史记录。按后续“打包给我”要求已生成包含最新设置修改的 0.5.14 安装包与 Portable；当前摘要与验收边界以 `PACKAGING_0.5.14_20261011_REPORT.md`、`delivery-summary.json` 为准。未签名、发布或覆盖安装。

| 文件 | 大小（字节） | SHA-256 |
|---|---:|---|
| `bundle/M2Shelf-Setup-0.5.14-x64.exe` | 10000240 | `0ccb8d56200e25dfb1850cf1d3e382e2559867fe59b03ef04bc9af4d0959aba2` |
| `bundle/M2Shelf-Portable-0.5.14-x64.zip` | 13263704 | `5676a8b68ba29acd6b3f9af21aaac12844043931cc07dfc9c5d6e001fda90fd0` |
