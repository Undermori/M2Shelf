# Windows 本地开发与构建

## 环境

M²Shelf 的 Windows x64 开发环境使用 Rust MSVC 工具链、Microsoft Visual Studio Build Tools 的 C++ 桌面构建组件、Windows SDK、WebView2 和 Node.js。依赖版本使用项目的 `Cargo.lock` 和 `package-lock.json`，无需先升级依赖。

本轮环境配置版本：Rust/Cargo 1.98.1、rustup 1.29.1、Visual Studio Build Tools 2022 17.14.40（MSVC 14.44）、Windows SDK 10.0.26100、Node.js 24.16.0。Rust 包含 `rustfmt` 和 `clippy`，默认目标为 `x86_64-pc-windows-msvc`。

安装 Rust 后重新打开终端，使用户 PATH 生效。如果正在使用安装前已打开的 PowerShell，可仅在该终端中刷新 Rust 路径：

```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
```

以下命令均在包含 `package.json` 的项目根目录执行。

## 自检与运行

```powershell
rustc --version
cargo --version
npm run tauri info
npm run tauri dev
```

`tauri dev` 启动桌面应用和 Vite 开发服务，适合继续修改和调试。第一次编译 Rust 依赖需要较长时间。

## 验证

```powershell
npm run check
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml --locked
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings
```

## 本地构建

构建主程序和更新辅助程序，并检查 x64 架构和构建路径：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build_windows_release.ps1 -Bundles none
```

输出位于 `src-tauri/target/release/m2shelf.exe` 和 `src-tauri/target/release/M2ShelfUpdater.exe`。若需要 NSIS 安装包，将 `-Bundles none` 改为 `-Bundles nsis`。编译产物应保持在 Git 忽略目录中。

如果旧 exe 正在运行，可以保留它并追加 `-TargetDirectory src-tauri/target/local-build`。新产物位于该目录的 `release/` 下；脚本也尊重已有的 `CARGO_TARGET_DIR`，两种路径均执行相同架构和隐私检查，并在结束后恢复环境变量。

本地构建不等于签名发布。对外发布仍须按 [更新发布流程](UPDATE_RELEASE_PROCESS.md) 完成验证和签名。

## 官方安装资料

- [Tauri Windows 环境要求](https://v2.tauri.app/start/prerequisites/#windows)
- [Rust 安装](https://rust-lang.org/tools/install/)
- [Visual Studio 命令行安装参数](https://learn.microsoft.com/en-us/visualstudio/install/use-command-line-parameters-to-install-visual-studio?view=vs-2022)
