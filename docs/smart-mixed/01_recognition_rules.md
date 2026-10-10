# SMART_MIXED Phase 1：纯识别规则

实现入口：`tools/smart-mixed-lab/src/recognize.rs:160` / `recognize(&Snapshot) -> Result<Plan, String>`。数据模型位于 `src/model.rs:129` / Snapshot、`:255` / Plan；辅助证据在 `src/signals.rs`。这些位置属于独立实验 crate，未被生产 `mod`、Tauri command 或 Cargo workspace 引用。

## 分层与决策

1. **PhysicalEntry**：Root-relative path、父路径、种类和状态。Root `""` 固定为 Root；输入路径全部保留，缺少父目录只补结构并诊断，不猜出一本书。冲突会在输出中标为 Excluded 隔离，原路径实体仍在。
2. **ReadingUnit**：FileBook 或 DirectPages。一本有效文件与它是否属于系列分别处理。图片组只含直属页，文件书不会按每页产生阅读单元。
3. **Group/Member/EditionProposal**：Work/Series 建议，成员含 Main/Volume/Chapter/Collection/Extra、卷/话、证据、强度；版本只做关系建议，阅读源始终独立。
4. **DisplayItem/DetailProposal**：根入口、详情阅读来源、穿透目录和剩余目录。物理树仍可用于回到原路径，核心内容不会再作为剩余目录重复建议。

决策枚举：`APPLY` 表示满足本轮确定规则；`REVIEW` 表示需要复核；`KEEP_SEPARATE` 表示保持独立/分类 fallback。它们只是原型建议，不会驱动应用写入。一个有效独立文件可以是 APPLY 阅读单元，同时所属 Work 是 KEEP_SEPARATE；这表示“已成书，尚不归并”，没有矛盾。

无网络评分、模糊全局匹配或任意百分比阈值。

## 规则号

| 规则 | 行为与证据 |
|---|---|
| B01 | 已验证、支持的文件书独立成书；PDF/EPUB/TXT/MOBI/AZW3/CBZ 要求后缀和已检测格式相符。ZIP/CBR/RAR 不由改名或格式标签直接提升 |
| I01 | 直属、可访问、已验证图片 + 完整可信页序 + 编号连续命名或明确阅读语义才成图片书；不加入子目录页 |
| D01 | 固定 Root；作者/画师/出版社为 Category；特典资料为 Extras；格式/范围/卷话组织目录为 IntermediateContainer；缺证据保留 Ambiguous |
| C01 | 同一目录多个独立书籍，缺少兼容系列证据时保留 Category |
| N01 | 中/日/英文显式卷话标记或可信元数据提供卷/话；Extras/Collection 不成为正篇编号 |
| N02 | 只有数字的名称需要明确 Work/Series/可信阅读上下文，作为 REVIEW 卷候选；Root 裸数字不编号 |
| S01 | 非 Root 命名目录中，穿透纯组织目录后至少两个不同卷/话位置、兼容标题 → Series；一份无卷号直属图片书和明确子卷共存可提 REVIEW，不补成第1卷 |
| S02 | 同一物理父目录、同一精确标题前缀/可信 series 元数据、至少两个显式不同卷话位置 → 虚拟 Series；不跨父目录/Root 模糊归组 |
| W01 | 没有系列证据、或被人工排除时，每个阅读源保持独立 Work |
| W02 | 明确 Work 上下文可以关联本目录阅读内容，素材目录仍独立导航；不将不同作品名子目录吞进父 Work |
| M01 | 人工目录角色及卷话优先；人工 Category/Ambiguous/NoMerge 抑制自动归组；人工图片书也不能绕过格式、可达和页序检查 |
| M02 | 明确 source→series_directory 的人工归属先执行；只在本 Root 的有效目录之间，冲突给诊断并保守保留 |
| E01/E02 | 同位置/同精确 sibling 标题的多个阅读源只是 REVIEW 版本候选，不合并文件、ID、进度或书签；单位置多格式不会强行变成多卷系列 |

`signals` 识别 `Vol.01`、`Volume 2`、`第十二卷`、`2巻`、`Ch.003`、`第03话/第3話/第十章`。`第04-06卷` 是范围容器，不提取末尾数字为册号。`20世纪少年`、`86`、`3月的狮子` 无显式标记不编号。

## 证据顺序与冲突

人工角色/归属/编号 → 可信书籍元数据与既有读取页序 → 明确卷话/格式目录语义 → 编号命名 → 保守独立/分类。

人工 override 不提供磁盘访问许可。角色冲突的多个 override 会变为 Ambiguous+NoMerge，不取“最后写入赢”；丢失目标会诊断，无效系列目标/Root 角色拒绝。来源 metadata 卷号与名称显式卷号冲突时保留来源并输出 `NUMBER_EVIDENCE_CONFLICT`，相关自动组为 REVIEW。

