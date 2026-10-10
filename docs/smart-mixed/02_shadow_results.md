# SMART_MIXED Phase 2：实际执行结果

交付日期：2026-10-09。**只读索引适配器、隔离 Shadow runner、当前 schema 工厂、测试、性能测量和第三阶段草案已完成。停在 Phase 2，等待审阅；没有进入 Phase 3。**

所有实际执行使用私有合成 SQLite。没有查询个人 DB、扫描或读取用户媒体，没有 Bangumi 网络访问、生产写入、提交、推送、签名、打包或发布。当前正式应用没有 SMART_MIXED 选项。

## 1. 验证结论

| 验证 | 实际结果 | 证据 |
|---|---|---|
| 原 Phase 1 Rust | 84 PASS / 0 FAIL / 0 IGNORED | `.tmp/smart-mixed-phase2/phase1-tests.log` |
| 原 Phase 1 release CLI golden | 49/49 精确结构/指定诊断对照 | `phase2-verification.json:phase1_cli_goldens`，`scripts/verify.py` |
| 新 adapter + Shadow Rust | **42 PASS / 0 FAIL / 0 IGNORED**（1 unit + 41 integration） | `.tmp/smart-mixed-phase2/shadow-tests-final.log`，`02_shadow_test_matrix.md` |
| 新 crate rustfmt check | PASS | 下方命令，exit 0 |
| Clippy all-targets/all-features | PASS，`-D warnings` | `.tmp/smart-mixed-phase2/clippy-all-final.log` |
| Clippy all-targets/default（无 fixtures） | PASS，`-D warnings` | `.tmp/smart-mixed-phase2/clippy-default-final.log` |
| 独立 fixtures release build | PASS | `.tmp/smart-mixed-phase2/release-final.log` |
| 11 个合成 SQLite CLI 案例 | 各运行两次，summary 确定性、主 DB SHA 不变、无缺失/重复 | `phase2-verification.json:cli_cases` |
| CLI 失败路径 | 非法 Root：exit 2，`ROOT_NOT_FOUND`，无半份成功报告 | `scripts/verify.py` |
| 性能完整流程 | 1k/10k/100k，各 3 次新进程 | `phase2-verification.json:benchmarks` |
| 原工作树保护 | 467 个旧文件 SHA 全部不变、HEAD/branch 不变、私有 ZIP 逐文件校验 | `phase2-scope-verification.json` |

Phase 1 的 84 项与新 42 项属于不同测试套件；49 个 CLI golden 和 11 个 CLI 场景没有混算成 Rust test 数。

没有执行正式应用 npm/Rust 全量 gate、GUI、真实 decoder、更新器验收。本轮未改任何正式应用文件，只编译独立 crate；以上不是正式软件完整验收声明。

## 2. 可运行代码与实际命令

入口是 `tools/smart-mixed-shadow/`，自己的 workspace、Cargo.lock、src、tests、scripts 和 README。直接 path-depend 原 `tools/smart-mixed-lab` 的纯识别器，没有重写第二套识别算法。依赖版本固定，离线构建；没有改生产依赖。

在仓库根执行（环境 Rust/MSVC 与依赖缓存已就绪）：

```powershell
cargo test --offline --locked --manifest-path tools/smart-mixed-lab/Cargo.toml
cargo fmt --manifest-path tools/smart-mixed-shadow/Cargo.toml -- --check
cargo test --offline --locked --all-features --manifest-path tools/smart-mixed-shadow/Cargo.toml
cargo clippy --offline --locked --all-targets --all-features --manifest-path tools/smart-mixed-shadow/Cargo.toml -- -D warnings
cargo clippy --offline --locked --all-targets --no-default-features --manifest-path tools/smart-mixed-shadow/Cargo.toml -- -D warnings
cargo build --release --offline --locked --features fixtures --manifest-path tools/smart-mixed-shadow/Cargo.toml
python tools/smart-mixed-shadow/scripts/verify.py
python tools/smart-mixed-shadow/scripts/check_scope.py
```

