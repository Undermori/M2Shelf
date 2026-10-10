# M²Shelf 2026-10-09 定向更新交付

## 最新补充：删除资源库主页海报区重复标题

用户指定删除截图中“下级资源 / 继续向下浏览 / N 个节点”整行。仅修改 `BrowsePage` 的条件渲染：Root 主页不渲染该 section heading，海报随原布局上移；顶部项目数量、进入下级目录后的分区标题、核心文件/书籍/附件和所有操作保持。没有 CSS、i18n、图片或业务逻辑改动。同步本报告、产品规格、上下文和 D53。

修改前 316 文件快照位于 `.tmp/library-refresh-20261009/20261009-102029/`，源码 ZIP SHA-256 `f46045b4bc89f809e2d0c42334bedc21bff38de6c6a5330022b9bd179fc1b20f`。最终检查：117 前端测试、278 Rust 测试（10 默认忽略）、135 项目校验、rustfmt、严格 Clippy 通过。初次前端跑测的一个已有新增库异步查找超时，未修改测试，完整重跑通过；浏览器首次冷启动超时，预热后 231 项实际组件夹具检查通过。四语言/两主题/三窗口以及六类别/两识别模式均检查主页标题消失、子目录标题保留、计数及原导航/筛选/操作范围。私有截图和日志在 `.tmp/library-header-cleanup-20261009/`，不是原生 WebView 全页面验收。

标准 Windows 编译和原 Portable 打包脚本通过，版本保持 0.5.11。新包 `bundle/local-test-20261009-102624/M2Shelf-Portable-0.5.11-x64.zip`，SHA-256 `AA147D54B171777632B8AFD66D14DFC6E9C52178571391795B20688901BE684D`。当前本地测试程序目录的五个文件已先备份到 `.tmp/library-header-cleanup-20261009/active-package-backup-20261009-102628/`，旧窗口正常关闭后逐项替换及摘要核对，重新启动并确认 M²Shelf 窗口响应。没有直接编辑数据库、媒体、旧 Skill、Logo、公钥或正式发布包，无签名、发布、提交或推送。

## 结果与范围

直接修改执行时的最新本地应用代码。保留既有海报网格和缓存机制，调整海报 footer、全部资源页头及集成窗口栏，并按后续补充同步各资源库页头；没有将此前任意整套原型移植到应用。

## 后续补充：资源库同步与最新程序

`BrowsePage` 和 `AllResourcesPage` 共用 `collection-toolbar`、`collection-heading`、`collection-title`、`collection-count`、`collection-filters` 样式。各类 Root、文件/文件夹识别及下级目录使用同一页头；保留路径、面包屑和所有原回调。当前显示的节点、直属核心文件/书籍及附件合计计数；快筛只影响原节点集，直属表格保持原行为。海报 footer、窗口栏为全应用共享，详情内下级海报及收藏页同样继承。各库类型固定，跨类型 Tabs 仍留在全部资源。

补充修改仅涉及 `src/pages/BrowsePage.tsx`、`src/pages/AllResourcesPage.tsx`、`src/styles/workspace.css` 及本报告/产品规格/项目上下文/长期决策。执行前最新工作树快照：`.tmp/library-refresh-20261009/20261009-100001/worktree-source.zip`，316 文件，SHA-256 `f7de33e1da7d687aa2037c616b1dae9b237fcd1a10d46155e56d605570bc0659`；项目 Skill 单独备份，未修改。

验证：117 前端测试、278 Rust 测试（10 默认忽略）、135 项目校验、rustfmt 和严格 Clippy 全部通过。浏览器实际组件夹具的 195 项资源库检查覆盖四语言/两主题/900×640、1280×800、1920×1080 与 DPR 1/1.25/2，共 24 组；另覆盖六类别 × 两种识别模式、直属文件和书籍、嵌套面包屑、导航恢复、库范围扫描/匹配及空库。原有 135 项全应用浏览器回归亦通过。截图、JSON 与日志位于 `.tmp/library-refresh-20261009/verification/`。浏览器 IPC 是夹具，未对真实媒体执行测试动作。

