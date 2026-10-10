# SMART_MIXED Phase 1：案例矩阵

48 项要求全部覆盖，另加第49项组合验收。每个编号是独立 Rust test，读取同编号 JSON 中人工声明的 input/expected/negative_assertion；期望值不由识别器生成。

每例精确断言：阅读来源路径/类型/页序、Root入口、系列数量及成员卷/章/角色/强度、指定目录角色、诊断、版本数、保留旧源数；另外统一检查 Root 固定、源/归属唯一、原物理项目可达。未列成系列的场景显式断言系列数0。

| # | 场景 | 阅读单元 | Root入口 | 系列 | 判定/关键保护 | 结果 |
|---|---|---:|---:|---:|---|---|
| 01 | Root单PDF | 1 | 1 | 0 | KEEP_SEPARATE / 按来源独立 | PASS |
| 02 | Root 300独立PDF | 300 | 300 | 0 | KEEP_SEPARATE / 按来源独立 | PASS |
| 03 | 2系列+100独立PDF | 104 | 102 | 2 | APPLY | PASS |
| 04 | 东京食尸鬼显式卷册 | 2 | 1 | 1 | APPLY | PASS |
| 05 | PDF+CBZ | 2 | 1 | 1 | APPLY | PASS |
| 06 | PDF+EPUB+TXT | 3 | 1 | 1 | APPLY | PASS |
| 07 | 文件册+图片卷 | 3 | 1 | 1 | APPLY | PASS |
| 08 | 范围容器内裸数字卷 | 3 | 1 | 1 | REVIEW | PASS |
| 09 | 多层卷/话/图片 | 2 | 1 | 1 | APPLY | PASS |
| 10 | 作者多个不同作品 | 2 | 1 | 0 | KEEP_SEPARATE / 按来源独立 | PASS |
| 11 | 同人画师分类 | 2 | 1 | 0 | KEEP_SEPARATE / 按来源独立 | PASS |
| 12 | 设定集+原画素材 | 1 | 1 | 0 | KEEP_SEPARATE / 按来源独立；IMAGE_ASSETS_ONLY | PASS |
| 13 | 数字书名20世纪少年 | 1 | 1 | 0 | KEEP_SEPARATE / 按来源独立 | PASS |
| 14 | 数字书名86 | 1 | 1 | 0 | KEEP_SEPARATE / 按来源独立 | PASS |
| 15 | 根文件精确前缀虚拟系列 | 2 | 1 | 1 | APPLY | PASS |
| 16 | 仅一个显式卷 | 1 | 1 | 0 | KEEP_SEPARATE / 按来源独立 | PASS |
| 17 | Root裸数字文件 | 2 | 2 | 0 | KEEP_SEPARATE / 按来源独立 | PASS |
| 18 | 明确作品内裸数字 | 2 | 1 | 1 | REVIEW | PASS |
| 19 | 缺第02卷 | 2 | 1 | 1 | APPLY；VOLUME_GAP | PASS |
| 20 | 同卷多格式版本 | 2 | 1 | 0 | KEEP_SEPARATE / 按来源独立；1个REVIEW版本关联 | PASS |
| 21 | 同名不同父目录/Root | 2 | 2 | 0 | KEEP_SEPARATE / 按来源独立 | PASS |
| 22 | 全集与分卷并存 | 3 | 1 | 1 | APPLY | PASS |
| 23 | 特典番外目录 | 3 | 1 | 1 | APPLY | PASS |
| 24 | 角色设定资料 | 3 | 1 | 1 | APPLY | PASS |
| 25 | cover排除 | 1 | 1 | 0 | KEEP_SEPARATE / 按来源独立；COVER_EXCLUDED | PASS |
| 26 | wallpaper/keyvisual/poster | 0 | 1 | 0 | KEEP_SEPARATE / 按来源独立；IMAGE_ASSETS_ONLY,COVER_EXCLUDED | PASS |
| 27 | 图片书与readme附件 | 1 | 1 | 0 | KEEP_SEPARATE / 按来源独立；UNSUPPORTED_OR_UNVERIFIED_FILE | PASS |
| 28 | 父图片书与不同作品子图片书 | 2 | 1 | 0 | KEEP_SEPARATE / 按来源独立 | PASS |
| 29 | 父图片与子文件卷 | 2 | 1 | 1 | REVIEW | PASS |
| 30 | 单图片 | 0 | 1 | 0 | KEEP_SEPARATE / 按来源独立；SINGLE_IMAGE_REVIEW | PASS |
| 31 | 自然顺序1/2/10 | 1 | 1 | 0 | KEEP_SEPARATE / 按来源独立 | PASS |
| 32 | 空目录 | 0 | 1 | 0 | KEEP_SEPARATE / 按来源独立 | PASS |
| 33 | ZIP附件/CBZ书 | 1 | 1 | 0 | KEEP_SEPARATE / 按来源独立；UNSUPPORTED_OR_UNVERIFIED_FILE | PASS |
| 34 | CBR/RAR不支持 | 0 | 0 | 0 | KEEP_SEPARATE / 按来源独立；UNSUPPORTED_OR_UNVERIFIED_FILE | PASS |
| 35 | 大小写冲突/Unicode差异 | 2 | 2 | 0 | KEEP_SEPARATE / 按来源独立；PATH_CASE_COLLISION,SOURCE_UNAVAILABLE | PASS |
| 36 | 打乱列表/MOBI/AZW3 | 2 | 1 | 1 | APPLY | PASS |
| 37 | 重复执行 | 2 | 1 | 1 | APPLY | PASS |
| 38 | 离线/局部失败保留 | 0 | 1 | 0 | KEEP_SEPARATE / 按来源独立；PRIOR_RETAINED_INCOMPLETE,SOURCE_UNAVAILABLE | PASS |
| 39 | 人工CATEGORY | 2 | 1 | 0 | KEEP_SEPARATE / 按来源独立 | PASS |
| 40 | 人工系列成员/卷号 | 2 | 1 | 1 | APPLY | PASS |
| 41 | 删除override | 2 | 1 | 1 | APPLY | PASS |
| 42 | 跨格式容器同编号 | 3 | 1 | 1 | APPLY；1个REVIEW版本关联 | PASS |
| 43 | 直属图片不擅自编号 | 2 | 1 | 1 | REVIEW | PASS |
| 44 | 正篇/特典/设定分区 | 4 | 1 | 1 | APPLY | PASS |
| 45 | EBOOK作者混合格式 | 3 | 1 | 0 | KEEP_SEPARATE / 按来源独立 | PASS |
| 46 | 损坏/未知/未验证文件 | 0 | 0 | 0 | KEEP_SEPARATE / 按来源独立；SOURCE_UNAVAILABLE,UNSUPPORTED_OR_UNVERIFIED_FILE | PASS |
| 47 | 隐藏/临时/回收站既有策略 | 2 | 2 | 0 | KEEP_SEPARATE / 按来源独立；EXCLUDED_EXISTING_POLICY,UNSUPPORTED_OR_UNVERIFIED_FILE | PASS |
| 48 | Root同名目录和PDF | 3 | 2 | 1 | APPLY | PASS |
| 49 | 组合混合格式与容器示例 | 7 | 1 | 1 | REVIEW | PASS |