实际使用本机 Rust/Cargo 1.97.1 和随工作环境提供的 Python；本机 sandbox 对 Cargo 路径 canonicalization/cache 曾返回权限错误，因此 Cargo 独立检查在已批准的宿主执行环境中运行。操作范围仍是新工具、编译缓存及合成临时文件，不因此访问个人数据库。

`verify.py` 需要此前将两个 test 命令的完整输出分别记录到上表的 `phase1-tests.log`、`shadow-tests-final.log`；它读取实际测试日志中的计数，不以文档数字代替执行。

单例运行：

```powershell
$exe = 'tools/smart-mixed-shadow/target/release/m2shelf-smart-mixed-shadow.exe'
$fixture = (& $exe --make-fixture 3 | ConvertFrom-Json).synthetic_db
& $exe --shadow $fixture 1 --authorized-index
```

无 fixtures 的默认构建没有创建测试库命令；生产不依赖该 crate。`--shadow` 只打开显式选择的索引，READ_ONLY + query_only，Root 受范围约束。真实个人 DB 仍需其所有者另行明确允许，本轮没有获取或推定这种允许。

## 3. 实际读模型与表

读取：schema marker、library_roots、nodes、metadata_bindings、comic_books、comic_pages、resource_files、library_scan_health、scan_runs。字段、原调用链和源码出处见 `02_adapter_audit.md` 第4–5节，精确 API 见 `02_shadow_protocol.md`。

保留真实 Root/Node/book/Resource/page ID、原 revision、source_resource_stamp、source_path、page_index、locator。MOBI/AZW3 与 ARTBOOK 以当前实际 schema 处理；旧 TXT 的 text_encoding 回退已用测试覆盖。

适配器使用独立 SQLite 读连接和单个事务，不调用生产 Database 初始化/migrate。第一次 SELECT 固定快照；WAL 并发写测试、stale expected_version、真正读中断和 pin 后故障测试已通过。成功读以 ROLLBACK 结束，无持久组写入。

## 4. 三个核心案例的结构对照

全部为工厂写入当前正式 schema 的合成索引，不是用户媒体实测。

| 案例 | 当前来源/旧视图 | Shadow 结果 | 身份与差异 |
|---|---|---|---|
| 两个系列 + 100 独立 PDF（03） | 104 book IDs；Root Browse=2 Node +100 直属书籍；Root detail 投影=104 册，各系列=2 册 | 2 Series +100 Work，102 Root 入口，104 proposed，0 fallback；Root role=1/Series role=2 | 缺失0、重复0；100 个独立文件组没有旧独立 Node 锚点，因此 changed_group_count=100，不是说旧页面漏了100本书 |
| 同系列 PDF/CBZ/图片卷、多层范围容器（49） | 7 book IDs；Root Browse=1 Node；当前 Root/系列 detail 各7册 | 1 Series（REVIEW），6 proposed +1 image fallback；2 IntermediateContainer、1 Extras | 缺失0、重复0；原 PNG 页序 `[0,1]` 与 locator 保留；S02 说明系列后代证据不完整，changed_group_count=1 |
| 作者目录两部独立作品（10） | 2 book IDs；Root Browse=1 作者 Node；作者 detail=2册 | 1 Category、2 Work、0 Series；Root 入口1 | 缺失0、重复0；2 个独立文件组保留真实来源，没有把作者视为一部系列；changed_group_count=2 |

### 混合系列没有“造出第七个 verified”

当前 `comic_pages` 不保存图片 binary-validation attestation。Phase 1 合成 JSON 可以声明 verified；真实生产 schema 适配器不能凭 `.png` 后缀复制这种声明。因此图片书进入 physical fallback，完整索引身份/页序仍可达，系列降 REVIEW。这是**明确的索引证据缺口**，不是已解决的全自动图片归组。

本轮不解码、不遍历媒体来补齐证据。直接图片 + 子目录图片的案例 28 同样保留父/子两个 book ID，互不吞并；没有把缺证据当作删除理由。

### 人工、绑定与附件

manual WORK/CONTAINER/MIXED、IGNORED、不同 binding 有独立测试。Shadow 的任意 binding no_merge 比生产同 binding detail 穿透更保守，两者分别报告；不改生产 owner。