使用 `scripts/build_windows_release.ps1 -Bundles none` 编译当前最新主程序/helper，版本仍为 0.5.11，构建日期 2026-10-09。标准构建隐私检查通过；将原打包脚本及必要输入复制到新的私有打包目录后运行 `build_portable.ps1 -SkipBuild`，未改脚本、未覆盖正式 Portable/边车。新包：`bundle/local-test-20261009-100839/M2Shelf-Portable-0.5.11-x64.zip`，SHA-256 `CE4C660CE46258C24317FB41EE192609661795A25717A9B58018D92561156BF7`；解包程序位于同目录 `M2Shelf/M2Shelf.exe`。正式包、Logo 和公钥摘要保持。未签名、发布、提交或推送。

原生检查：解包主程序在正常 Windows 用户环境成功启动，窗口标题 M²Shelf，进程持续运行且响应正常，已保留该最新版窗口供用户使用。尝试重定向 APPDATA/LOCALAPPDATA 的隔离启动曾提前退出，原因未确认；正常环境启动通过。因此原生证据仅为实际启动/响应，不宣称原生全页面交互通过，也不把隔离环境退出推断为产品 Bug。页面视觉、各类型数据和操作范围证据来自浏览器实际 React 夹具。

- 海报仍使用原 2:3 框和自适应列宽；角标、单图片显示、cover/contain 判定、实际滚动根预热、原生缩略图和有界缓存均未改。标题最多两行，数量和用户标签摘要共用 20px 行，最多一个标签及 `+N`。无标签不产生独立空标签行。完整标签保留在数据、过滤和编辑流程中，摘要悬停显示完整列表。
- 全部资源页按“标题/动态计数与管理动作 → 六类 Tabs → 快筛/标签/排序/视图”组织。类型为全部、动画、真人影视、漫画、电子书、同人本。保留旧 VIDEO 兼容，以及编辑、仅匹配已有内容、扫描并匹配的不同动作和真实禁用状态。900px 下管理动作允许独立折行，六个类型和筛选控件均可见。
- 原 36px 自绘窗口栏保留 Logo、品牌、拖拽区域、最小化/最大化/关闭调用；加入系统/浅色/深色主题及四语言快捷选择。主题和语言与设置页双向同步、持久化，失败回滚，不重载页面。模态期间隐藏快捷设置，阅读器全屏沿用原 caption 隐藏规则。

## 状态和调用关系

全部资源仍直接使用 App 的 `allMediaKind`、`allFilter`、`allTagFilterId`、`allSort`、`viewMode` 和原导航快照；没有第二套筛选 state。Tabs 替换该页的类型 Select，其他页面的共享 Select 保留。卡片打开/编辑选择、菜单、聚合来源目标、绑定和重试回调保持原契约。列表同样复用紧凑标签摘要。

`AppSettingsProvider` 的 `SettingsStore` 是应用生命周期内唯一的完整 AppSettings writer。初始化调用已有 `api.getSettings`，读取去重；设置页和窗口栏均调用同一 `change`，乐观发布最新完整候选，串行调用既有 `api.updateSettings`。旧响应不覆盖新修订，最终失败回到最近成功值，App 报错并应用回滚外观。保存不依赖设置页挂载；没有新增 localStorage、数据库键、IPC、权限或状态框架。设置页原缓存统计仍独立加载，并丢弃旧缓存目录响应；成功保存回执的 3200ms / 180ms 时序保留。

App 通过共享设置订阅应用语言/主题/更新偏好/全部资源聚合开关，现有 `matchMedia` 监听保持 System 实时跟随。初始索引慢返回不再重复覆盖已经编辑过的外观/聚合偏好。首次使用时创建库后的播放器路径保存也通过同一 writer，并等待保存完成后继续原有初始化；不会留下另一条写旧快照的入口，原创建顺序保持。

