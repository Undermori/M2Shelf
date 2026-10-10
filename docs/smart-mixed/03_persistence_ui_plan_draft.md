# SMART_MIXED Phase 3：持久化与展示接入草案

**草案，未实施。Phase 2 交付后停止，等待用户审阅。** 当前没有新 Root 模式、逻辑组持久表、人工归组 UI、Tauri 接口、扫描接入或新增海报流程。本文件不构成执行 Phase 3 的授权。

## 1. 先决结论

Phase 2 已证明当前 schema 可以在单个只读快照中提供真实书籍/目录/资源身份、原页序、人工边界和扫描健康，并运行独立 Shadow。它没有证明：当前图片可以据索引获得 binary verified、10 万规模的完整内存结构适合产品后台、或新逻辑展示与既有 Node metadata 已经解决。

因此接入顺序不是把 `recognize()` 直接放入 scanner/UI。应先确定下列契约，然后才允许 additive migration 和新书籍 Root 的智能组织方式。

## 2. 第一步：补齐证据、查询与性能契约

### 2.1 图片验证证据

可选方向：由既有安全索引/读取边界提供与 book revision 和 page identity 绑定的验证记录，或明确将“既有 indexed image book”作为另一种可信度而非 binary verified。两者都需要专门设计与验收，不能让 Shadow 按 `.png` 推断真实格式。

如增加 attestation，应至少约束：验证版本、book revision、page locator/digest、实际 MIME/尺寸结果、失效规则。沿用现有格式、尺寸、字节、Root、opened handle、archive CRC 限制，不允许为归组重扫整盘、跳过 reader validation 或写入媒体。

未补齐之前，图片 physical fallback 是合法用户可达对象，不应被隐藏以凑“完整归组”。不应把 PNG 问题与本轮 binary evidence gap 混为一谈；生产已支持图片，缺的是只读索引能证明什么。

### 2.2 单一读模型与版本

将 Phase 2 受测 ownership、Resource 去重、readingFiles 集合投影整理为生产可复用的 Rust 读模型或等价共享契约；保留前端当前展示集合兼容。不要让 scanner、detail、catalogue、matching、cover warmup 各自再实现一套 owner。

设计 Root 级单调 index revision 与独立 override revision，在物理索引事务成功提交后更新；人工边界、绑定变更等影响归组的写入也必须使相关 revision 失效。规则版本独立保存。

后台读取 `(root_id, index_rev, override_rev, rules_version)`，计算结束后的写事务重新比较 CAS，过期结果拒绝；不能用“任务还没取消”或 Phase 2 内容摘要单独替代。

### 2.3 批次、取消与内存

100k 最终实测完整流程约 1.81 秒、约 514 MiB 工作集，虽未复制四份 JSON，仍需优化后再嵌入。先确定实际规模/预算，再做字符串共享、compact source map、按锚点差异输出、分页返回、单 Root 后台任务与 bounded queue。保持全部身份不变量。

增加 recognizer 内部可取消检查点；保留确定性，避免取消留下半份逻辑持久状态。后台准备只返回进度/摘要到 WebView，不发送整份十万项 Plan。

## 3. 第二步：新 Root 识别策略与 additive schema 评审

仅为**新建书籍家族 Root**考虑 SMART_MIXED。既有 FOLDER/VIDEO_FILE 必须保持原语义、原不可变模式和全部旧索引身份；视频 Root 完全绕开新策略。

当前 `recognition_mode` 的历史 CHECK/trigger 只允许 FOLDER/VIDEO_FILE。不能只加 Rust enum，让 DB 不接受；也不能使用未知值 fallback Folder 掩盖迁移问题。可比较两条设计路径：

1. **优先评估新增正交、不可变的书籍组织策略字段**。SMART_MIXED 作为新书籍 Root 的逻辑组织策略，物理识别/来源身份仍遵循明确定义的现有底层模式。需要解释 UI 选项与底层字段关系，避免两个模式歧义。
2. 如果坚持扩展既有 recognition_mode CHECK，需专门评审 SQLite Root 表的外键安全重建、trigger 恢复、所有相关 FK 以及旧数据库升级。它比加字段风险更高，不得顺手 DROP/重建或关闭 FK 后假称安全。

