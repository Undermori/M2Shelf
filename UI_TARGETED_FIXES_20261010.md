# M²Shelf 五项 UI 定向修复交付记录

后续追加修复及最新候选见 `ALL_RESOURCES_BOOK_FIX_20261010.md`：窗口语言按钮多余左占位、全部资源书籍投影、搜索与最近打开的逐书身份。本报告候选与验证保留为此前五项修复的历史记录。

日期：2026-10-10。当前版本 **0.5.12**，schema **26**。五项修复已合并当前本地工作树并构建新候选。此前主返工与 TMDb v2 保留，历史报告为 `REPAIR_REPORT.md`。

最新候选：`bundle/local-rc-ui-targeted-20261010/`。本轮没有安装、签名、发布、提交或推送。

## 1. 修改结果与根因

| 项目 | 实际修复 | 主要位置 |
|---|---|---|
| TMDb 配置位置 | 官方 Logo、英文原声明、四语言许可说明、官网和配置按钮整块移到浏览与扫描，紧邻 Bangumi 开关；About 不重复展示。沿用原生配置与外链 API | `src/pages/SettingsPage.tsx:182`；`src/styles/workspace.css:743` |
| 窗口栏菜单 | 共用 Select 原来按触发器左边缘展开最少 200px 菜单。900px 基线里菜单右缘为 832px，横向延伸至原生按钮下方的区域。窗口栏两选择器改为末端锚定，新右缘为触发器右缘 752px；首绘前定位，随窗口/触发器尺寸变化调整，保留视口内边距与可滚动高度 | `src/components/Select.tsx:34`；`src/components/WindowTitlebar.tsx:51` |
| 高亮与状态 | 复用原有中性 hover、品牌复选框和鼠标/键盘焦点规则；应用 UI 选区改用灰色。扫描时间及说明保持普通颜色，只对结果赋予成功绿、警告金、错误红，避免整行使用强调色 | `src/components/LibraryScanHealth.tsx:11`；`src/styles/workspace.css:25`、`:757` |
| 缓存路径底边 | 外框 36px，1px 边框后的内高只有 34px，而旧子控件 min-height 为 36px，背景越过内部下边缘。局部取消子项最小高度并拉伸到 34px，统一内外圆角；没有遮盖伪元素或 overflow 裁剪 | `src/styles/workspace.css:750` |
| 创建资源库 | 视频也使用两张等宽、12px 间距、完整 1px 边框的独立卡片。六类均无默认方式、选择后点创建；删除常驻提示和空 span。仅未选择就点创建时显示就地提示并聚焦选项，选择/切换类型清除提示 | `src/components/LibraryRecognitionModeDialog.tsx:20`、`:95`；`src/styles/workspace.css:798` |

视频仍只有 `FOLDER / VIDEO_FILE`；书籍仍为智能混合、文件夹、单本三个选项，默认“不自动关联 Bangumi”保持。创建按钮可点击以触发缺项校验，**实际提交必须已有选择**；busy 时禁用。视频选择卡片不再立即创建，改为显式确认，这是本轮包中“未选择点创建时校验”的统一交互要求，见决策 D57。

对话框焦点陷阱只随打开路径建立，不再因父组件每次产生新的 onClose 函数而重新抢焦点；Escape 使用最新回调。没有新建状态管理、菜单框架或主题系统。

## 2. 变更清单与保护核对

修改的生产文件只有六个：

- `src/components/Select.tsx`
- `src/components/WindowTitlebar.tsx`
- `src/components/LibraryRecognitionModeDialog.tsx`
- `src/components/LibraryScanHealth.tsx`
- `src/pages/SettingsPage.tsx`
- `src/styles/workspace.css`

对应调整三个测试文件：`Select.test.tsx`、`LibraryRecognitionModeDialog.test.tsx`、`ComicReaderPage.test.tsx`。同步 `docs/PRODUCT_SPEC.md`、`docs/PROJECT_CONTEXT.md`、`docs/DECISIONS.md`，新增本报告，并在旧交付报告中加最新报告指引。其余新增夹具、基线、日志、截图和本地候选保存在忽略的开发输出中。

开始前保存 **637 文件**工作树快照，包含未提交/未跟踪实现和项目 Skill：

- `.tmp/ui-targeted-20261010/snapshot/worktree-before.zip`
- SHA-256：`3daef1055013beea0721f565d014fce949c0450cbfaf33b0e6515f105893d86e`
- 同目录保留文件摘要、Git 状态和 staged/working diff。
- 分支保持 `ui/global-visual-refresh`，HEAD 保持 `f7f5c1e44e991aa7c7f0cd1b0f460921a18e15b6`。

逐文件核对所有基线文件仍存在，变化严格位于上述 UI/测试/文档清单。**所有 Rust、迁移、扫描器、SMART_MIXED、阅读器实现、poster 管道、API、凭证实现、图标、Skill、AGENTS、依赖、版本配置和更新公钥均与快照字节相同**。没有读写用户数据库、媒体、真实凭证或生产私钥。

证据：`.tmp/ui-targeted-20261010/preservation.json`、`task-only.patch`。此前交付目录 `bundle/local-rc-repair-20261010/` 保留；标准 bundle 文件由本次新构建更新。

读取并使用 frontend-design-codex 和项目 m2shelf-ui Skill。`product-constraints.md:10` 的“Preserve ... About content including author links and credits”保留其内容保护含义；用户本轮明确授权迁移 TMDb 位置，所以按当前要求搬移并记录 D57。没有修改 Skill 或 AGENTS。

## 3. 验证结果