## 负向要求与性质补充

每例完整负向要求保存在 `fixtures/NN.json` 的 negative_assertion；结构断言直接阻止错误合并、重复或丢失，而不是检查输出字符串是否“看起来正确”。

`tests/invariants.rs` 另有35项测试：所有49例覆盖与唯一性；每例12次固定种子随机遍历顺序（588次整份Plan比较）；重复序列化；无环；Root隔离；全部视频类型拒绝；无外部元数据；人工覆盖影响范围/恢复；完整与不完整缺失保留；稳定引用；父/子页面不重复；非法页序/路径/链接；原生身份别名与冲突；人工图片单页/可信封面包含；中日韩封面名和中日英卷话；ARTBOOK保守策略；后缀不能提升ZIP；核心/剩余目录去重；元数据数字冲突；隐式父目录大小写冲突。

第21例的不同Root引用隔离由额外性质测试执行；第36/37/41例也由整份Plan比较补充其结构断言。

所有图片/书籍验证标志均为合成输入证据。没有制作空PDF假文件来声称真实解码成功；不是真实媒体验收。

可直接CLI执行的三个 Snapshot 输入与实际输出为 `fixtures/example-{root-parallel,mixed-series,author-category}.{input,output}.json`。实际输出是审计记录，独立 expected 才是测试oracle；报告脚本不会用输出回写期望。