选择路径需要用户/产品验收，不在 Phase 2 决定或实施。无论哪条路径，数据库旧 Root 不自动改为智能，All Resources flattened 设置也不能改 Root 策略。

候选逻辑表（仅设计，当前不存在）：

| 候选关系 | 最小契约 |
|---|---|
| logical groups | 稳定 group PK、Root FK、kind、可空真实 anchor Node、local grouping key、规则/索引/覆盖版本、decision/stale/provenance |
| logical members | group FK + 真实 ComicBook FK，role、volume/chapter、origin；当前主归属唯一，Root 一致 |
| edition links | 不合并 book 的有序唯一 pair、relation、decision、origin；多格式/合集各有独立进度 |
| recognition overrides | 指向真实 Node/book 或既有 group 的明确目标、manual role/member/volume/no_merge、revision；跨 Root 拒绝 |
| validation evidence（如采用） | 与真实 source/page identity、revision、验证版本绑定；不改变媒体 |

Phase 1 的 proposal_ref/source_ref 是实验引用，不能直接拿作持久 PK。虚拟 Series 没有磁盘目录，不能造 absolute_path。具体 FK、约束、cleanup 和 stable key 规则必须先有迁移测试，再写 migration 编号。

迁移必须在真实 schema 工厂上从旧版本演进测试，保留原 book/Node/Resource ID、binding、manual cover/name/type、progress/bookmark、favorites/tag/history/settings。不把“新表没有丢数据”当成充分证据；还要验证既有行/外键与升级中断。

## 4. 第三步：后台计算与事务持久化

建议顺序：

1. 原 scanner 按原模式完成物理索引；沿用 Root canonical/取消/局部失败/ancestor refresh 边界。
2. 只对显式启用的新书籍 Root 发出单 Root 后台识别任务；由 SQLite 一致读模型取证据，不再次枚举媒体。
3. 在后台形成 compact plan，并验证每个真实 book ID 恰好提案或物理回退一次；人工/绑定/ignored fence 优先。
4. 写入事务 CAS 确认 Root/index/override/rules 版本；一次替换该 revision 的自动组关系，保留 manual overrides 和所有物理来源。
5. 成功后发布一次 typed refresh；旧 async UI loader generation 不得覆盖更新结果。

部分失败/离线/取消：保留上一份逻辑结果并标 stale，或只更新有明确完整性证明的范围。绝不把未观测成员解释为应删除/解绑；保留物理导航兜底。人工操作、重扫与 worker completion 的相互时序必须有并发测试。

不让新 group 写入反向改变 NodeType、文件名、路径、book ID 或普通 matcher 的候选边界。

## 5. 第四步：人工覆盖、绑定与 Node 元数据

通过 typed Rust commands 校验整批选集、Root 和版本，事务写入；恢复自动只清指定 override，不清现有绑定/封面/标签/收藏/进度。

需要先明确：FOLDER 中多个独立 books 共用一个旧 Node 时，Node 级标签/收藏/绑定如何显示。不得将其自动复制或任意拆分到每个新 Work；虚拟 Series 也不能无来源地继承所有成员 metadata。

人工或不同 binding 的子 edition 始终保留独立边界；同 binding 后代在当前生产 detail 可穿透，Shadow 暂保守 fence。正式规则需明确兼容方式并有 fixture；不能悄悄把 Shadow 保守策略当新的永久产品规则。

本地归组不依赖 Bangumi。四种书籍 scope 仍为 type 1，Root auto_bangumi 独立；DOUJIN/ARTBOOK 的 opt-in exact-title safeguard、manual cover 优先、匹配预算、失败封面重试仍不变。已折叠卷/图片/格式目录不新增独立匹配或 warmup。

文件移动/改名、同哈希副本和跨盘身份重匹配继续独立范围；没有 Windows File ID 生命周期与审查契约之前，不自动迁移阅读状态或绑定。

