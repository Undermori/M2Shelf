# M²Shelf 0.5.14 本地候选交付

公开基线为 0.5.13；当前工作树为 0.5.14 / schema 27。本轮仅本地 unsigned 构建，无提交、推送、签名、发布或覆盖安装。任务开始前 403 文件快照已校验，可恢复未提交源代码和项目 Skills。

## 本轮结果

- 播放器按用户最终修正取消内置。设置提供自动检测本机 mpv、手工选择任意兼容播放器、系统默认播放器；旧自定义路径保留，失效不静默改用别的程序。自动检测仅检查已知安装位置和绝对 PATH 项，不从当前目录寻找。没有 mpv 时使用 Windows 默认关联；没有视频关联时明确报错并引导配置。所有视频入口共用 Rust resolver；自定义进程仅接收一个字面媒体路径，成功启动才记录历史。
- 默认 TMDb API Read Access Token 仅由授权的私有文件在 Rust 构建时注入；发行包不附文本凭证。默认/个人切换、独立验证、原生确认删除、旧个人配置优先和过期请求拒绝共用一个服务。API 输入不返回 React，不写日志或项目文档。署名和 TMDb 官方英文声明放在 About。
- 所有漫画、PDF、EPUB、TXT、MOBI、AZW3 使用共享 `ReaderShell` header/footer，上一页在左、状态居中、下一页/目录/书签在右。点击状态打开跳转/书签；无目录时同位置禁用。保留不同格式的渲染、RTL、双页、连续模式、稳定文字锚点、版本验证及有界缓存。
- 智能与普通目录复用普通右键菜单，移除实验性归类项。后端人工组织覆盖继续保留。虚拟分类/系列只显示安全的导航动作，不伪造 Node ID 或磁盘路径。
- 统一刮削、导入缓存及 Movie/TV 命名空间见 `SCRAPER_UPGRADE_REPORT.md`。

## mpv 分发决策