核心 PDF/图片页不重复列普通附件。README、普通 ZIP、未知文件、未打开 MOBI ResourceFile 保留。打开过的附件保留 Resource FK、原索引 timestamp 与高精度 opened revision；不升为 Work、不隐藏或清理其阅读状态。当前阅读表与数量的去重规则按 `readingFiles.ts` 镜像，且有直接抽取生产 SQL 的 parity 测试。

## 5. 确定性、健康与安全结果

- 同一 snapshot 重复执行/序列化相同；逆 SQL 插入顺序按路径/来源投影相同。真实自增 ID 当然可能不同，测试不会把跨 DB 的随机 ID 相等当确定性要求。
- 模拟 override 仅在内存，人工/绑定 fence 不可削弱。没有新增持久 override 表。
- failed/partial/offline/cancelled/running 健康保留全部 book 来源，提案 REVIEW；中途取消/SQL interrupt 返回 Err，不输出半成功报告。
- 跨 Root owner/FK、路径逃逸、非法页 locator、孤儿 Node、超长文本、超过10k页、未来/缺失 schema 均有测试。
- 原始非连续 page_index 保留为原值并回退，不重新编号伪造完整索引。
- readonly unit test 将 query_only 临时关闭后仍不能 DELETE，证明 OS/SQLite READ_ONLY flags 不是只依赖一个 PRAGMA。
- seeded 进度/书签/最近打开/收藏/标签/绑定/手工封面/settings 等合成状态，读取前后全表摘要与 DB 字节一致。WAL 合成读测试主库与已有 WAL 内容不变。
- 全部 Root 内映射来源恰好在 proposed 或 fallback 之一；无法形成有效映射时拒绝，不假报 orphan=0。报告的0仅针对成功读取的 Root 归属集合，不是全库损坏行审计。

## 6. 完整流程性能

最终记录时间：`2026-10-09T09:35:44.849721+00:00`。同一台本地 Windows x64 主机、独立 release CLI、每规模3个新进程。Rust/Cargo 1.97.1，MSVC 构建。CPU 型号未在本轮获得（CIM 查询受环境权限限制），不虚构硬件参数。

| 来源数 | 合成 DB bytes | adapter ms | recognize ms | diff ms | 总计 ms | open→summary ms | 峰值工作集 MiB |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 1,000 | 593,920 | 6.4561 | 9.6497 | 1.1301 | 17.2228 | 17.7718 | 13.3711 |
| 10,000 | 3,575,808 | 51.1644 | 100.3886 | 14.5455 | 166.5361 | 166.8912 | 61.9688 |
| 100,000 | 34,099,200 | 572.9442 | 1,078.9815 | 158.9146 | **1,812.0689** | 1,812.4306 | **513.9414** |

口径：

- 数据分布为单 Root 直属独立 PDF 索引、每书一页，没有真实 PDF 文件、没有嵌套大图或真实解码。DB 创建在另一个进程中，不计入耗时/峰值。
- adapter 包含 SQLite 读取 IO、Root 映射、身份/路径/健康校验、流式摘要、旧 Browse/Root detail 集合；不含 Connection open。
- recognize 是原纯识别器；diff 包含 REVIEW 降级与**完整组差异对象**生成，不是只计一段空循环。
- total 为 adapter 开始至报告完成；open→summary 额外包含打开连接与摘要序列化，不含 stdout 最终写出/管道耗时。
- summary 序列化中位耗时分别0.0071/0.0087/0.0148 ms。完整私有 Snapshot+Plan JSON 序列化不计入基准，明确不将其冒充已经测量。
- 各阶段是分别取中位数，和总耗时中位数不一定逐项相加相等。
- 峰值通过 Windows `K32GetProcessMemoryInfo` 读取 PeakWorkingSetSize，各规模取3次最大值；含整个进程、SQLite、返回 Snapshot、Plan 和 diff，不是精确 Rust heap profiling。OS 文件缓存未清空控制，不是冷盘基准。
- 原 Phase 1 约347 MiB 是纯实验完整计划的另一测量；不能用于本轮 adapter+diff 的峰值，本轮实际约514 MiB。

