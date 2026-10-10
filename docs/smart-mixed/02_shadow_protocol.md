# SMART_MIXED Phase 2：只读 Shadow 协议

状态：独立可运行实验。没有生产应用入口，没有持久化推案，没有新增正式模式。本文描述**已实现的 Phase 2**；Phase 3 仅见单独草案。

## 1. 代码与 API

| 文件/符号 | 职责 |
|---|---|
| `tools/smart-mixed-shadow/Cargo.toml` | 独立 workspace、自己的 lock；生产应用没有依赖 |
| `src/adapter.rs:18` `ReadOptions` | expected_version、内存模拟 overrides、cancelled；fixtures 才有 SQL fault budget |
| `src/adapter.rs:186` `ReadIndex::open` | 显式路径，SQLite READ_ONLY/NO_MUTEX、query_only、trusted_schema OFF |
| `src/adapter.rs:200` `ReadIndex::read` | 单 Root 一致读事务，生成 IndexSnapshot |
| `src/adapter.rs:205` `read_with_pin_hook` | 固定快照之后的独立 harness seam；用于并发提交与错误注入 |
| `src/model.rs:81` `IndexSnapshot` | 原身份映射、物理输入、健康、旧视图与摘要 |
| `src/lib.rs:32` `run` | adapter → Phase 1 recognize → REVIEW 降级 → compare；分段计时 |
| `src/lib.rs:97` `compare` | source_ref 映射、唯一归属、physical fallback、结构差异 |
| `src/model.rs:141` `ShadowReport` | summary、真实 book ID 映射、回退来源、差异、完整逻辑提案 |
| `src/fixture.rs:41` `Factory::create` | fixtures feature 专用，create_new，24 份当前 migration 的合成 DB |
| `src/main.rs:93` `execute` | CLI 参数、脱敏 summary、退出码和 Windows 峰值工作集 |

API 示例（不表示本轮读了个人 DB）：

```rust
let mut index = ReadIndex::open(explicitly_authorized_index_path)?;
let options = ReadOptions::default();
let (snapshot, report, timings) = run(&mut index, trusted_root_id, &options)?;
// 没有 apply/save-proposal API；不得将 report 反向驱动生产写入。
```

`read_with_pin_hook` 在默认 crate 中也存在，但生产完全不导入这个 crate。其闭包不获得连接；测试闭包只使用另一个合成 DB writer。`fixtures` feature 默认关闭，控制工厂和故障预算。不存在用 debug feature 暗中开放正式 Tauri command 的路径。

## 2. 输入边界与事务协议

1. 操作者显式选择 SQLite 文件与 Root ID；CLI 不探测 AppData，不查找个人数据库。
2. 以 SQLITE_OPEN_READ_ONLY 打开，3 秒 busy timeout；连接设置 query_only ON、trusted_schema OFF。读事务期间不执行 journal_mode、checkpoint、migration 或生产初始化。
3. 开启 deferred transaction，读取 schema marker 后快照固定。严格要求 marker 为 24 个不同版本，MIN=1/MAX=24。与未来 schema 不兼容时明确拒绝，不猜字段。
4. 读取指定 Root；stored media_kind 不是 COMIC 即 `OUT_OF_SCOPE_VIDEO_LIBRARY`，在访问书籍表之前停止。四种书籍子类型按当前存储 flags 解析。
5. 同一事务读取 Root scoped Node/binding/book/page/resource、scan health 与最新 scan run，校验 graph、FK、路径和上限。
6. 生成内容摘要；expected_version 不一致即 `STALE_INDEX_NO_REPORT`，不返回部分结果。
7. 计算旧 browse/detail 集合，显式 ROLLBACK；任何中途 Err 由 Transaction drop 回滚。清除 SQLite progress handler，以便连接可再次使用。
8. 在内存运行 unchanged Phase 1，再做差异比较。无数据库写入接口。

SQLite 只读查询仍可能参与 SQLite 锁/共享内存协调；本轮验证了合成主 DB、已有 WAL 内容不变，不能将这一事实泛化成所有操作系统环境下目录完全无副作用。它不读取媒体文件。

## 3. 快照字段与 revision 的精确定义

`IndexSnapshot` 包含：

- `root_identity`：真实 Root ID；`indexed_root_path`：索引的原始 Root 路径，仅供私有 provenance。
- `media_type`、`recognition_mode`：实际类别与旧模式原值。
- `index_snapshot_version`：流式序列化 `(schema 24, Root ID/path/kind/mode/policy, health, nodes, sources, resources)` 的 SHA-256。所有输入来自同一读事务。
- `override_revision`：**调用者原始模拟 override payload** 的 SHA-256，随后才规范化/合并硬边界；不是持久 override 表版本，也不保证语义相同但数组顺序不同的 payload 得到相同摘要。
- `nodes`、`source_id_map`、`resources`：真实 ID 与索引字段，不是重建后的生产对象。
- `recognition_input.entries/page_orders/prior_units/overrides`：供 Phase 1 使用的物理条目、既有页序、旧来源、仅内存覆盖。
- `current_browse_*`：首页直属 Node、book、待打开 readable Resource、Other Resource 的独立集合。
- `current_details`：隐藏 Root + 最多 64 个当前首页子锚点的旧详情投影。
- `scan_health`、计数式 `diagnostics`：不含私有书名的状态代码。

