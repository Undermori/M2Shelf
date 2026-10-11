# M²Shelf 0.5.14 本地打包结果（2026-10-11）

本次按“打包给我”生成安装包和 Portable，已包含最新设置修改。播放器只有自定义/系统默认，默认系统播放器；无 mpv 自动检测。TMDb 服务设置、署名折叠区及移除匹配依据说明也在包内。

## 文件

| 文件 | 大小（字节） | SHA-256 |
|---|---:|---|
| `bundle/M2Shelf-Setup-0.5.14-x64.exe` | 10000240 | `0ccb8d56200e25dfb1850cf1d3e382e2559867fe59b03ef04bc9af4d0959aba2` |
| `bundle/M2Shelf-Portable-0.5.14-x64.zip` | 13263704 | `5676a8b68ba29acd6b3f9af21aaac12844043931cc07dfc9c5d6e001fda90fd0` |

## 验证

- 正式脚本完成 Windows x64 release、NSIS、helper、Portable；默认 TMDb 构建输入保留，未在 React、其他载荷、worker 源码或日志中泄露。
- 安装包解出的 helper、MOBI worker、worker 源码包和第三方声明与 Portable 逐字节一致。主程序仅有 Tauri 官方 NSS/UNK 分发类型标记的三个字节不同；统一标记后完整一致。未忽略或归一化任何其他字节。
- Portable 恰好八文件，ZIP CRC 与七条内容摘要全部通过；helper 的版本、应用 ID 和既有公钥一致。主程序和安装器元数据均为 M²Shelf 0.5.14，原生载荷为 x64。
- 主程序图标各帧与原 ICO 逐字节相同，顺序 256/128/64/48/32/24/16，PerMonitorV2 manifest 保留。构建者个人目录、工作树路径、worker 源码包隐私检查通过。
- 当前源码轮次 typecheck/build、173 项前端、135 项验证、340 项 Rust（13 项既有默认跳过）、fmt、严格 Clippy 已通过；打包脚本调整后项目验证重新执行为 135/0。27 项隔离原生检查和 245 项浏览器检查记录在设置修改验收目录。

## 构建顺序修复

封装之后再次构建 helper 会使安装包与 Portable 中的 helper 内容不同。本次改为 Tauri 无封装构建 → helper 锁定构建 → NSIS 封装 → Portable，更新现有顺序验证和构建文档。未更改应用功能、数据库、源媒体或生产信任根。

## 边界

最终包已静态解包验证，尚未覆盖安装或在干净 Windows 虚拟机验收。此前原生界面检查使用隔离应用 ID 和合成媒体；未启动生产 ID 的最终 EXE，以免初始化或扫描现有个人数据库。没有将这些检查表述为最终包的真实安装/启动验收。

本次未签名、生成 latest.json、提交、推送或发布。旧本地候选保存在 `.tmp/settings-package-20261011/prior-bundle/`，当前 `bundle/` 的八个解包文件同步为新候选。详情见 `delivery-summary.json` 和 `.tmp/settings-package-20261011/artifact-verification.json`。