该结果证明独立流程可以扩展至10万来源，**没有证明内存已经适合正式应用或 UI 线程**。需要 compact maps/strings、分页差异、后台预算和 recognizer 内部取消，见 Phase 3 草案。

## 7. 修改范围与安全快照

本轮新增的受审计源码/文档均位于两个目录：

```text
tools/smart-mixed-shadow/
  .gitignore
  Cargo.toml
  Cargo.lock
  README.md
  src/adapter.rs
  src/fixture.rs
  src/lib.rs
  src/main.rs
  src/model.rs
  tests/adapter_shadow.rs
  scripts/verify.py
  scripts/check_scope.py
docs/smart-mixed/
  02_adapter_audit.md
  02_shadow_protocol.md
  02_shadow_test_matrix.md
  02_shadow_results.md
  03_persistence_ui_plan_draft.md
  phase2-verification.json
  phase2-scope-verification.json
```

另外只有 Git-ignored 的新 crate target 与 `.tmp/smart-mixed-phase2/` 合成 DB、完整私有报告、日志和快照。没有将 private DB/带路径完整报告放入公开交付目录。

安全快照：`.tmp/smart-mixed-phase2/20261009-165210/`

- `worktree-before.zip`（467 个旧文件）
- `manifest.json`（各文件 SHA-256/size）
- `status-before.bin` / `status-after.bin`
- `current.json` 在上一级保存 HEAD/branch/SHA 指针
- ZIP SHA-256：`b82473ea42917adf5604daae658f47c022a601b81f9f093d4dd1a3e3cca5f1a4`

快照恢复对象是原工作树文件内容，不是整个 Git 元数据/机器环境镜像；没有自动恢复操作。旧 Phase 1 私有快照继续保留。

`check_scope.py` 逐文件重算467个旧SHA，检查新非忽略文件路径、HEAD/branch、ZIP完整性与ZIP内每个文件SHA，以及 private目录确实被Git忽略。结果记录于 `phase2-scope-verification.json`；**原工作树467个文件零变化，新增范围之外零文件，HEAD/branch未变**。这比仅看 HEAD 或“Git 没提交”更能证明原有未提交修改未被覆盖。

## 8. 仍需补齐与不能声称的事

1. 图片 binary-validation attestation 不存在，本轮 IMAGE_FOLDER 仅原身份/页序回退；混合系列并非7个全部 verified 自动成员。
2. 当前内容摘要不等于正式单调 index revision；上线需要 Root index/override/rules CAS 与事务生命周期。
3. 旧完整 detail 只覆盖 Root +最多64个首页锚点。其他锚点 `None` 表示未比较，不是0差异；source map全量但并非所有深层页面全展开。
4. 逻辑 group/member/override 持久化不存在；新 Root策略、Node metadata继承、同binding边界需要设计/验收。
5. 100k约514MiB，纯 recognize内部未接取消；未对抗任意恶意SQLite（少量health状态字段没有统一文本预算），只处理受信任已有应用索引。
6. Windows路径仅词法检查索引值，不证明现盘canonical/重解析点/句柄安全；现有scanner/reader安全不能被该层替代。
7. 无真实个人索引/媒体/native界面验收；没有生产版SMART_MIXED可交付，也没有本轮正式应用构建/发行证明。

## 9. Phase 3 次序与停止点

只提交草案 `03_persistence_ui_plan_draft.md`：

1. 图片证据、共享读模型、Root revision/CAS、内存与内部取消；
2. 新书籍 Root 的不可变策略设计及 additive schema升级测试；
3. 后台识别和单事务持久化、stale/partial安全；
4. 人工覆盖与既有Node元数据/绑定边界；
5. typed Browse/Detail集合API，再接UI与原reader/海报路径；
6. 旧模式/视频/状态回归、四语言两主题native验收、逻辑层安全回滚。

主要风险是混淆物理来源与逻辑组身份、图片证据不足、旧Node元数据被错误分摊、异步过期写入和大Root内存。当前交付没有通过提前接入绕开这些问题。

**本轮完成后等待用户与 ChatGPT 审阅本报告和 Phase 3 草案；不继续工程接入。** 本次16:50的一次性续跑安排已暂停。
