# M²Shelf 稳定更新发布流程

本文记录 `0.5.11` 起的 Windows 稳定更新发布契约。它只包含可公开的操作规则；生产私钥、DPAPI 文件、独立指纹、个人路径和账号凭据不得写入仓库、Issue、Actions、Release 或日志。

## 1. 信任模型

- 客户端只读取 `https://github.com/Undermori/M2Shelf/releases/latest/download/latest.json`；
- `latest.json` 内的资产 URL 必须固定到 `v{version}` Release，不能指向 `latest`、任意主机或可变文件名；
- 版本只允许严格高于当前版本的规范 `major.minor.patch`；
- 每个资产同时校验固定文件名、长度、SHA-256 和 Ed25519；签名绑定应用 ID、版本、平台、长度和摘要；
- GitHub Actions 不接触生产私钥，只构建无签名候选；
- 生产 seed 只以密码加密的 `M2Shelf-Production-Key/encrypted-private-key.m2key` 存放在可移动 USB 中；日常开发环境不保存其副本。容器不绑定 Windows 用户或单台电脑，密码只在签名工具中交互输入；
- 已发布 Release 视为不可变。修复错误发布必须提升版本，不能替换原资产。

## 2. 发布前准备

### 2.1 长期生产更新密钥规则

自 `v0.5.11` 起，当前 production seed 与 `src-tauri/update-public-key.txt` 中的 update public key 作为 M²Shelf 的长期生产更新签名密钥使用。后续正常版本发布必须继续使用现有 production seed，通过加密 USB 和人工输入密码完成签名；不得重新生成 production seed，也不得生成、替换或改写 `src-tauri/update-public-key.txt`。只有在明确执行生产密钥轮换流程时，才允许建立新的 seed 和公钥。

任何涉及 production key/seed 的生成、迁移、替换或轮换操作，都必须立即停止当前发布流程，并先取得项目所有者的明确确认。不得根据版本升级、文件缺失、Agent 建议或自动化结果自行推断需要迁移或轮换密钥。

### 2.2 一次性迁移到可移动 USB 密钥

`v0.5.11` 的既有 production seed 和公钥保持不变。CurrentUser-DPAPI 文件只作为一次性迁移源，不再作为长期日常签名格式。仓库中的 `tools/portable-key-tool` 是独立、非分发工具；`scripts/build_portable_key_tool.ps1` 只执行该 crate 的 fmt、test、clippy 与锁定 release 构建，绝不执行迁移或读取密钥。Agent 只能以合成测试 seed 验证它。

真实迁移不是普通构建步骤。每次准备操作真实 production seed 时，必须先停止并重新取得项目所有者的明确确认；不得由 Agent 自动发现、读取或执行。获得确认后，由密钥所有者在旧 DPAPI 所属 Windows 用户中手动运行一次：

```powershell
M2ShelfPortableKeyTool.exe migrate-dpapi `
  --input $legacyDpapiSeed `
  --output (Join-Path $usbRoot "M2Shelf-Production-Key\encrypted-private-key.m2key") `
  --public-key .\src-tauri\update-public-key.txt `
  --key-id production-v0.5.11
```

工具会在控制台无回显地要求输入并确认新密码；明文 seed 只存在于 Rust 进程内。它先确认旧 seed 对应当前公钥，再以 Argon2id 派生密钥、用 XChaCha20-Poly1305 认证加密，写出以下固定结构：

```text
M2Shelf-Production-Key/
├─ encrypted-private-key.m2key
├─ key-metadata.json
└─ README.txt
```

写入后工具会重新解密，确认 seed、公钥和确定性 Ed25519 签名均与迁移前一致。旧 `production-seed.dpapi` 不会被修改或删除，应作为独立离线恢复副本保留，直到项目所有者明确批准退役。密码不得写入脚本、仓库、密码参数、环境变量或日志；USB 内容不得上传到 GitHub、云盘或聊天。

迁移后可随时执行只读身份核对；该命令不会签名或生成 Release 文件：

```powershell
M2ShelfPortableKeyTool.exe verify-key `
  --key (Join-Path $usbRoot "M2Shelf-Production-Key\encrypted-private-key.m2key") `
  --public-key .\src-tauri\update-public-key.txt
```

