"""Render task reports from authored expectations and recorded CLI measurements (not a recognizer)."""
import json
from pathlib import Path

ROOT=Path(__file__).resolve().parents[3]
DOCS=ROOT/'docs/smart-mixed'
v=json.loads((DOCS/'verification.json').read_text(encoding='utf-8'))
cases=[json.loads(p.read_text(encoding='utf-8')) for p in sorted((DOCS/'fixtures').glob('[0-9][0-9].json'))]
assert [c['id'] for c in cases]==list(range(1,50))
assert v['cli_golden_passed']==49 and v['cli_golden_failed']==0
titles=['Root单PDF','Root 300独立PDF','2系列+100独立PDF','东京食尸鬼显式卷册','PDF+CBZ','PDF+EPUB+TXT','文件册+图片卷','范围容器内裸数字卷','多层卷/话/图片','作者多个不同作品','同人画师分类','设定集+原画素材','数字书名20世纪少年','数字书名86','根文件精确前缀虚拟系列','仅一个显式卷','Root裸数字文件','明确作品内裸数字','缺第02卷','同卷多格式版本','同名不同父目录/Root','全集与分卷并存','特典番外目录','角色设定资料','cover排除','wallpaper/keyvisual/poster','图片书与readme附件','父图片书与不同作品子图片书','父图片与子文件卷','单图片','自然顺序1/2/10','空目录','ZIP附件/CBZ书','CBR/RAR不支持','大小写冲突/Unicode差异','打乱列表/MOBI/AZW3','重复执行','离线/局部失败保留','人工CATEGORY','人工系列成员/卷号','删除override','跨格式容器同编号','直属图片不擅自编号','正篇/特典/设定分区','EBOOK作者混合格式','损坏/未知/未验证文件','隐藏/临时/回收站既有策略','Root同名目录和PDF','组合混合格式与容器示例']
lines=['# SMART_MIXED Phase 1：案例矩阵','',
       '48 项要求全部覆盖，另加第49项组合验收。每个编号是独立 Rust test，读取同编号 JSON 中人工声明的 input/expected/negative_assertion；期望值不由识别器生成。',
       '', '每例精确断言：阅读来源路径/类型/页序、Root入口、系列数量及成员卷/章/角色/强度、指定目录角色、诊断、版本数、保留旧源数；另外统一检查 Root 固定、源/归属唯一、原物理项目可达。未列成系列的场景显式断言系列数0。',
       '', '| # | 场景 | 阅读单元 | Root入口 | 系列 | 判定/关键保护 | 结果 |','|---|---|---:|---:|---:|---|---|']
for c,title in zip(cases,titles):
    e=c['expected'];decisions=','.join(sorted({s['decision'] for s in e['series']})) or 'KEEP_SEPARATE / 按来源独立'
    if e['diagnostics']:decisions += '；'+','.join(e['diagnostics'])
    if e['editions']:decisions += f'；{e["editions"]}个REVIEW版本关联'
    lines.append(f'| {c["id"]:02} | {title} | {len(e["units"])} | {len(e["root_paths"])} | {len(e["series"])} | {decisions} | PASS |')
lines += ['', '## 负向要求与性质补充','',
          '每例完整负向要求保存在 `fixtures/NN.json` 的 negative_assertion；结构断言直接阻止错误合并、重复或丢失，而不是检查输出字符串是否“看起来正确”。', '',
          '`tests/invariants.rs` 另有35项测试：所有49例覆盖与唯一性；每例12次固定种子随机遍历顺序（588次整份Plan比较）；重复序列化；无环；Root隔离；全部视频类型拒绝；无外部元数据；人工覆盖影响范围/恢复；完整与不完整缺失保留；稳定引用；父/子页面不重复；非法页序/路径/链接；原生身份别名与冲突；人工图片单页/可信封面包含；中日韩封面名和中日英卷话；ARTBOOK保守策略；后缀不能提升ZIP；核心/剩余目录去重；元数据数字冲突；隐式父目录大小写冲突。', '',
          '第21例的不同Root引用隔离由额外性质测试执行；第36/37/41例也由整份Plan比较补充其结构断言。', '',
          '所有图片/书籍验证标志均为合成输入证据。没有制作空PDF假文件来声称真实解码成功；不是真实媒体验收。', '',
          '可直接CLI执行的三个 Snapshot 输入与实际输出为 `fixtures/example-{root-parallel,mixed-series,author-category}.{input,output}.json`。实际输出是审计记录，独立 expected 才是测试oracle；报告脚本不会用输出回写期望。']
