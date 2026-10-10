# SMART_MIXED Phase 2 接入草案（未实施）

本文件只有接口和迁移建议。当前应用仍只有 FOLDER/VIDEO_FILE，没有 SMART_MIXED 创建选项、Tauri command、数据库表或自动改库。本轮到 Phase 1 停止。

## 1. 最小接入位置

在书籍分支保持 `scanner.rs:476` / `run_scan_inner` 的 Root/canonical/取消边界；由 `comics.rs:503` / `scan_library` 在 **现有物理索引完成后**，从同一 Root 的索引生成只读 Snapshot，再调用纯识别器。不要在 `ComicScan::directory` 每见一项就合并、改 NodeType 或重扫子树；会引入遍历顺序依赖、祖先局部更新和半成功清理问题。

先复用当前 `ComicBook` / `comic_pages` / ResourceFile / Node 清单形成适配器。保留既有 `store_book_rows` 身份与 revision 更新，在其后组织逻辑关系。需要分析但尚未成为核心书的素材/未知/不可读项，附状态从索引带入，不再解码或枚举磁盘。页序使用既有 page_index，自然顺序证据不能由原型另造。

首次真实接入应只做 shadow 结果比较和显式预览，不改变所有权；确认事务、身份、部分失败和 UI 承载契约后再允许新 SMART_MIXED 库采用。不可将这个提案当作本轮授权继续实施。

## 2. 与现有模型的映射

| 实验层 | 可复用现有字段/对象 | 缺口或注意 |
|---|---|---|
| PhysicalEntry | library_root_id、Node.id/parent_node_id/absolute_path、ResourceFile.id、路径/元数据/状态 | Root-relative 路径由 Rust 适配器从受检索引路径得到，不从 UI 接受任意绝对路径 |
| FileBook/DirectPages | ComicBook.id/source_path/source_kind/reader_format/revision、comic_pages.page_index | source_ref 仅映射索引来源；绝不能替换 ComicBook.id |
| Series/Work | 已有物理目录 Node 可作为展示锚点；绑定、封面、标签、收藏继续附原 Node | 虚拟 Series 没有真实目录，不能虚构 absolute_path 或混用 NodeType |
| Category/Intermediate/Extras | 保留真实目录树，添加逻辑角色作为可派生注释 | Category 不等于 IGNORED；容器不一定需要新的 UI 必经层 |
| Volume/Chapter/Edition/Collection | 可用现有 ComicBook 作为成员目标 | 现有 schema 缺少显式逻辑组/成员/版本/override 关系 |
| Presentation | NodeDetail/comic_books/children/resource_files 现有返回契约 | 必须同一个 SQLite read_snapshot；不可独立异步拼接旧/新方案 |

物理 FOLDER Node 树与 VIDEO_FILE 扁平 Node 树仍按原模式生成。未来新 SMART_MIXED 库需要稳定文件/目录来源身份，不能先改旧库模式再重建 Node；尤其 FOLDER 中多本书共享一个 Node，其历史绑定/收藏不能随意分摊给新文件卡片。

## 3. 真正缺少的持久化字段

建议在未来单独审查的 additive migration 中建立以下最小关系（不是本轮创建的表）：

- `book_logical_groups(id, library_root_id, anchor_node_id NULL, local_key, kind, title, rules_version, snapshot_revision)`：真实整数 ID 与 Root 内稳定 local_key；锚点只关联已有真实目录；虚拟系列不造路径。Root+local_key 唯一。
- `book_logical_members(group_id, comic_book_id, role, volume_number NULL, chapter_number NULL, decision, origin)`：主归属约束保证同册只属于一个当前逻辑组，成员去重由真实 book ID；章节/卷不得覆盖书 ID。
- `book_edition_links(left_book_id, right_book_id, relation, origin)`：规范有序键去重，独立阅读状态，默认建议复核。
- `book_recognition_overrides(library_root_id, source_node_id NULL, comic_book_id NULL, role NULL, target_group_id NULL, volume_number NULL, chapter_number NULL, no_merge, updated_at)`：一个明确目标，检查 Root 一致，人工覆盖独立于自动结果。
- Root 的模式 CHECK/不可变 trigger 若未来允许 SMART_MIXED，需要明确保留旧 CHECK/外键的迁移方案和新库限定；不能只改 Rust enum 或让 unknown fallback Folder。

这些是具体候选字段，仍需依据 shadow 数据评估能否减为派生查询、是否需要 group revision/实体生命周期，不承诺已经是最终 schema。没有现有表删除、没有旧行数据迁移，本阶段没有编号新的 migration。

## 4. 人工覆盖与稳定身份

