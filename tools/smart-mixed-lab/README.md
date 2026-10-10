# M²Shelf SMART_MIXED Recognition Lab — Phase 1

这是独立、未接入正式程序的 Rust 识别实验。输入是调用方提供的只读清单；输出是逻辑建议。不会扫描媒体目录、解码书籍、打开数据库、调用 Bangumi 或修改应用设置。

## API 与入口

```rust
use m2shelf_smart_mixed_lab::{model::{Snapshot, Plan}, recognize};
// pub fn recognize(snapshot: &Snapshot) -> Result<Plan, String>
```

模型在 `src/model.rs`，路径/名称证据在 `src/signals.rs`，识别在 `src/recognize.rs`，JSON CLI 在 `src/main.rs`。库本身没有 filesystem/network/database/process 接口；CLI 只读取显式 JSON 文件或 stdin。CLI 的 Windows 内存查询只用于 benchmark。

在项目根目录执行（需要 Rust/Cargo/MSVC 环境）：

```powershell
cargo test --offline --locked --manifest-path tools/smart-mixed-lab/Cargo.toml
cargo fmt --manifest-path tools/smart-mixed-lab/Cargo.toml -- --check
cargo clippy --offline --locked --all-targets --manifest-path tools/smart-mixed-lab/Cargo.toml -- -D warnings
cargo build --release --offline --locked --manifest-path tools/smart-mixed-lab/Cargo.toml
& ./tools/smart-mixed-lab/target/release/m2shelf-smart-mixed-lab.exe ./docs/smart-mixed/fixtures/example-mixed-series.input.json
& ./tools/smart-mixed-lab/target/release/m2shelf-smart-mixed-lab.exe --benchmark 10000
python ./tools/smart-mixed-lab/scripts/verify_and_record.py
```

`--offline` 使用已缓存依赖；新机器缺缓存时先按锁文件获取依赖，识别和测试自身不需要网络。实际本机 Cargo 通过已安装工具链完整路径运行，版本见测试报告。不会修改生产 `Cargo.toml` 或锁文件。独立 manifest 中的空 `[workspace]` 防止被生产 workspace 纳入；独立 `.gitignore` 只排除自身 `target/`。

## 输入约定

- `root_id` 是稳定、不含路径的 Root 标识；一次调用只处理一个 Root，四种书籍类型有效，三个视频类型直接拒绝。
- `entries` 包含 Root-relative 路径和状态；父路径由目录分隔推导，缺少的父目录会补为结构实体并给诊断。空字符串代表 Root。
- `verified=true` 是调用方**已经通过既有索引/格式安全检查**的证据。这里不验证 PDF/图片真实字节；扩展名本身不构成验证。文件书要求支持的后缀与格式一致；普通 ZIP、CBR、RAR 仍为附件。
- 图片 `page_orders` 必须来自既有索引自然顺序或可信书籍元数据；必须覆盖完整直属候选页，不能混入后代页、重复或未知页。原型不另写生产排序器。编号集合还会检查顺序单调。
- `identity` 可选，只能由原生索引/句柄边界提供、Root 内稳定的源身份；不能是书名或内容摘要。没有它时按 Root+种类+Windows ASCII NOCASE 路径生成引用。
- `metadata` / `hint` 是可选可信证据，不由本原型联网猜测。缺失时走保守规则。
- `overrides` 仅模拟人工覆盖；`prior_units` 模拟上次索引存在的来源，便于测试保留建议。没有持久化接口。

完整例子见 `docs/smart-mixed/fixtures/example-*.input.json`。编号 `01.json`–`49.json` 是包含 input、独立 expected 和负向要求的测试包装，不能直接作为 Snapshot CLI 输入。

## 输出与边界

`Plan` 包含 `physical_tree`、目录角色、唯一 `reading_units`、作品/系列与成员、版本候选、根展示及详情展开建议、保留的旧来源、诊断。每个判断带规则号、事实和解释；强度是 APPLY / REVIEW / KEEP_SEPARATE，没有任意置信度百分比。

`source_ref` / `proposal_ref` 是可复现 JSON 元组字符串，**不是正式 DB ID**；组键变化不会改变阅读来源。版本候选不共享进度。路径大小写冲突隔离，Unicode NFC/NFD 不凭文字猜测等价。输出中的 Excluded 状态包括冲突隔离状态；物理路径实体仍保留。

预算：CLI JSON 64 MiB；输入及补齐后清单各 ≤200,000 项；深度 ≤64；每个图片书 ≤10,000 页；所有页序合计 ≤2,000,000。路径字节上限 32,767 是实验输入预算，不等于 Windows UTF-16 路径许可。超限或无效关系整体返回错误；不产生部分写入。

84 项 Rust 测试和 49 项 CLI golden 对照通过。完整结果、当前实现审计、限制及下一阶段草案见 `docs/smart-mixed/`。**Phase 1 到此为止，没有正式 SMART_MIXED 模式。**