(DOCS/'01_case_matrix.md').write_text('\n'.join(lines)+'\n',encoding='utf-8')

table='\n'.join(f'| {b["files"]:,} | {b["median_elapsed_ms"]:.2f} ms | {b["max_peak_working_set_bytes"]/1048576:.2f} MiB |' for b in v['benchmarks'])
report=f'''# SMART_MIXED Phase 1：执行结果与边界

记录日期：2026-10-09。本阶段完成，可供验收；尚未接入正式应用。

## 实际验证

| 检查 | 结果 |
|---|---|
| 独立 Cargo offline/locked 编译 | PASS，debug 与 release |
| `cargo fmt ... -- --check` | PASS |
| `cargo clippy ... --all-targets ... -- -D warnings` | PASS |
| 结构测试 | **49 PASS / 0 FAIL**（最低矩阵48项全覆盖 + 组合案例1项） |
| 性质/边界测试 | **35 PASS / 0 FAIL** |
| Rust测试合计 | **84 PASS / 0 FAIL / 0 ignored**，lib/bin/doc没有另行虚增测试数 |
| 实际release CLI对照golden投影 | **49 PASS / 0 FAIL**，另验证无效JSON退出码2 |
| 原始文件SHA保护 | 原390个基线文件无变化，见 scope-verification.json |

工具链：Windows x64 / MSVC，rustc 1.97.1 (8bab26f4f 2026-07-14)，cargo 1.97.1 (c980f4866 2026-06-30)。依赖是本地生产锁文件已使用的 serde 1.0.229、serde_json 1.0.151、regex 1.13.1；锁文件与编译配置独立。未新增生产依赖。

实际测试命令（从项目根执行）：

```powershell
cargo fmt --manifest-path tools/smart-mixed-lab/Cargo.toml -- --check
cargo test --offline --locked --manifest-path tools/smart-mixed-lab/Cargo.toml
cargo clippy --offline --locked --all-targets --manifest-path tools/smart-mixed-lab/Cargo.toml -- -D warnings
cargo build --release --offline --locked --manifest-path tools/smart-mixed-lab/Cargo.toml
python tools/smart-mixed-lab/scripts/verify_and_record.py
python tools/smart-mixed-lab/scripts/check_scope.py
```

本机 shell 的 PATH 未包含 Cargo，实际使用已安装 cargo.exe 完整路径并给子工具补 PATH。沙箱最初拒绝 Rust工具链访问，经过工具授权在宿主执行了上述独立crate检查；不是源代码编译失败。初版结构验证44/48，四个失败暴露图片卷被中间Work提前占有的问题，修正后全过；新增不同作品图片子目录、范围名误编号、隐式父目录大小写冲突保护后，再执行全部测试。期间一处测试代码借用冲突和一处Clippy useless_vec均已修正，没有留待下轮。

## 性能记录

来源：`verification.json`，记录时间 `{v['recorded_at_utc']}`。每档启动3个新的release进程，清单构建在计时外；计时只覆盖 recognize()（包括本次正则初始化与完整Plan生成）。峰值由 Windows K32GetProcessMemoryInfo 读取整个进程的 PeakWorkingSetSize，包含输入/输出/运行库，不包含JSON输出序列化。不是机器总内存或精确算法堆分配数。

| 独立文件数 | 识别耗时中位数 | 三次最大峰值工作集 |
|---:|---:|---:|
{table}

输出每档阅读单元与独立Work数都等于输入文件数。原始三次测量均保存在 JSON，可重现 `--benchmark 1000 / 10000 / 100000`。核心使用有序索引与父路径有界遍历，没有全对全模糊标题比较；在条目数n、最大路径深度d≤64、总页序p下近似 O((n+p)log(n+p)+n*d)，字符串长度受输入预算约束。

10万项完整计划仍约347 MiB峰值，说明当前全量模型需要生产适配时优化内存和取消；没有宣称“大库零成本”。本基准是平铺独立文件的合成清单，不代表真实磁盘、解码、极深目录、UI或网络耗时。CLI输入JSON64MiB和200,000条目是拒绝边界，未用10万项数据保证每种最坏形状都适用。

## 三个重点示例

1. **Root两系列 + 100独立PDF**：系列甲/第01、02卷；系列乙/第01、02卷；100个独立文件。结果104个阅读源，Root102入口（2Series + 100Work），不把Root当系列，也不把100文件吞入任一目录。
2. **混合系列**：第01卷PDF、第02卷CBZ、第03卷PNG图片组；第04–06卷容器里04/05/06.pdf；特典/附录.pdf。结果7个独立阅读源、1个Series；卷1–3直接证据，4–6裸数字在明确Work上下文中为REVIEW，附录Extra；整个系列REVIEW。图片直属页2张，范围目录不成为第6卷，核心容器不重复为剩余目录。
3. **藤本树作者分类**：再见绘梨.pdf、蓦然回首.pdf。结果Category入口1、独立Work/阅读源2、Series0。同人画师与EBOOK混合作者场景也保持独立。

对应可运行输入/完整实际输出见 fixtures/example-*.json；逐项48矩阵见 `01_case_matrix.md`。

## 明确不自动合并

Root多本无关书、作者/画师分类、不同作品名子目录、跨父目录/Root相似书名、单个显式卷、缺上下文裸数字、素材/单图、损坏/未知/未验证文件、ASCII路径冲突、Unicode疑似等价路径、人工Category/NoMerge。不同格式同卷仅版本关联，不共享进度；缺卷只报告，不造书。影片类型整体拒绝。

## 工作树与恢复

本轮开始前已有130条porcelain状态记录（包含既有tracked/untracked工作）；本轮新增范围是两个目录，不清空历史改动。当前HEAD/分支不变，无提交、推送、合并、版本变化或发布。

安全快照：`.tmp/smart-mixed-phase1/20261009-130953/worktree-before.zip`，390个已跟踪、非忽略未跟踪以及项目Skill文件；manifest逐文件SHA、原始before/after状态也在该目录。ZIP SHA-256：`8007ffaf1b8d3f1b78a8b72e6854c8b20e4a4b41bfd9222e0c6fd2686400b3d5`。

`check_scope.py` 逐项比对基线SHA并校验HEAD/branch，检查所有新增非忽略文件只能属于 tools/smart-mixed-lab/ 或 docs/smart-mixed/。Git的HEAD diff包含历史脏工作，不能把它全部归到本轮；逐文件快照才证明没有覆盖旧修改。完整新文件清单见 scope-verification.json。私有快照不应公开提交；恢复若需要应先逐文件审查，不能盲目覆盖后续工作。

生产 scanner/comics/db/commands/models/works/incremental、正式Cargo文件/迁移、App/Reader/Settings/CSS/i18n、AGENTS/Skills、旧交接/产品文档均未改。实验唯一辅助配置是独立Cargo.toml/Cargo.lock/[workspace]和自身target忽略，不影响正式构建。没有读写真实SQLite、封面缓存或用户媒体，没有Bangumi查询。

## 尚未验证与下一阶段风险

- 未运行正式程序或真实解码/库重扫：本轮明确禁止实际接入和数据变动；测试verified是合成已有验证证据。
- 未重跑生产npm/Rust全量gate、未打包发布：没有生产文件/配置改变，验收对象是独立crate；生产测试只审查代码，不能据本轮声称正式应用全量验收通过。
- 没有生产持久化override/group、stale扫描事务、真实移动/改名身份重匹配、全Unicode路径等价、媒体format安全适配器或SMART_MIXED UI。这些在 `02_integration_plan_draft.md` 列为未实施接入工作。
- 本地规则不能保证所有语义都正确；非典型语法/不明确标题保留为独立或复核；无全局模糊或LLM猜测。

本轮已经完成纯识别实验与文档，**停止在Phase1，等待验收**。
'''
(DOCS/'01_test_results.md').write_text(report,encoding='utf-8')
print('Wrote case matrix and execution report.')
