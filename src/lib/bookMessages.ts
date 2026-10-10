const copy = {
 'match.title': ["从 {provider} 选择作品", "Choose a {provider} title", "{provider} の作品を選択", "{provider} 작품 선택"],
 'match.tmdbDescription': ["为「{name}」选择 TMDb 电影。", "Choose a TMDb movie for “{name}”.", "「{name}」の TMDb 映画を選択します。", "“{name}”의 TMDb 영화를 선택하세요."],
 'match.provider': ["数据来源", "Metadata provider", "データ提供元", "메타데이터 제공자"],
 'match.keywordAria': ["{provider} 搜索关键词", "{provider} search keyword", "{provider} 検索キーワード", "{provider} 검색어"],
 'match.connecting': ["正在连接 {provider}…", "Connecting to {provider}…", "{provider} に接続中…", "{provider} 연결 중…"],
 'match.noneTitle': ["没有找到作品", "No titles found", "作品が見つかりません", "작품을 찾지 못했습니다"],
 'match.searchFailed': ["搜索 {provider} 失败", "{provider} search failed", "{provider} の検索に失敗しました", "{provider} 검색 실패"],
 'match.saveFailed': ["保存 {provider} 绑定失败", "Could not save {provider} binding", "{provider} の紐付け保存に失敗しました", "{provider} 연결 저장 실패"],
 'match.bound': ["TMDb 绑定已保存", "TMDb binding saved", "TMDb の紐付けを保存しました", "TMDb 연결을 저장했습니다"],
 'library.modeGuidance': ["← 文件夹比较杂乱的话，选这个\n已经整理好的话，就选下面两种 ↓", "For mixed or untidy folders, choose this mode.\nFor organized folders, choose an option below.", "混在したフォルダーには、このモードを。\n整理済みなら、下の2つから選択してください。", "폴더가 복잡하거나 혼합되어 있다면 이 모드를 선택하세요.\n정리되어 있다면 아래 두 방식 중 선택하세요."],
 'library.chooseMode': ["请选择识别方式", "Choose a recognition mode", "認識方法を選択してください", "인식 방식을 선택하세요"],
 'library.create': ["创建资源库", "Create library", "ライブラリを作成", "라이브러리 만들기"],

 'reader.chapterStatus': ['第 {page} 章 / 共 {count} 章','Chapter {page} of {count}','第 {page} 章 / 全 {count} 章','{count}장 중 {page}장'],
 'reader.chapterName': ['第 {page} 章','Chapter {page}','第 {page} 章','{page}장'],
 'reader.segmentStatus': ['第 {page} 段 / 共 {count} 段','Section {page} of {count}','第 {page} 節 / 全 {count} 節','{count}절 중 {page}절'],

 'book.readableFiles': ['可阅读文件','Readable files','読書ファイル','읽기 파일'],
 'artbook.name': ['设定集','Artbooks','設定資料集','설정집'],
 'artbook.files': ['设定集文件','Artbook files','設定資料ファイル','설정집 파일'],
 'library.noAutoBangumi': ['不自动关联 Bangumi（推荐）','Do not match Bangumi automatically (recommended)','Bangumi と自動照合しない（推奨）','Bangumi 자동 연결 안 함 (권장)'],
 'library.noAutoHelp': ['不进行自动搜索与绑定。图片目录使用阅读顺序中的首张有效图片；其他书籍优先使用内置封面。创建后仍可手动关联或更换封面。','Skip automatic search and binding. Image folders use their first valid page; other books prefer embedded covers. Manual binding and covers remain available.','自動検索・紐付けを行いません。画像フォルダーは読書順の最初の有効な画像、他の本は内蔵表紙を優先します。作成後も手動で紐付け・表紙変更できます。','자동 검색과 연결을 하지 않습니다. 이미지 폴더는 읽기 순서상 첫 유효 이미지, 다른 책은 내장 표지를 우선합니다. 이후 수동 연결과 표지 변경이 가능합니다.'],
 'library.doujinAdvice': ['Bangumi 可能未收录部分同人作品，建议保持此选项开启。','Some doujinshi are not listed on Bangumi. Keeping this option enabled is recommended.','Bangumi に未登録の同人作品もあるため、この設定を推奨します。','Bangumi에 없는 동인지가 있으므로 이 설정을 권장합니다.'],
 'library.policySaved': ['资源库关联策略已保存','Library matching policy saved','ライブラリの照合設定を保存しました','라이브러리 연결 정책을 저장했습니다'],
 'book.drm': ['此书受 DRM 保护，无法内置阅读。请使用合法的未加密版本。','This book is DRM-protected. Use a legally obtained DRM-free edition.','DRM で保護された本です。正規の DRM なしの版を使用してください。','DRM으로 보호된 책입니다. 합법적인 DRM 없는 버전을 사용하세요.'],
 'book.variant': ['暂不支持此 Kindle 变体（如字典、Print Replica 或非 MOBI6/KF8 格式）。','This Kindle variant is unsupported (such as dictionaries, Print Replica, or formats other than MOBI6/KF8).','この Kindle 形式には未対応です（辞書、Print Replica、MOBI6/KF8 以外など）。','이 Kindle 변형은 지원하지 않습니다 (사전, Print Replica, MOBI6/KF8 이외 형식 등).'],
 'book.compression': ['此 Kindle 文件使用了不支持的压缩方式。','This Kindle compression method is unsupported.','この Kindle 圧縮方式には未対応です。','이 Kindle 압축 방식은 지원하지 않습니다.'],
 'book.encoding': ['此 Kindle 文件的文字编码不受支持或已损坏。','The Kindle text encoding is unsupported or damaged.','Kindle の文字コードが未対応または破損しています。','Kindle 텍스트 인코딩이 지원되지 않거나 손상되었습니다.'],
 'book.limit': ['此 Kindle 文件超过读取预算（源文件 128 MiB、正文 32 MiB、单章 4 MiB），或解析超时。','This Kindle file exceeds reading limits (128 MiB source, 32 MiB text, 4 MiB per chapter), or parsing timed out.','Kindle の読み取り上限を超えたか、解析がタイムアウトしました（本体 128 MiB、本文 32 MiB、1章 4 MiB）。','Kindle 읽기 한도를 초과했거나 분석 시간이 초과되었습니다 (파일 128 MiB, 본문 32 MiB, 장당 4 MiB).'],
 'book.invalid': ['Kindle 文件结构损坏或内容无法解析。','The Kindle file structure is damaged or cannot be parsed.','Kindle の構造が破損しているか、内容を解析できません。','Kindle 파일 구조가 손상되었거나 내용을 분석할 수 없습니다.'],
 'book.worker': ['内置 Kindle 解析组件无法启动，请重新安装完整程序。','The bundled Kindle parser could not start. Reinstall the complete application.','内蔵 Kindle 解析コンポーネントを起動できません。完全なアプリを再インストールしてください。','내장 Kindle 분석기를 시작할 수 없습니다. 전체 프로그램을 다시 설치하세요.'],
 'menu.file': ['文件','File','ファイル','파일'],
 'menu.view': ['显示','View','表示','보기'],
 'menu.go': ['转至','Go','移動','이동'],
 'menu.help': ['帮助','Help','ヘルプ','도움말'],
 'menu.about': ['关于 M²Shelf','About M²Shelf','M²Shelf について','M²Shelf 정보'],
 'menu.official': ['项目主页','Project website','プロジェクトページ','프로젝트 페이지'],
 'book.toc': ['目录','Contents','目次','목차'],
} as const;
type Key = keyof typeof copy;
const locale = (index: number) => Object.fromEntries(Object.entries(copy).map(([key,value])=>[key,value[index]])) as Record<Key,string>;
export const bookMessages = {'zh-CN':locale(0),'en-US':locale(1),'ja-JP':locale(2),'ko-KR':locale(3)};
export function bookErrorKey(error: unknown) {
 const value=String(error);
 if(value.includes('EPUB_CHAPTER_LIMIT'))return 'epub.chapterLimit';
 if(value.includes('EPUB_STRUCTURE_LIMIT'))return 'epub.structureLimit';
 if(value.includes('BOOK_KINDLE_')) {
  if(value.includes('DRM'))return 'book.drm';
  if(value.includes('VARIANT'))return 'book.variant';
  if(value.includes('COMPRESSION'))return 'book.compression';
  if(value.includes('ENCODING'))return 'book.encoding';
  if(value.includes('LIMIT'))return 'book.limit';
  if(value.includes('WORKER'))return 'book.worker';
  return 'book.invalid';
 }
 return value.includes('ENCRYPTED')?'comic.encrypted':value.includes('LIMIT')?'comic.limit':value.includes('OUTSIDE_ROOT')?'comic.outside':'comic.error';
}