[mpv 官方安装页](https://mpv.io/installation/)说明 Windows 第三方构建及官方 CI 构建的定位。[v0.41.0 release](https://github.com/mpv-player/mpv/releases/tag/v0.41.0)与 [shinchiro 构建项目](https://github.com/shinchiro/mpv-winbuild-cmake)已检查。本机审计了官方 MinGW/MSVC 和推荐的稳定静态包：没有获得足以对应所选二进制的完整依赖许可证、精确源码 revision/补丁及可履行的相应源码材料。静态包还显示 libplacebo dirty revision；不能仅凭 mpv.exe 可播放就声称分发验收通过。

按用户最终要求停止内置，不自行编译，也不建立新的许可证构建流水线。试验性 staging 脚本及十二文件载荷方案已撤回；正式安装/Portable 不含 mpv、其 DLL 或未经审计的源码包。保留原八文件 Portable 契约及既有 LGPL worker 材料。因此本次没有「干净系统开箱内置 mpv」声明；没有 mpv 的路径是系统默认播放器，需要 Windows 已有可用视频关联。

## 凭证与许可

默认 TMDb 凭证已经在独立 Python 请求和实际 Rust 原生 `/authentication` 请求中验证成功；原生 TV 搜索验证了官方结果及 `TV` 标记。客户端编译凭证仍可从 EXE 提取、被滥用或撤销，不是保密方案。实际凭证不进入 Git、日志、截图、测试 JSON 或文本载荷。公开商业发行与凭证授权需要遵守 TMDb 条款；本轮在线可用性不能替代其许可确认。个人凭证保存在 Windows Credential Manager，切回默认不删除它；本轮不写入或删除真实个人凭证。

## 验收和制品

完成后的精确测试计数、路径、大小、SHA-256 由本地 `delivery-summary.json` 汇总。重要验收边界：浏览器 fixture 使用合成 IPC；native 使用真实 Tauri/WebView、SQLite、扫描及文件读取；两者分别记录。native 数据与文件均由本轮创建，媒体 SHA-256 前后比较，不涉及私人库。

截图与原始记录：`.tmp/next-release-20261011/verification/`。包含旧源码快照渲染的 before、四语言/深浅主题/900×640、1280×800、1920×1080 的共享阅读器、设置、菜单，以及实际原生 PDF/EPUB/TXT/Kindle/漫画界面。截图为本地验收材料，未提交公共仓库。

尚不能确认：独立干净 Windows VM、声卡/显卡及任意系统默认播放器的视频/字幕兼容性；真实个人凭证的新建/删除交互（避免改变已有配置）；真实私人库全量识别率。本轮使用实际外部 mpv 验证配置及视频调用，不能把它等同于 Windows VM 上的默认播放器验收。

两个最终包位于 `bundle/`，保持必要的安装 EXE 和 Portable ZIP；内部保留 updater、marker、README、checksums、Kindle worker、完整对应源码和 NOTICE。没有生成签名 manifest 或 latest.json，不能通过生产自动更新器安装这个 unsigned 候选。


## 需求对应与主要改动文件

| 要求 | 当前落点 | 验证及保留行为 |
|---|---|---|
| 本机播放器检测/自定义/系统打开 | `player.rs::detect_mpv/resolve/play_selected`、`db.rs`、`SettingsPage.tsx` | 旧路径保持，路径为单独字面参数，成功调用才记录历史；无内置选项 |
| 默认/个人 TMDb | `native/build_tmdb.rs`、`tmdb_credentials.rs`、`commands.rs`、`TmdbCredentialSettings.tsx` | 私有本地构建输入；原生凭证库；source generation、连接状态、原生确认删除；真实默认认证成功 |
| 统一阅读器 | `ReaderShell.tsx`、`ComicReaderPage.tsx`、`DocumentReaderPage.tsx`、`workspace.css` | 共享顶部、固定 56px 底部、按需位置 popup；保留引擎、RTL、双页、虚拟化、缩放与稳定定位 |
| 普通/智能右键 | `ContextMenu.tsx`、`BookContextMenu.tsx`、`SmartMixedBrowser.tsx` | 真实 Node 复用普通菜单；真实书籍复用书籍菜单；虚拟分类只导航，原后台覆盖仍保留 |
| 统一解析和元数据 | `tools/media-parser`、`media_scraper.rs`、`scraper_metadata.rs`、`scanner.rs`、migration 27 | 同一结构解析及安全扫描导入，手工值优先；SQLite 索引证据、原 ID 与修订、取消/失败边界保持 |
| 组织/匹配集成 | `auto_match.rs`、`title_extractor.rs`、`smart_mixed.rs`、lab `signals.rs/recognize.rs` | 首次/增量/显式重匹配/手动预填使用共同证据；作者目录不会合并成一本书 |
| TV 与错误类别 | `tmdb.rs`、`logical_works.rs`、`works.rs`、`TmdbDiagnostics.tsx`、`BangumiModal.tsx` | Movie/TV namespace、季集/日期、原语言海报；无结果/低可信度/多候选/认证/限流/网络分别说明；无效 ID 回退 |
| 发布链与持久文档 | release/portable scripts、Cargo/package/Tauri versions、AGENTS 和三份 durable docs | 0.5.14、schema 27，八文件契约、公钥、签名边界保持；未发布候选 |

## 已执行的门禁与运行边界

| 检查 | 结果 | 记录 |
|---|---|---|
| `npm run typecheck` | 通过 | `typecheck-final.log` |
| `npm run build` | 最终发行脚本实际执行并通过 | `release-build.log` |
| `npm test -- --run` | 28 文件、166 测试通过 | `vitest-final.log` |
| `npm run validate` | 135 通过，0 失败 | `validate.log` |
| `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` | 通过 | 最终门禁 |
| `cargo test --manifest-path src-tauri/Cargo.toml --locked` | 338 通过、13 默认忽略、0 失败 | `rust-final.log` |
| `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings` | 通过 | `clippy.log` |
| parser 独立 `cargo test --manifest-path tools/media-parser/Cargo.toml --locked` | 7 通过 | `parser-final.log` |
| lab `cargo test --manifest-path tools/smart-mixed-lab/Cargo.toml --locked` | 35 核心 + 49 结构测试通过 | `lab.log` |
| Shadow `cargo test --manifest-path tools/smart-mixed-shadow/Cargo.toml --features fixtures,production --locked` | 1 单元 + 41 适配器测试通过 | `shadow.log` |
| 显式合成规模性能测试 | 100/1000/5000/10000 项；分组 59/279/1443/3542ms，目录 7/17/68/123ms | `performance.log` |
| Chrome fixture | 92 检查，0 运行错误；四语言、深浅/系统、三尺寸，格式外壳和菜单 | `verification/ui-results.json` |
| 实际 Tauri/WebView | 48 检查，0 前端运行错误；7 书索引、真实二进制读取、两次重扫进度/书签、五类文档及漫画可见 | `native-results.json` |
| TMDb 在线 | 默认认证和官方 TV 搜索成功；token 未输出 | `tmdb-default-verify.json`、native 结果 |
| 外部播放器 | 未检测到 mpv 时解析为 SYSTEM；私有测试外部 mpv 验证成功并实际启动 AVI、记录历史 | native 结果 |
| 只读/保护 | 10 个本轮格式测试文件的完整路径集合及 SHA-256 前后相同；Skill、logo source、公钥与快照比较 | artifact 与 final safety 记录 |

所有原始日志位于本地 `.tmp/next-release-20261011/`。性能时间是这台机器上的合成索引，不是私人库或所有硬件的承诺。

13 个默认忽略项涉及外部 Bangumi、指定用户文件/读失败状态、真实 Kindle 样本、海报基准和合成性能。合成性能已另行显式运行通过；实际合法 Kindle/PDF/EPUB/CBZ/PNG 通过独立原生流程覆盖。没有声称其余忽略函数全部执行，也没有为跑测试读取私人资料。

原生窗口使用同一最终源码、独立 application ID / SQLite / WebView profile；发行包使用正式 ID。正式 Setup 未执行安装，正式 EXE 未以生产 profile 启动。因此“隔离原生验证 + 制品静态校验”不能描述成安装后或干净 VM 的完整验收。测试程序不对真实 Windows Credential Manager 的已有个人令牌做新增/删除，个人原生交互仍需作者实机确认。


## 最终本地文件

| 文件 | 字节数 | SHA-256 |
|---|---:|---|
| `bundle/M2Shelf-Setup-0.5.14-x64.exe` | 10020962 | `19cc80b32dbc38db4d31f4bf680e86beb1d17ac3863a56ed35eb8799a9e312e1` |
| `bundle/M2Shelf-Portable-0.5.14-x64.zip` | 13266082 | `0d95e432bb831869dc1505d2e06034a74bd5ede5b5d1259b581cad39a20a989b` |

包内 7 项内容摘要、三份 EXE 的 x64 标头、正式 updater identity/版本/公钥、与最终 release 二进制的一致性、构建者路径及源码包隐私检查均通过。默认凭证仅存在主 EXE 的编译常量中，未进入其他载荷、worker 源码、414+ 个公开工作树文件或验收日志。最终静态检查见 artifact-verification；没有生产签名或安装验收的隐含声明。


## 后续打包完成（2026-10-11）

此前包状态属于历史记录。按后续“打包给我”要求已生成包含最新设置修改的 0.5.14 安装包与 Portable；当前摘要与验收边界以 `PACKAGING_0.5.14_20261011_REPORT.md`、`delivery-summary.json` 为准。未签名、发布或覆盖安装。

| 文件 | 大小（字节） | SHA-256 |
|---|---:|---|
| `bundle/M2Shelf-Setup-0.5.14-x64.exe` | 10000240 | `0ccb8d56200e25dfb1850cf1d3e382e2559867fe59b03ef04bc9af4d0959aba2` |
| `bundle/M2Shelf-Portable-0.5.14-x64.zip` | 13263704 | `5676a8b68ba29acd6b3f9af21aaac12844043931cc07dfc9c5d6e001fda90fd0` |