| 检查 | 结果 |
|---|---|
| `npm run typecheck` | 通过 |
| `npm run build` | 通过；既有大 chunk 提示保留，没有为此扩大重构 |
| `npm test` | 21 文件、136 测试通过 |
| `npm run validate` | 135 通过，0 失败 |
| `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` | 通过 |
| `cargo test --manifest-path src-tauri/Cargo.toml --locked` | 308 通过，13 原有忽略，0 失败 |
| `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings` | 通过 |
| Chrome 真实 React DOM + 合成 IPC | 718 项检查通过，0 React 运行错误 |
| 标准 Windows 构建、NSIS、updater、MOBI worker、八文件 Portable | 通过；未安装或启动默认主程序 |
| 载荷、摘要、x64、版本、helper identity/公钥、私有构建路径扫描 | 通过 |

渲染矩阵：900×640、1280×800、1920×1080；light/dark；zh-CN/en-US/ja-JP/ko-KR。缓存控件另覆盖 Chrome DPR 1、1.25、1.5 与正常/hover/键盘 focus/模拟 disabled。**Chrome DPR 不是 Windows DPI 实测。**

实际 DOM 检查包括菜单视口边界、打开时缩窗、长语言名称、上下键/End/Enter/Esc/Tab/外部点击、语言设置写入、三枚原生窗口按钮的合成回调、六种库选项数量和缺项校验、底层创建参数、书籍原自动关联默认值、TMDb 英文声明和本地化许可、配置成功/取消/失败的合成回调、官方链接、movie 搜索结果、扫描语义色、文字可选择复制、鼠标安静焦点和 Tab 指示、阅读器顶部/底部控件及运行中 system 主题切换。

第一次匹配巡检误选了 series 夹具，改为 movie Work 后通过；system 主题断言补上异步事件等待后通过。均为验证脚本问题，没有因此修改匹配或主题业务。记录保留于忽略输出，最终结果在 `verification/ui-results.json`。

## 4. 修前/修后截图

统一目录：`.tmp/ui-targeted-20261010/verification/`。其中 `before-*` 来自本轮修改前的最新工作树归档，`after-*` 来自本轮真实 React 组件，数据和 IPC 均为合成。不是旧版截图或手绘原型。

| 对照 | 修前 | 修后代表图 |
|---|---|---|
| TMDb 位置 | `before-tmdb-about-dark.png` | `after-tmdb-browse-dark-zh-CN.png`、`after-about-dark.png` |
| 900px 语言菜单 | `before-language-900-dark.png` | `after-language-900-dark-zh-CN.png`、`after-language-900-light-en-US.png` |
| 设置状态/选区 | `before-settings-roots-dark.png` | `after-health-PARTIAL-dark.png`、`after-health-FAILED-light.png`、`after-text-selection-dark.png` |
| 缓存控件底边 | `before-cache-path-dark.png` | `after-cache-normal-dark.png`、`after-cache-hover-light-900-dpr1.5.png`、`after-cache-focus-dark.png`、`after-cache-disabled-light.png` |
| 视频卡片 | `before-video-modes-dark.png` | `after-video-unselected-dark.png`、`after-video-hover-light.png`、`after-mode-1-dark-ko-KR.png` |
| 书籍三模式/校验 | `before-book-modes-dark.png` | `after-book-unselected-dark.png`、`after-mode-validation-light.png`、`after-mode-3-dark-zh-CN.png` |
| 其他交互巡检 | 本轮基线及前轮报告 | `after-home-light.png`、`after-checkbox-keyboard-dark.png`、`after-bangumi-dark.png`、`after-tmdb-matching-light.png`、`after-reader-controls-light.png` |

完整图片索引：`.tmp/ui-targeted-20261010/verification/index.html`。索引只包含正式 before/after 图，不包括脚本失败时的诊断截屏。图片保存在忽略目录，不加入公开 Git 历史。

## 5. 最新本地候选

| 文件 | 字节 | SHA-256 |
|---|---:|---|
| `bundle/local-rc-ui-targeted-20261010/M2Shelf-Setup-0.5.12-x64.exe` | 9,849,608 | `62b71543e01ab8e2c7f2b1ec17edf4145539d8ac03fe3339b9f86b9fe9198212` |
| `bundle/local-rc-ui-targeted-20261010/M2Shelf-Portable-0.5.12-x64.zip` | 13,058,435 | `c046295accf68561fc62681559a673f7b7e7a43c77545bd7bb210df3c1a022b6` |

Portable 精确八文件及内部七项摘要已验证，主程序/updater/worker 与 release 二进制逐字匹配；主程序、updater 和 NSIS 的 ProductVersion/FileVersion 均为 0.5.12，PE 为 x64。helper 的应用 ID、版本和公钥与当前配置匹配。对载荷和 worker 源码 ZIP 内部进行 UTF-8/UTF-16 私有构建路径扫描通过。

验证详情：`.tmp/ui-targeted-20261010/artifact-verification.json`；可执行文件已解包在候选目录的 `portable/`。候选未签名，摘要不是正式更新授权，没有生成或复用正式签名清单。

## 6. 未验证事项

- 本轮未启动使用真实 AppData 的默认 Tauri 主程序，也未访问私人数据库或媒体。没有将浏览器夹具写成原生 WebView 验收。
- Windows 125%/150% DPI、原生标题栏拖动/最大化、Credential Manager 输入窗口和真实 TMDb/Bangumi 网络仍需实机验收；窗口按钮本轮仅验证 DOM 几何与原生调用契约。
- 当前候选保留主返工已有 schema 26；本轮不新增迁移。Portable 仍使用正常应用数据目录，不是隔离数据库。首次从旧版本运行前需要所有者保存完整升级前数据备份；本轮没有执行私人数据库备份或覆盖安装。

本轮止于五项修复和本地候选交付，没有开展新一轮设计。
