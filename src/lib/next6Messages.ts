const zh={
 'note.reload':'放弃草稿并读取已保存内容',
 'note.title':'笔记','note.saving':'正在保存…','note.failed':'保存失败，输入已保留。请重试；如内容冲突，请重新打开核对。','note.editing':'编辑中','note.saved':'已保存','note.retry':'重试','note.save':'保存','note.clear':'清空',
 'palette.title':'快捷操作','palette.placeholder':'搜索作品或操作','palette.empty':'没有结果',
 'drop.title':'拖入文件夹以添加资源库','drop.remaining':'还有 {count} 个文件夹','drop.cancel':'取消剩余','drop.path':'文件夹路径','drop.choose':'选择文件夹','drop.invalid':'无法添加该目录，请检查路径、权限及资源库是否重叠。',
 'find.title':'书内查找','find.placeholder':'搜索书内文字','find.empty':'未找到匹配文字','find.noText':'未检测到可搜索文字','find.progress':'已搜索 {count} / {total}','find.matches':'{current} / {count}','find.limit':'结果已达上限（500）；请缩小搜索范围。','find.stopped':'已达到本次搜索预算，可缩小关键词后重试。','find.previous':'上一个匹配','find.next':'下一个匹配','find.close':'关闭查找',
 'cover.title':'更换封面','cover.local':'本地图片','cover.online':'在线候选','cover.pages':'书籍内页','cover.more':'加载更多','cover.use':'使用此封面','cover.preview':'预览封面','cover.empty':'没有可用候选','cover.failed':'读取失败，当前封面已保留。','cover.auto':'恢复自动封面','cover.confirm':'清除当前自定义封面？','cover.page':'第 {page} 页',
 'relocate.samplesVerified':'已比对 {count} 个原文件内容样本','relocate.offline':'原位置离线，按索引结构、大小和修改时间核对。',
 'relocate.title':'重新定位资源库','relocate.old':'原位置','relocate.new':'新位置','relocate.check':'预检查','relocate.confirm':'确认重定位','relocate.ready':'已核对 {count} 项，{missing} 项不一致或缺失','relocate.sample':'相对路径样本','relocate.blocked':'尚不能安全重定位。请确认是同一收藏，且相对结构及文件内容完整。','relocate.done':'资源库位置已更新','relocate.busy':'请先关闭阅读器，并等待扫描和后台任务结束。'
} as const;
type Key=keyof typeof zh;
const en:Record<Key,string>={
 'note.reload':'Discard draft and reload saved note',
 'note.title':'Notes','note.saving':'Saving…','note.failed':'Could not save. Your draft is retained. Retry; reopen to resolve a conflict.','note.editing':'Editing','note.saved':'Saved','note.retry':'Retry','note.save':'Save','note.clear':'Clear',
 'palette.title':'Quick actions','palette.placeholder':'Search works or actions','palette.empty':'No results',
 'drop.title':'Drop folders to add libraries','drop.remaining':'{count} folders remaining','drop.cancel':'Cancel remaining','drop.path':'Folder path','drop.choose':'Choose folder','drop.invalid':'Cannot add this folder. Check its path, permissions and overlapping libraries.',
 'find.title':'Find in book','find.placeholder':'Search book text','find.empty':'No matches','find.noText':'No searchable text detected','find.progress':'Searched {count} / {total}','find.matches':'{current} / {count}','find.limit':'500 results reached. Refine your search.','find.stopped':'Search budget reached. Refine your query and retry.','find.previous':'Previous match','find.next':'Next match','find.close':'Close find',
 'cover.title':'Change cover','cover.local':'Local image','cover.online':'Online covers','cover.pages':'Book pages','cover.more':'Load more','cover.use':'Use this cover','cover.preview':'Preview cover','cover.empty':'No available covers','cover.failed':'Could not read the image. Current cover retained.','cover.auto':'Restore automatic cover','cover.confirm':'Clear the custom cover?','cover.page':'Page {page}',
 'relocate.samplesVerified':'Compared {count} original file samples','relocate.offline':'Previous location is offline. Checked indexed structure, sizes and modification times.',
 'relocate.title':'Relocate library','relocate.old':'Previous location','relocate.new':'New location','relocate.check':'Preflight','relocate.confirm':'Confirm relocation','relocate.ready':'Checked {count} items; {missing} missing or different','relocate.sample':'Relative path samples','relocate.blocked':'Cannot safely relocate. Select the same collection with complete, unchanged relative structure and content.','relocate.done':'Library location updated','relocate.busy':'Close the reader and wait for scanning and background tasks to finish.'
};
const ja:Record<Key,string>={
 'note.reload':'下書きを破棄して保存済みメモを読む',
 'note.title':'メモ','note.saving':'保存中…','note.failed':'保存できませんでした。入力は保持されています。再試行してください。競合時は開き直して確認してください。','note.editing':'編集中','note.saved':'保存済み','note.retry':'再試行','note.save':'保存','note.clear':'消去',
 'palette.title':'クイック操作','palette.placeholder':'作品や操作を検索','palette.empty':'該当なし',
 'drop.title':'フォルダーをドロップしてライブラリを追加','drop.remaining':'残り {count} フォルダー','drop.cancel':'残りをキャンセル','drop.path':'フォルダーパス','drop.choose':'フォルダーを選択','drop.invalid':'パス、権限、ライブラリの重複を確認してください。',
 'find.title':'本の中を検索','find.placeholder':'本文を検索','find.empty':'一致なし','find.noText':'検索可能な文字がありません','find.progress':'検索済み {count} / {total}','find.matches':'{current} / {count}','find.limit':'500 件に達しました。検索を絞ってください。','find.stopped':'検索の上限に達しました。検索語を絞って再試行してください。','find.previous':'前の一致','find.next':'次の一致','find.close':'検索を閉じる',
 'cover.title':'表紙を変更','cover.local':'ローカル画像','cover.online':'オンライン候補','cover.pages':'本のページ','cover.more':'さらに表示','cover.use':'この表紙を使用','cover.preview':'表紙をプレビュー','cover.empty':'候補がありません','cover.failed':'読み取れませんでした。現在の表紙は保持されています。','cover.auto':'自動表紙に戻す','cover.confirm':'手動の表紙を消去しますか？','cover.page':'{page} ページ',
 'relocate.samplesVerified':'元ファイル {count} 件の内容サンプルを照合','relocate.offline':'元の場所はオフラインです。構造、サイズ、更新日時を照合しました。',
 'relocate.title':'ライブラリの場所を変更','relocate.old':'現在の場所','relocate.new':'新しい場所','relocate.check':'事前確認','relocate.confirm':'場所の変更を確定','relocate.ready':'{count} 項目を確認、{missing} 項目が不一致または不在','relocate.sample':'相対パスの例','relocate.blocked':'安全に変更できません。同じ内容と相対構造のコレクションを選択してください。','relocate.done':'ライブラリの場所を変更しました','relocate.busy':'リーダーを閉じ、スキャンとバックグラウンド処理の完了をお待ちください。'
};
const ko:Record<Key,string>={
 'note.reload':'초안을 버리고 저장된 메모 읽기',
 'note.title':'메모','note.saving':'저장 중…','note.failed':'저장하지 못했습니다. 입력은 유지됩니다. 다시 시도하고 충돌 시 다시 열어 확인하세요.','note.editing':'편집 중','note.saved':'저장됨','note.retry':'재시도','note.save':'저장','note.clear':'지우기',
 'palette.title':'빠른 작업','palette.placeholder':'작품 또는 작업 검색','palette.empty':'결과 없음',
 'drop.title':'폴더를 끌어 라이브러리 추가','drop.remaining':'폴더 {count}개 남음','drop.cancel':'나머지 취소','drop.path':'폴더 경로','drop.choose':'폴더 선택','drop.invalid':'경로, 권한 및 라이브러리 중복을 확인하세요.',
 'find.title':'책에서 찾기','find.placeholder':'본문 검색','find.empty':'일치하는 결과 없음','find.noText':'검색할 수 있는 텍스트가 없습니다','find.progress':'검색됨 {count} / {total}','find.matches':'{current} / {count}','find.limit':'결과가 500개에 도달했습니다. 검색 범위를 좁히세요.','find.stopped':'검색 한도에 도달했습니다. 검색어를 좁혀 다시 시도하세요.','find.previous':'이전 결과','find.next':'다음 결과','find.close':'찾기 닫기',
 'cover.title':'표지 변경','cover.local':'로컬 이미지','cover.online':'온라인 후보','cover.pages':'책 페이지','cover.more':'더 보기','cover.use':'이 표지 사용','cover.preview':'표지 미리 보기','cover.empty':'사용 가능한 후보 없음','cover.failed':'읽지 못했습니다. 현재 표지는 유지됩니다.','cover.auto':'자동 표지 복원','cover.confirm':'사용자 지정 표지를 지울까요?','cover.page':'{page}페이지',
 'relocate.samplesVerified':'원본 파일 샘플 {count}개 비교','relocate.offline':'기존 위치가 오프라인입니다. 색인의 구조, 크기, 수정 시간을 확인했습니다.',
 'relocate.title':'라이브러리 위치 변경','relocate.old':'기존 위치','relocate.new':'새 위치','relocate.check':'사전 확인','relocate.confirm':'위치 변경 확인','relocate.ready':'{count}개 확인, {missing}개 누락 또는 불일치','relocate.sample':'상대 경로 예시','relocate.blocked':'안전하게 변경할 수 없습니다. 내용과 상대 구조가 같은 컬렉션을 선택하세요.','relocate.done':'라이브러리 위치가 변경되었습니다','relocate.busy':'리더를 닫고 스캔과 백그라운드 작업이 끝날 때까지 기다리세요.'
};
export const next6Messages={'zh-CN':zh,'en-US':en,'ja-JP':ja,'ko-KR':ko};
