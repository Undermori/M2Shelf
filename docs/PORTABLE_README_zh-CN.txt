M²Shelf {{VERSION}} Portable（Windows x64）
MORI MEDIA SHELF

使用方法
1. 将 ZIP 完整解压到普通文件夹，不要在压缩包内直接运行。
2. 双击 M2Shelf.exe；无需安装或管理员权限。
3. 添加媒体目录并扫描，按需设置外部播放器。

Portable 文件
- M2Shelf.exe：应用主程序。
- M2ShelfUpdater.exe：仅用于校验并应用 M²Shelf 官方签名更新，请勿单独移动或替换。
- M2Shelf.portable.json：Portable 模式标记；主程序和更新程序依靠它确认分发类型。
- README_zh-CN.txt：本说明。
- M2ShelfMobi.exe：内置 Kindle 文档解析辅助程序。
- M2ShelfMobi-source.zip：解析辅助程序的源码、许可证与构建说明。
- THIRD-PARTY-NOTICES.txt：第三方组件声明。
- SHA256SUMS.txt：其余七个文件的 SHA-256 完整性记录。

数据与隐私
- M²Shelf 只读取媒体目录，不移动、重命名、删除或写入源文件。
- 索引、绑定、标签、收藏夹、观看记录和设置保存在 Windows 应用数据目录。
- 封面写入应用缓存；缓存位置可在设置中修改。
- Portable 表示应用本体免安装，不代表完全不产生本地数据。

联网说明
- Bangumi 搜索和封面下载需要联网。
- 本地索引浏览和打开本地文件不依赖 Bangumi。

升级
- 软件只接受与版本和 Windows x64 分发类型绑定、并通过官方 Ed25519 公钥验证的更新包；校验失败时不会替换程序。
- 自动更新由 M2ShelfUpdater.exe 在主程序退出后完成，不会修改媒体资源库。
- 也可以退出旧版本，完整解压官方新版本后直接运行；不要只复制其中某个 EXE。
- 数据库会自动执行兼容迁移；媒体源不会被修改。

注意
- 当前构建未进行代码签名，Windows SmartScreen 可能提示“未知发布者”。
- 界面依赖 Microsoft Edge WebView2 Runtime；Windows 11 通常已包含。
- ZIP 同目录的 .sha256 文件用于校验整个下载包；ZIP 内的 SHA256SUMS.txt 用于校验解压后的各组成文件。