## 本轮文件清单

这是相对执行前安全快照的清单，不能将 Git 中更早的未提交差异算作本轮修改。

| 文件 | 本轮改动 |
| --- | --- |
| `src/components/MediaCard.tsx` | 数量与一个标签及 +N 合行，删除独立标签槽，原回调/图片 hook 保持 |
| `src/components/MediaCard.test.tsx` | 无标签/单标签/长标签/多标签、列表和编辑点击回归 |
| `src/pages/AllResourcesPage.tsx` | 三层页头、动态数量、复用类型筛选的可键盘操作 Tabs，删除重复数量和该页类型 Select |
| `src/components/WindowTitlebar.tsx` | 使用共享 Select 的主题和语言快捷入口，原窗口命令保持 |
| `src/lib/settingsStore.tsx`（新增） | 应用级唯一序列化设置 writer、读取去重、修订保护与回滚 |
| `src/lib/settingsStore.test.ts`（新增） | 合并编辑、连续写入、卸载后失败、过期失败保护 |
| `src/pages/SettingsPage.tsx` | 将页内保存队列移至共享 writer；保留原设置页面与回执/缓存统计行为 |
| `src/App.tsx` | 订阅共享设置、统一错误报告，移除晚到初始化对外观的重复覆盖 |
| `src/App.test.tsx` | 与正式入口一致的 Provider；更新实际页名断言；新增主题/语言同步、系统跟随、失败回滚、类型兼容、慢启动保护 |
| `src/main.tsx` | 同一 Provider 包含标题栏和 App |
| `src/lib/i18n.tsx` | 六类 Tabs 的“全部/真人影视”短标签，四语言完整 |
| `src/styles/workspace.css` | 紧凑 footer、局部三层页头、薄 caption 及窄窗口/模态兼容 |
| `scripts/validate_project.py` | 将设置保存静态契约迁移到实际共享 writer；保留保存、修订和卸载回滚检查 |
| `AGENTS.md` | 仅替换已被本次要求推翻的独立标签空行规定 |
| `docs/PRODUCT_SPEC.md` | 更新三层页头、单元信息行和共享快捷设置的当前行为 |
| `docs/DECISIONS.md` | D53 及 D23/D50 中被明确替代的标签布局文字 |
| `docs/PROJECT_CONTEXT.md` | 记录实际实现，并标注旧标签布局的历史性质 |
| 本文件 | 交付与验证记录 |

旧项目 Skill 及 references 两个文件摘要保持不变。没有修改 `styles.css`、Sidebar、详情/阅读器源码、共享封面 hooks、`PosterImage`、任何 Rust 源码、schema、依赖、权限或版本。

## 渲染证据

截图均为完整当前 React 应用的 Chrome 浏览器夹具，Tauri IPC 为受控替身，使用虚构资源和封面；不是独立设计原型，也不是原生 Windows WebView 截图。私有输出不进入公共 Git 历史。

目录：`.tmp/ui-targeted-refresh-20261009/verification/`

| 文件 | 内容 |
| --- | --- |
| `index.html` | 可打开的截图与前后对比页 |
| `main-1280-dark.png` | 1280×800 深色，混合类型/多种标签/缺封面 |
| `main-900-dark.png` | 900×640 深色，六类 Tabs 与紧凑工具栏 |
| `main-1440-light.png` | 1440×900 浅色 |
| `titlebar-quick-settings-2x.png` | 标题栏右侧原始 DPR 2 局部截图 |
| `before-main-dark.png` / `after-main-dark-same-fixture.png` | 相同 1280×800、相同原始夹具的前后主界面 |
| `before-footer.png` / `after-footer.png` | 同卡片前后对比 |
| `before-metrics.json` / `after-metrics.json` | 实测图片、标题、元信息、卡片及后续行坐标 |
| `regression.json` | 24 组语言/主题/视口/DPR 与 135 项浏览器检查记录 |
| `preservation.json` | 执行前文件摘要对照及非目标文件保护结果 |