摘要是投影内容版本：进度/书签/收藏/标签未参与 recognition，不放入摘要。内容恢复为相同值可得到相同摘要；它不是单调提交号。不能单独用它在正式异步写入后证明全库或磁盘没有变化。

在同一 pinned snapshot 下，即使外部 writer 已提交新索引，本次仍返回旧版本和旧视图；下次读取才发现变化。上线前需要真正的事务内 Root revision/CAS，详见 Phase 3 草案。

## 4. 身份映射与未知来源保留

| 对象 | Shadow 保存/用途 | 禁止含义 |
|---|---|---|
| Node | id、parent、相对路径、类型、manual、binding | 新逻辑组不是一个假 Node |
| Source | book_id、node_id、resource_id、原 source_path/revision/stamp、格式字段、大小/时间戳、原 pages、status | 不换 book ID，不合并阅读进度 |
| Page | id、原 page_index/name/locator、size/CRC/modified | 不重新排列，不用 font-dependent screen 取代章节/页定位 |
| Resource | id、node_id、原路径/size/stamp、扩展名/type、核心重复标记 | 未打开附件不会凭后缀成为 Work |
| proposal/source_ref | Root + 来源类别 + `native:book:{id}` 的实验稳定引用 | 不是生产 PK、Reader ID 或持久 group ID |

Phase 1 的实验 source_ref 保持原算法。旧 prior 引用用同一 JSON tuple 编码，避免每次报告把已识别源误判成“旧来源未观察到”。最终 `source_ref_to_book_id` 映射回现有真实册 ID。

每个已提案 reading unit 必须映射到一个一致格式证据的核心 book；每个已提案 book 恰好属于一个逻辑主组。未被提案接收的真实 book 全部进入 `fallback_book_ids`。损坏、忽略、图片证据未知、附件阅读状态等保留物理索引身份。

这些等式针对**本 Root 可以通过 Node 归属读取的来源集合**：

```text
proposed book IDs ∩ physical fallback book IDs = ∅
proposed book IDs ∪ physical fallback book IDs = indexed book IDs
每个 proposed book ID 的主组成员次数 = 1
```

`orphan_sources=0` 表示本次成功返回的 Root 归属图/映射通过校验；不是对全数据库中无法归属任何 Root 的损坏行做了全库审计。发现 Root 内缺父、跨 Root FK、坏 owner/path、重复 source path、页序/上限等会拒绝或保守回退，不能伪造修复。

## 5. 格式证据状态

| status | 含义与行为 |
|---|---|
| `INDEXED_FORMAT_EVIDENCE` | 现有 reader/document/TXT 回退/CBZ 证据一致，revision、页数、原页序有效；可给 Phase 1 verified 输入。仅索引证据，未重读文件 |
| `INDEXED_IMAGE_BINARY_VALIDATION_UNKNOWN` | 核心图片书有原页序，但 schema 不证明 binary 格式；保留物理书，绝不按 PNG 后缀升级 verified |
| `INVALID_BOOK_INDEX_RETAINED` | 图片书索引不完整或非连续；原 page_index/locator 留在私有映射 |
| `UNAVAILABLE_OR_UNVERIFIED_INDEX` | 格式不一致、缺 revision、失败或 ignored 等；保守保留 |
| `ATTACHMENT_RETAINED_NOT_A_WORK` | 已打开的 ResourceFile 有 book 状态；仍是附件身份，不成为 Work |

未知 ResourceFile 不在 `indexed_sources` 的 book 数内，但始终存在 resources/物理条目与旧附件投影中。`unopened_readable_resources` 按**采样详情集合**去重，不是全 Root 所有附件后缀数；`remaining_attachments` 是全 Root 非核心重复 Resource 行数，含可读附件，不等于 Other UI 行数。

未持久保存作者/作品内容摘要等额外元数据时，`Entry.metadata/hint` 保持 None，不从目录名假造作者字段或外部验证事实。画师/作者分类用当前纯识别器结构证据，带适用边界。

## 6. 人工、绑定与扫描健康

任何非隐藏 Root 的手动分类或既有 Bangumi binding 加入 no_merge fence。忽略状态从祖先传播。调用者模拟 override 可以指定类别、系列成员/卷章等实验意图，但不能撤销生产硬边界；不写入任何 override 表。

同 Subject binding 的 descendant：旧 ownership SQL 仍可按当前实现穿透，Shadow 新归组暂保守保留边界。报告区分二者；不是暗中改变生产规则。

