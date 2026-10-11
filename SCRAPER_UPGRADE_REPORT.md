# M²Shelf 统一刮削实现报告

日期：2026-10-11。当前工作树为本地 0.5.14 候选；公开 0.5.13 保持不变。本轮没有提交、推送、签名、发布或覆盖安装。

## 实现与边界

新增 `tools/media-parser` 共享 Rust 解析库。它先识别有语境的提供方 ID、季集、日期、卷章、版本及发布信息，再生成标题候选；不是在每个入口各自补清洗正则。输出保留原名、显示标题、候选来源、年份、子类型、季集、日期、卷章、版本、发布标签、ISBN、提供方 ID、作者、语言、证据和警告。保留数字标题和 `:re`；没有标记的数字不自动当季、卷或集。匹配使用 NFKC，物理归组使用同一语法的非 NFKC 入口，避免把 Unicode 拼写不同的独立源误并为版本。

`src-tauri/src/media_scraper.rs` 是索引证据入口。自动匹配、显式重匹配和手动搜索预填复用它。视频证据来自 SQLite 的逻辑所有权及至多 32 个索引文件名；书籍来自已有阅读归属。详情和匹配不遍历磁盘。已有标题冲突评分、Root 类型约束、取消/过期写入保护和手工绑定保护继续使用。

`src-tauri/src/scraper_metadata.rs` 在扫描时读取经过 Root、真实打开句柄、大小及版本检查的 NFO、ComicInfo、EPUB OPF、MOBI/AZW3 EXTH 和 PDF Info。XML 禁止 DTD/外部实体，有 1 MiB、节点数及归档 CRC/路径限制。Kindle 此处仅读取非加密元数据，不绕过 DRM。PDF 元数据读取限于 16 MiB；较大 PDF 仍按原有 512 MiB 索引/分段阅读边界工作，元数据读取记为延期，不影响阅读。TXT 没有虚构的内嵌元数据。

新增 additive migration 27 的 `indexed_media_metadata`，按源版本缓存导入结果，含 Node/Book/Resource 外键及查询索引。失败保存警告；同版本重扫复用导入结果。既有库需实际扫描相关目录才会补齐旧索引的元数据，单纯打开详情不进行回填。手工显示名优先；相互冲突的年份/ID阻止自动绑定。

智能书籍组织从这个表按 Book ID 获取标题、系列和卷数，复用已有 lab 的 Metadata 结构与人工修正优先级。作者仅是元数据，不把作者目录里的多个书合成一部作品。归组改变不修改物理 Book/Resource/Node ID，也不删除进度、书签、收藏或人工关系。

## 媒体与提供方

| 类型 | 自动路径 | 保留边界 |
|---|---|---|
| 动画 | Bangumi type 2 | 精确主标题/官方别名及硬冲突检查；不强行绑定首条结果 |
| 真人电影 | TMDb `/movie` | 可靠发布年、精确名称/官方别名、唯一候选、非冲突类型 |
| 真人剧集 | 有结构证据时 TMDb `/tv` | 季集/日期/明确 TV ID；仅有 `TV` 字样不够；后续季年份不等同首播年 |
| 漫画 | 共享卷章/系列证据、ComicInfo；Bangumi type 1 | 格式/卷目录仍由既有所有权折叠，不额外成为匹配对象 |
| 电子书 | 内嵌书名/作者/ISBN/语言及文件名回退；Bangumi type 1 | 独立文件和作者分类保留独立身份 |
| 同人本、设定集 | 本地封面优先；默认不自动联网 | 显式开启后仍需精确标题证据；不引入新提供方 |

明确的 Bangumi/TMDb ID 优先进行有界官方详情核验，仍检查类型和强冲突。未绑定的普通自动流程不替换已有提供方选择或手工封面。Bangumi 移除 primary result 的强制提分，多个达到门槛且无硬冲突的精确结果保留未绑定，不以提供方排名强行选择。TMDb Movie/TV 使用独立 endpoint、cache key、绑定 JSON 类型及目录聚合键；旧绑定 JSON 无类型时兼容为 Movie，避免相同数字 ID 跨类型误并。原始语言海报策略两类共用，不随界面语言换成中文海报。

现有每节点最多 3 次查询、最多 5 个详情及全轮 256 新详情预算、缓存、429/瞬态重试及 provider breaker 保留。诊断区分认证、限流、网络、无结果、年份不确定、低可信度、多可靠候选和预算延期；无效显式 TMDb ID 的 404 回退标题搜索，不当作提供方全面故障。Bangumi/TMDb 最近原因以有界 settings 记录，并进入现有匹配弹窗；手动搜索弹窗新增折叠的识别依据。没有新增代理服务、API key 或设置页面。

## 实际研究的上游实现与测试

以下均检查了实际源文件和相应测试，不只阅读 README。实现为本项目独立编写；未复制 GPL/LGPL 上游实现或测试语料进发行程序。没有声称移植完整上游引擎。