同一原始夹具实测：图片框始终约 **166.59×249.89px**，信息区 **102.02→72.02px**，卡片总高减少 30px。图片底到标题顶 **12→10px**；标题底到数量顶 **34→4px**；第二排海报顶与首排海报顶的距离 **379.91→349.91px**。网格列宽、图片比例和 28px 行间隔未改。0/1/N/超长标签的 footer 高度一致，长标题限制为两行。

## 测试结果

| 检查 | 结果 |
| --- | --- |
| `npm run typecheck` | 通过；最终 build 再次包含完整 typecheck |
| `npm test` | **117 通过**，17 个测试文件 |
| `npm run build` | 通过；存在现有主 bundle 超过 500kB 的 Vite 提示，未扩展为分包重构 |
| `npm run validate` | **135 通过，0 失败** |
| `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` | 通过 |
| `cargo test --manifest-path src-tauri/Cargo.toml --locked` | **278 通过，10 忽略，0 失败**；忽略项为既有专用/外网检查 |
| `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings` | 通过 |
| 浏览器实际组件回归 | **135 检查通过**；四语言×双主题×900×640 / 1280×800 / 1920×1080，分别 DPR 1 / 1.25 / 2，共 24 组 |

浏览器检查包括：六类 Tabs、旧 VIDEO、动态过滤、0/1/N/长标签及长标题几何、没有横向溢出、快捷菜单定位/Escape、系统主题实时改变、四语言及设置双向同步、失败保存回滚、导航类型/排序/列表状态保留、Library/Favorites 共享卡片、右键菜单、模态隔离、漫画进入/退出与浏览器 fullscreen、原窗口控制命令及两种不同扫描/匹配参数。源文件保护按执行前 313 项摘要核对，目标外修改为 0，Skill 两项摘要和 HEAD 不变。

未运行/不能作为完成声明的项目：真实 Windows WebView 的 caption 拖拽/双击、真实最小化/最大化/关闭效果、多显示器 DPI 切换和 Windows 原生全屏交互；真实数据库重启后偏好值验证。浏览器只核对 native 命令连接及持久化 API 调用，不能证明真实进程/磁盘操作。当前工具没有可操作的原生桌面表面，未重启用户正在使用的程序。未进行真实网络匹配、正式打包、签名或发布。

## 安全快照与恢复

执行分支仍为 `ui/global-visual-refresh`，HEAD 为 `f7f5c1e44e991aa7c7f0cd1b0f460921a18e15b6`。开始时工作树已有大量未提交和未跟踪内容，未重置、切换、清理或提交它们。

本轮新快照目录：`.tmp/ui-targeted-refresh-20261009/20261009-092631/`

- `worktree-source.zip`：313 个已跟踪及重要非忽略文件，逐项字节/SHA-256 校验通过。SHA-256：`67ff06e4084d9885c5a95ebd396280895716301ee462ac2e50b8cd21a072cf11`。
- `ignored-project-skills.zip`：两项项目 Skill/reference 单独备份。SHA-256：`f0a5ca6a24d97b16b67bc8e713893d0cc0f666a2a5cf5e88a7dbe912fe4c860c`。
- `snapshot.json`：每项路径/大小/摘要及原 HEAD。快照不包含依赖、真实数据库、媒体或生产密钥。

需要恢复时，先将归档解压到新的比较目录并按 snapshot 校验，只比较/恢复确认的目标文件；当前后来新增的工作继续单独保留。不要直接覆盖整个工作树或使用 reset/clean。备份未删除，分支和暂存区未改变。附件执行资料只解压在该快照的私有 `instructions/`，未复制到公共上下文。