## 6. 第五步：typed Browse/Detail DTO 与 UI 接入

先实现受测集合 API，再考虑视觉：

- Root 同级展示真实 Series 锚点、独立文件 Work、Category，以及必要的物理 fallback。例如 2 Series + 100 PDF 是 102 入口/104 来源。
- 系列读取 explicit member book IDs；格式/范围 Intermediate 可以穿透，但真实来源位置可查。不同作品/edition/绑定保留导航入口。
- 同一个 SQLite read snapshot 产出 reading table、未展开 children、remaining attachments 和 hero count；核心来源/图片页只出现一次。
- 未打开的 CBZ/PDF/EPUB/TXT/MOBI/AZW3 ResourceFile 保留资源身份并进入共享阅读表，打开也不改变核心计数；README/普通 ZIP/未知文件留在 Other。
- 所有阅读动作继续以真实 book/Resource ID 调用原安全 reader，逻辑组变动不移动进度/书签。文本章节/稳定 byte segment 位置保持。
- 新虚拟组的 Explorer 动作必须选择具体真实 source；无磁盘路径的组不能传 group key 给 Shell。
- 返回/Alt+Left/history、独立目的地 snapshot、筛选、排序、grid/list、typed i18n、system/light/dark 和旧 settings store 继续保留。
- 海报仍使用已准备持久本地缩略图与同一 image/frame；不因智能归组恢复滚动重绘、Canvas 换图或目录重复封面准备。

UI 与 Skill 调整不属于本轮。后续若用户授权正式 UI 工作，加载当时的 Skill 并结合用户选择更新规范；不要把当前 Sidebar 尺寸等历史外观误认作本草案新增强制条件。

## 7. 第六步：回滚、测试与验收顺序

| 顺序 | 必须证明 |
|---|---|
| 1. 证据/读模型 | 图片 attestation 或可信度契约、精确 production SQL parity、Resource 兼容、全部 source ID/页序不变量 |
| 2. 性能与任务 | 大 Root 内存预算、阶段取消、队列/并发、无重复完整 JSON/DTO、stale completion 拒绝 |
| 3. migration | 旧 DB/FK/原身份/所有状态保留、不可变新 Root policy、失败回滚/重开 |
| 4. 事务写入 | CAS、人工覆盖优先、跨 Root 拒绝、局部失败/离线/取消保留 |
| 5. 查询契约 | Browse/Detail/All Resources/匹配/海报边界一致；Root 不成为一部书，混合系列与 Category 集合准确 |
| 6. UI | 四语言、两解析主题及系统切换、最小/默认/最大窗口、long titles、返回/过滤/计数/Reader action |
| 7. 全应用 gate | typecheck/build/validate、Rust fmt/test/clippy；旧视频、PDF/EPUB/TXT/Kindle/附件阅读/更新器必要回归 |
| 8. 显式授权真实验收 | owner 允许的真实索引只读预览，随后受控 native/媒体验收；区分 fixture 与实际用户验收 |

逻辑层回滚应关闭新策略/恢复物理展示，保留物理索引、人工覆盖、book IDs 和阅读状态，不通过删除用户数据实现“回退”。已创建新字段/表不作破坏性 down migration；重开或规则升级能够安全重建自动提案。保留旧模式分支直到完整验收。

本轮未执行以上任何 Phase 3 步骤。后续发行、签名、发布还需要单独明确授权。

## 8. 需要后续决定的事项

1. 新书籍 Root 的组织策略采用正交字段还是扩展旧模式；新 UI 如何清楚表达。
2. 图片书在没有持久 binary attestation 时应保持 fallback，还是增加受 revision 约束的独立可信度。
3. 多本共享 Node 的绑定/标签/收藏与虚拟 group metadata 的明确继承/编辑规则。
4. 同绑定 descendant 的正式兼容边界。
5. 后台识别的规模、内存与取消响应目标，以及允许的初始人工预览流程。

这些是下一阶段设计/验收项，不是要求用户现在提供个人 DB 或允许本轮继续上线。