健康 complete 的必要条件：outcome SUCCESS、error_count 0、最近 run 若存在则 COMPLETED 且 errors 0。其他情况不丢索引行，全部新 group/member/unit 决策 REVIEW。含证据不全后代的 Series 额外带 S02。成功状态也不证明当前磁盘在线，freshness 总是 `INDEX_ONLY_CURRENT_DISK_UNKNOWN`。

## 7. 差异报告语义

完整 Rust `ShadowReport` 保存 Plan、每组真实 book IDs、原 owner Node IDs、对应旧锚点及理由 rule IDs。

- `current_detail_extra_books=Some(n)`：已计算该旧锚点，旧阅读表中比本提案多出的 book 数。
- `None`：没有真实对应锚点或不在本轮采样，不等于 0 差异。
- `changed_group_count`：没有旧锚点、已比较旧详情含额外册、或原 owner 超过一个的组数量。它是上述定义的结构差异计数，**不是像素 diff、完整行为差异或待写入更新数**。
- `rules`：group/member evidence 的出现次数，不是唯一受影响书数；同规则可以在多条证据中重复出现。
- `directory_roles`：Root/Series/Category/Intermediate/Extras/Ambiguous 等按当前 Plan 计数。REVIEW group 与 edition 关系单独统计，不能只看一个 Series 数判断自动可应用。
- `mutation_policy` 固定为 `READ_ONLY_PROPOSAL_AND_PHYSICAL_FALLBACK_NO_DELETIONS`。没有删除/重绑/改名建议或执行器。

旧 detail book 集合镜像 `comics::owned_nodes/books_for_nodes`，附件镜像 native 核心去重与前端 `readingFiles`。保护树完整可达，不因逻辑穿透失去真实目录位置。对照是 ID 集合而非 UI 排序/显示标题；不声称顺序或视觉已验收。

## 8. 限额、取消与内存

| 限额 | 当前值 |
|---|---:|
| SQLite 累计索引行 | 2,000,000 |
| 累计受预算文本 | 128 MiB |
| 单个受预算文本/路径 | 32,767 bytes |
| 单书 page 行 | 10,000 |
| 物理输入条目（页也计入） | 200,000 |
| 模拟 overrides | 200,000 |
| Node 深度 | 64 层边界，超限拒绝 |
| 完整旧详情投影 | Root + 最多 64 个首页子锚点 |
| Phase 1 既有总页序上限 | 2,000,000 |
| CLI benchmark 来源数 | 100,000 |

文本预算覆盖 Node/book/page/resource 等读取值；scan health 的少量状态/时间戳字段沿用 SQLite 类型读取，没有额外按此 Budget 逐项累加。此工具针对受信任的已有本地应用索引，不能作为任意恶意数据库解析器发布。

SQLite progress handler 每 1000 步检查取消；逐行映射和阶段边界检查 atomic cancel。Phase 1 recognize 内部目前无取消 callback，最长这一段仍可能完成后才响应。不能将当前实现描述为每个算法步骤实时可取消。

只按一个 Root 运行；摘要流式 hash，CLI 不把 Snapshot/Plan 再转为四份 JSON/DTO。但完整 Snapshot + Plan + compact diff 同时驻留，100k 实测仍约 514 MiB；未形成生产级内存目标。正式集成需要后台执行和进一步预算，不在 WebView 主线程运行。

## 9. 本轮实际命令与输出

在仓库根目录，先构建 fixtures release（完整命令见 results）：

```powershell
cargo build --release --offline --locked --features fixtures --manifest-path tools/smart-mixed-shadow/Cargo.toml
$exe = 'tools/smart-mixed-shadow/target/release/m2shelf-smart-mixed-shadow.exe'
$fixture = (& $exe --make-fixture 3 | ConvertFrom-Json).synthetic_db
& $exe --shadow $fixture 1 --authorized-index
```

`--make-fixture` 只能写固定私有目录 `.tmp/smart-mixed-phase2/cli-fixtures/`，生成独占新文件名、create_new 拒绝覆盖。所有路径/书名为合成数据；不会在合成的 R 盘 Root 路径创建真实媒体。

`--authorized-index` 是操作者对权限的显式确认，不代表 AI 可因此推定用户已授权读取任何真实个人库。本轮没有这样的授权，也没有执行个人 DB 查询。

CLI stdout 只有 summary、timings、摘要序列化耗时和峰值工作集；错误 stderr 为有限错误码 JSON，exit 2，无半成功 stdout。默认不输出源路径、书名列表或完整 Plan。fixture 创建命令单独返回合成 DB 的私有路径，便于接着验证。

完整对象可能包含个人路径/书名，调用者仅能保存私有报告。测试写三个**合成**完整报告：`.tmp/smart-mixed-phase2/{root-parallel,mixed-series,author-category}.shadow.json`。公开测量 JSON 不含 source names/absolute indexed paths。

本轮没有网络依赖、扫描媒体、解压媒体归档、写入阅读器状态、元数据匹配或封面生成。