1. 确认 `package.json`、lockfile、`src-tauri/Cargo.toml`、Cargo lockfile 和 `src-tauri/tauri.conf.json` 版本完全一致，且为稳定规范 SemVer；版本变更后同时刷新并审查 `tools/portable-key-tool/Cargo.lock`，因为它锁定了当前 `m2shelf_lib` 路径依赖。刷新锁文件不执行任何密钥命令。
2. 更新四语言 release notes，并确认所有长期文档与源码一致。
3. 在干净工作树运行 `AGENTS.md` 的完整质量门禁。
4. 使用 `scripts/build_windows_release.ps1 -Bundles nsis` 与 `scripts/build_portable.ps1` 做一次本地候选验证；检查版本、x64、图标、隐私扫描、Portable 八文件集合、helper identity 和启动。
5. 提交并推送目标 commit，在该 commit 创建唯一 tag `v{version}`，再推送 tag。不要移动或复用发布 tag。
6. 发布前审计完整可达 Git 历史的 author/committer；公开仓库只允许 GitHub noreply 地址。发现真实邮箱时必须先按经用户确认的历史清理流程处理并复核远端，再创建 release tag。

`v0.5.8` 只保留为 CI 失败的不可变历史 tag，`v0.5.9` 只保留为最终修复前创建且未公开的不可变历史 tag，`v0.5.10` 只保留为最终匹配修复和生产密钥轮换前创建的不可变未发布 tag；三者都没有 Release、资产或 `latest.json`，不得移动、复用或作为更新授权。`0.5.11` 是新的 updater 信任根引导版。`0.5.7` 及更早客户端和任何旧公钥测试包无法通过兼容的内置 updater 获取它，Release 文案必须明确要求手动安装一次。

## 3. 获取 CI 无签名候选

tag 会触发 `.github/workflows/windows-release.yml`。该 workflow 使用固定 action commit，执行完整门禁，构建 NSIS、Portable 和 helper，以 GitHub 官方 Sigstore attestation 绑定两个候选的仓库、tag、commit、workflow 和摘要，并上传短期保存的：

- `M2Shelf-Portable-{version}-x64.zip`；
- `M2Shelf-Portable-{version}-x64.zip.sha256`；
- `M2Shelf-Setup-{version}-x64.exe`；
- `M2Shelf-Setup-{version}-x64.exe.sha256`；
- `candidate-provenance.json`。

下载候选到仓库外或忽略的本地目录，保留原文件名与 provenance。`candidate-provenance.json` 是便于核对的 CI 元数据，不是密码学来源证明，不能单独授权生产签名。只接受 tag 触发、`tagExists=true`，且 repository、version、tag、commit 和资产摘要均与本地目标一致的候选。联网机使用 `gh attestation download` 获取两个候选的 bundle，并同时获取新的 `gh attestation trusted-root`；隔离签名环境按 GitHub 官方离线流程对两个候选执行 `gh attestation verify --bundle ... --custom-trusted-root ... -R Undermori/M2Shelf`，确认 source ref 为精确 `refs/tags/v{version}`和预期 commit 后，独立记录两个 SHA-256。若不使用 attestation，则必须在隔离环境从已验证 tag 独立构建并得到正式候选的可信精确摘要。正式签名只接受这条独立路径得到的摘要。

## 4. USB 密钥签名

发布负责人应预先拥有：已核对的 CI 无签名候选及其 attestation 或独立构建摘要、`candidate-provenance.json`、四语言 release notes、由锁定源码构建的 `M2ShelfPortableKeyTool.exe`，以及单独保存的加密生产密钥 USB。日常开发电脑不保存密钥副本；签名前插入，成功后拔出。

正常发版只运行一个包装命令：

```powershell
pwsh ./scripts/sign_update_from_usb.ps1 `
  -CandidateDirectory $candidateDirectory `
  -NotesPath $localizedNotesJson
```

默认模式只在已就绪的可移动盘中查找唯一的 `M2Shelf-Production-Key/encrypted-private-key.m2key`。若 Windows 把某个 USB 设备报告为固定磁盘，可显式传入 `-UsbRoot`；脚本不会打印密钥路径。候选目录和 release-notes 文件只有在当前版本恰好找到一个匹配项时才会自动采用，否则必须显式指定，避免误用旧候选。