| 上游与固定 revision | 实现/测试 | 本项目采用的原则与落点 | 上游许可证 |
|---|---|---|---|
| [GuessIt](https://github.com/guessit-io/guessit/tree/3c73072b8971d412935c6f749be63c564dd41385) | `rules/properties/title.py`、`episodes.py`、`date.py`；`test/rules/title.yml`、movies/episodes 语料 | 属性先识别、标题从剩余跨度提取；共享 parser 的 staged spans/候选来源 | LGPL-3.0 |
| [Kodi](https://github.com/xbmc/xbmc/tree/494721df6fde78d1188ae110b6688d2a8324ba44) | `xbmc/NfoFile.cpp` TryParsing；`video/test/TestVideoInfoTag.cpp` | NFO 根类型、明确 ID 与元数据优先；受限 XML 导入 | GPL-2.0+ |
| [Jellyfin](https://github.com/jellyfin/jellyfin/tree/a45d1415d4da7c9b69d6e7a6895fc63d2d68e8f1) | `Emby.Naming/TV/EpisodePathParser.cs`；`EpisodePathParserTest.cs` | 季集、范围和日期需有语境；绝对集数不无条件开启 | GPL-2.0 |
| [Kavita](https://github.com/Kareadita/Kavita/tree/3233c26dd5b67d1b76db74c3ee96260f833a1cf1) | `Kavita.Services/Scanner/BookParser.cs`；DefaultParserTests、BookParserTests | 书籍内嵌元数据优先、保留分类；其中部分 metadata-specific 测试为注释 TODO，未计为上游通过证据 | GPL-3.0 |
| [Komga](https://github.com/gotson/komga/tree/519e352aa894b2fdc80660f1dde21b98cf328e3e) | ComicInfoProvider、EpubMetadataProvider 及对应 Kotlin 测试 | 区分书/系列字段，OPF namespace 与作者/ISBN/语言；本项目 importer + 组织 Metadata | MIT |
| [calibre](https://github.com/kovidgoyal/calibre/tree/5a4ea6cb94641a348c1c4975b4cf204cdfcc6cb9) | `ebooks/metadata/meta.py`、`mobi.py`、作者测试 | 格式原生元数据优先、文件名回退、EXTH 有界字段；不移植转换器或 DRM 功能 | GPL-3.0 |

Kindle 阅读继续使用原有独立 LGPL libmobi worker，其完整源码、许可证及替换方式仍随八文件 Portable 分发。本轮 scraper 不把 calibre、GuessIt 或以上服务作为运行依赖。

## 验证材料与未覆盖事项

最终命令、结果、原生截图、包摘要见 `NEXT_RELEASE_0.5.14_REPORT.md`。解析器语料覆盖双语/括号/发布标签、数字标题、季集/范围/日期、卷章、ISBN/提供方 ID、`:re`、版本和一万条命名。生产 Rust 回归、49 个结构场景、41 个 Shadow 适配器场景验证旧状态、混合结构、Unicode、忽略/取消/失败及 schema 兼容。

原生验收使用应用自有隔离数据库和合法的实际格式合成文件；不读取真实个人库。它不能证明任意私人文件、所有压制命名或所有畸形文档均能匹配。未提供全量 GuessIt 兼容承诺；PDF XMP/超过 16 MiB 的 Info 导入、Kindle 未实现的 EXTH 字段及小数卷数需要安全回退。既有兼容评分与旧标题 formatter 保留，统一入口不等于删除全部历史正则。


## 反例与实际调用证据

| 输入/场景 | 验证的结果 |
|---|---|
| `Superbad.2007.1080p.BluRay.x264-GROUP.mkv` | Superbad / 2007 / Movie |
| `Ernest.And.Celestine.2012.DUBBED.AAC-RARBG.mkv` | Ernest And Celestine / 2012；发行字段从标题去除 |
| `1917.2019.2160p.mkv`、`2046.2004.mkv` | 数字标题保留；2019/2004 为发行年 |
| `Series.Name.S02E03`、`S01E01-E02`、`1x02` | 季/集及合并集列表；一个物理来源，TV endpoint |
| 芙莉莲字幕组括号、`86 - 01`、`3月的狮子 第01话` | 作品标题与集号分开；数字标题保留 |
| `20世纪少年 第01卷`、`東京喰種:re 第02巻` | 卷数明确、标题数字和 `:re` 保留 |
| 同一系列 PDF/CBZ/图片目录、系列与独立作品混放 | lab 49 结构语料及生产 ownership/详情回归；原书 ID 与阅读身份保留 |
| 作者/画师文件夹内多本书 | 分类下独立分组；原生 EPUB+TXT 实际扫描验证为两本 |
| 同人本/设定集 | Root opt-in/零自动网络默认及本地首图策略；结构和既有生产测试覆盖 |
| ComicInfo、EPUB OPF、MOBI EXTH、NFO、PDF Info | 实际格式/安全导入单测；原生七书扫描与文档读取 |
| 双模式首次/重扫/显式匹配 | `auto_match/movie_pipeline_tests.rs` 有界本地 HTTP、相同解析/请求；FOLDER 与 VIDEO_FILE |
| 无结果/多候选/错误年份/错类型/401/403/429/timeout | 分开诊断、不写绑定、提供方 breaker；无效显式 ID 404 恢复标题查询 |
| 人工绑定/海报/标签/收藏/进度/书签与取消 | 全量生产事务/扫描保留测试；原生两次重扫保留进度/书签，媒体未变化 |

上述“解析结果”由 parser golden 断言确认；本地 HTTP 的匹配效果是确定夹具，不等于线上全媒体匹配率。在线仅核验授权 TMDb 默认服务认证及代表性 TV 查询，没有自动向 Bangumi 发全量库请求。GUI 无法证明一个未自动绑定作品一定不存在于提供方。