格式和页序 evidence 是输入契约，不能自证安全。未来适配器必须从现有 Rust 索引/格式读取提供，不可由前端或未验证后缀设置 verified。

## 图片具体规则

- 默认排除 cover/front_cover/folder/thumb/thumbnail/poster/封面/カバー/표지（大小写兼容、允许编号后缀）。只有 TrustedBookMetadata + include_covers 明确要求才能把辅助封面加入正文。
- wallpaper/keyvisual/宣传图/素材/壁紙/原画或 Assets hint 保留为素材。纯编号图片也不是唯一规则：必须有已验证完整页序，同一命名前缀、编号严格递增，或明确阅读语义。
- 单图默认 `SINGLE_IMAGE_REVIEW`，不自动成书；人工 BookImageDirectory 可允许单张已验证页。
- ARTBOOK 比其他书籍更保守：编号页还需 Work/ArtPages、明确卷号或可信书籍元数据。DOUJIN 不因画师目录共有作者而归组。
- `1,2,10` 按调用方提供的现有自然顺序保留；不再用字符串排序。无序/不完整/后代/重复页序不自行修复。
- 父目录直属页与子卷分别成 ReadingUnit；子目录不同作品名则保持独立，不因为也是图片书就归入父作品。
- 阅读页的格式来自验证的真实签名，允许 JPEG 内容命名 PNG；实验不读取字节，不重写格式解码。

## 唯一性、范围和稳定引用

源引用是 JSON 元组 `[root_id, FILE|DIRECT_PAGES, path-key|native-identity]`；没有哈希碰撞，也不是临时数组下标。默认 path-key 使用分隔规范化和 ASCII NOCASE，与生产 SQLite 基线一致。可选 native identity 只由调用方提供。相同可信身份的兼容文件合为一个 ReadingUnit 并记录 aliases；身份与格式/图片页集合相互冲突则整体失败。

不做 Unicode NFC/NFD 等价猜测；两个表面相同但编码不同的路径，未提供原生等价身份时保持不同。ASCII 同名大小写碰撞隔离并诊断，不默选一个；单一路径只有大小写/分隔变化时源引用保持稳定。

Work/Series proposal_ref 是组织键，不得写成生产书 ID。组变化可以改变 proposal_ref；相同源不因此改变 source_ref。没有跨 Root、作者、目录或相似书名全局去重，没有内容摘要判重。

## 安全 fallback / diagnostics

诊断包含：PATH_CASE_COLLISION、DUPLICATE_ENTRY、INFERRED_PARENT、OVERRIDE_TARGET_MISSING、CONFLICTING_OVERRIDE、ROOT_OVERRIDE_REJECTED、MANUAL_MEMBERSHIP_CONFLICT、LINK_NOT_FOLLOWED、SOURCE_UNAVAILABLE、EXCLUDED_EXISTING_POLICY、UNSUPPORTED_OR_UNVERIFIED_FILE、COVER_EXCLUDED、IMAGE_ASSETS_ONLY、SINGLE_IMAGE_REVIEW、MISSING_OR_INVALID_PAGE_ORDER、CONFLICTING_PAGE_ORDER、INVALID_IMAGE_EVIDENCE、IMAGE_CONTEXT_REVIEW、NATIVE_ID_ALIAS、NUMBER_EVIDENCE_CONFLICT、VOLUME_GAP 和 prior 保留诊断。

无效相对路径、非目录父节点、视频库、超限、冲突源身份等返回 Err；CLI 输出 JSON error，退出码2。类型/格式不支持但结构有效的文件保持物理可达并给诊断。隐藏/临时/回收站名不新增排除规则，沿用当前四个明确名称。

缺卷只报告已观测区间内缺口（最多64条），不猜第1卷之前是否应存在。旧源没出现时完整/局部/离线都不产生删除命令；本阶段 `retention_policy` 永远为 PROPOSAL_ONLY_NO_DELETIONS_OR_PERSISTENT_MUTATIONS。

## 已知边界

这是可复用的确定规则实验，不是 100% 语义理解。没有 CJK 全角数字/罗马数字/所有语言卷名、复杂副标题与多重冲突标记的完整语法；缺证据宁可保留。数值页缺号不证明页面损坏，也不自行补页。人工指定一个目录为系列与显式成员指定可验证，但没有多用户编辑、撤销持久化或绑定冲突的生产事务。

原型展示顺序为稳定字典键序，用于可重复结果，**不是未来 UI 的自然卷册排序实现**；页内顺序保持现有索引。外部 source revision、CRC、打开句柄、可信元数据提取、取消和事务边界仍由既有生产模块承担。本轮没有执行这些边界的真实媒体验收。