PowerShell 只传递文件路径和公开元数据，不读取密码或私钥。`M2ShelfPortableKeyTool sign-release` 在控制台无回显地读取密码，在同一 Rust 进程内解密 seed、核对 `src-tauri/update-public-key.txt`、调用客户端共享的 Ed25519 签名消息实现，并立即用当前公钥验签。签名格式、URL 和 `latest.json` schema 与既有客户端完全相同。

输出包含一个只含以下八项正式文件的目录，以及同内容的 `M2Shelf-v{version}-SIGNED-RETURN.zip` 和其 SHA-256 sidecar：

- Portable ZIP、`.sha256`、`.sig`；
- NSIS EXE、`.sha256`、`.sig`；
- `latest.json`；
- 原 `candidate-provenance.json`。

正式 release notes JSON 必须恰好包含非空的 `zh-CN`、`en-US`、`ja-JP`、`ko-KR`。签名完成后拔出 USB；不要修改任何候选、sidecar、manifest 或 provenance，也不要把签名目录提交到 Git。现有 `scripts/publish_signed_release.ps1` 仍负责发布前用随包 helper 再次执行独立 fail-closed 公钥验签并发布。本地签名交接仍为八项，公开 Release 仅上传安装包、Portable 压缩包和自动更新需要的 `latest.json`；摘要与签名已包含在 manifest 中。

## 5. Fail-closed 验证与发布

发布必须在正常 Git worktree 中进行，并满足：工作树干净、HEAD 等于本地 `v{version}`、远端 tag 指向同一不可变对象、候选 provenance 来自该 tag/commit。

先运行只读验证：

```powershell
pwsh ./scripts/publish_signed_release.ps1 `
  -CandidateDirectory $candidateDirectory `
  -VerifierPath $trustedUpdaterPath `
  -TrustedVerifierSha256 $trustedUpdaterSha256 `
  -ReleaseNotesPath $releaseNotesMarkdown
```

输出明确说明验证通过且没有修改 GitHub 后，才运行：

```powershell
pwsh ./scripts/publish_signed_release.ps1 `
  -CandidateDirectory $candidateDirectory `
  -VerifierPath $trustedUpdaterPath `
  -TrustedVerifierSha256 $trustedUpdaterSha256 `
  -ReleaseNotesPath $releaseNotesMarkdown `
  -Publish
```

发布脚本会先核对独立保存的 verifier SHA-256、app ID、版本和嵌入公钥，并调用其 `verify` 命令对两个资产执行真实 Ed25519 验签；然后锁定本地输入，拒绝已有 Release，创建 draft，上传并要求每个远端资产同时精确匹配名称、大小和 GitHub 报告的 SHA-256 digest，重新确认 tag，最后才公开。公开集合恰好包含：安装包 EXE、Portable ZIP 和 `latest.json`。八项本地输入全部验签、锁定并复查，sidecar 与 provenance 保留作本地核验记录，不公开为下载附件。任一检查失败即停止；若已经建立 draft，它保持为 draft，必须先查明原因，不能绕过脚本直接公开。

## 6. 发布后验证

1. 核对 Release tag、commit、三个公开资产名、大小和公开 SHA-256；
2. 下载公开的 `latest.json`，确认版本、UTC 时间、四语言说明及两个 URL 固定到刚发布的 tag；
3. 普通后续版本应在上一稳定版的 NSIS 和 Portable 环境各检查一次：发现更新、明确下载、完整性/签名验证、安装与重启。首次公开引导版 `0.5.11` 例外：真实用户路径验证 `0.5.7` 手动安装 `0.5.11`，更新器链路则使用受控且内含同一新信任根的测试客户端验证，不得把 `v0.5.8`、`v0.5.9` 或 `v0.5.10` tag 描述成公开稳定版；
4. Portable 还需验证 helper-ready 后才退出旧程序、单实例、数据库保留、精确版本健康回执和完整三秒存活；用测试构造的失败场景确认文件/SQLite 自动回滚及两类恢复提示；
5. 确认 Library Root 内容和时间戳未变化，发布资产及解压 Portable 不含构建者个人路径；
6. 只有正式 `v0.5.11` 已发布后，才能把四语言 README 的公开版本与下载链接从 `0.5.7` 更新到 `0.5.11`；未公开的 `v0.5.8`、`v0.5.9`、`v0.5.10` tag 不得出现在公开下载入口。

更新包的 Ed25519 签名保护 M²Shelf updater 供应链，不等同于 Windows Authenticode。若没有 Authenticode，Windows 仍可能显示 SmartScreen 提示。