覆盖写入应由新的 typed Rust command 校验目标集合/Root/snapshot 后，在一次事务里操作；重扫只更新自动提案，manual origin 最高优先级。“恢复自动”只清指定覆盖并重算受影响 Root/锚点，不清绑定、封面、书签或收藏。

保留 `ComicBook.id`、`source_resource_id` 兼容关联、revision 和页面定位；绑定、手工封面、标签、收藏、历史继续使用原身份和外键。逻辑组变化不迁移阅读进度，不把同卷多格式视为同一本。文本进度继续章节/稳定 byte segment，图片按已验证页面。

移动/改名、跨盘、同内容副本的自动身份重匹配**未解决**：当前常规身份依赖路径/Node，实验的可选 native identity 只验证接口可表达，未采集 Windows File ID，更未处理 inode/File ID 重用。不能内容哈希相同就合并，也不能未经显式确认移动绑定/状态。未来先保留缺失旧来源，新增来源独立，再提供审查方案；掉盘/局部失败不触发缺失删除。

## 5. 所有权、附件和导航

新逻辑层必须先满足当前 `comics::owned_nodes` 的 IGNORED、人工类型、不同绑定边界，再考虑语义 Category/不同作品。`:re`、独立具体版不因共享前缀吞入正篇；需要专门真实 fixture 和既有 continuation 回归。

未来 `populate_detail` 可在当前只读快照里按显式 member book IDs 返回阅读表，并计算被展开祖先；继续使用 `db::list_resources_for_nodes_conn` 的核心来源/图片页排除。核心文件和已展开图片/格式容器不再列在附件，内部 readme/未知附件仍存在；hero 数量等于实际阅读表。原型 `DetailProposal` 只证明此集合可表达，尚未替换 SQL。

根显示系列锚点、独立文件、Category 并列：实例为2系列+100独立PDF→102入口/104来源。纯范围/格式目录可从系列详情穿透列册，原路径仍存在并由原生 Shell 查看；不同作品/不同绑定保留入口。不能把虚拟 Series 的组织键当 Explorer 路径。

## 6. 与旧模式、匹配策略隔离

FOLDER、VIDEO_FILE 继续原语义。未来只为新建书籍库增加模式，不强制升级旧库。All Resources 的 `all_resources_flattened` 仍只影响该页，不改 Root 模式。

COMIC/EBOOK/DOUJIN/ARTBOOK 的本地成书和归组不依赖 Bangumi。`auto_bangumi` 是独立设置；当前 DOUJIN/ARTBOOK 默认关闭但用户可主动启用，仍按现有 type1 和硬冲突规则。已折叠的册/格式容器不应突然单独进入匹配、海报生成/诊断/重试。没有授权自动回写已有绑定或手工封面。

ANIMATION/LIVE_ACTION/兼容 VIDEO 的分支完全绕过本识别器；保持 BDMV、supplement、视频聚合、播放器和现有 matcher 预算。新规则不能根据电影文件夹名称“智能”改变其行为。

## 7. 事务、失效和性能风险

Snapshot 应带实际 Root/index revision、规则版本、override revision；识别结果写入前重新校验，拒绝过期 worker 完成。当前实验的 root_id/source_ref 不能替代这些生产校验。

完整成功扫描才允许按原规则清理物理旧索引；部分失败仅重算可靠覆盖范围或保留上次逻辑组并打 stale 标记，不把未观测成员清空。规则版本升级不消除人工决定。掉盘恢复、取消、同一 Root 排队重扫沿用当前生命周期互斥。

实验10万来源峰值约347 MiB，采用完整结构化输出，不能直接在 UI 线程同步执行或无界复制多份清单。生产适配前需要分页/受控批次、去重字符串、索引变更局部重算、可取消后台执行和实测；维护确定性、Root 范围及稳定源身份优先。不可用无限并发或全对全模糊匹配替代规则。

## 8. 后续验收顺序（仅建议）

1. 适配器单测：真实索引→Snapshot 的 Source ID、格式、页序、人工/绑定边界；审计源只读及无新磁盘枚举。
2. shadow 样本：复用本轮49结构goldens和现有漫画/Kindle/文本测试；对照逻辑结果，无 DB mutation。
3. migration 升级/回滚演练：旧库原样、外键、同册ID、书签/进度/收藏/绑定/封面；人工覆盖事务和 stale completion。
4. 错误注入：局部目录失败、外接盘离线、取消、限额、模式不可变、Unicode/链接/句柄/CRC 边界、旧附件升级。
5. 正式 UI 集成后再做四语言两主题/默认和最小窗口、Root并列/穿透/返回/筛选/计数/去重、真实 WebView 和书籍解码验收。
6. 全量生产 gate 和用户提供目录的显式验收，确认以后才讨论发行；本轮不构建正式包或执行这些步骤。

本轮没有因提前接入而“顺手”修改模式、扫描器、数据库或 UI。
