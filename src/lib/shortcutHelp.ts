import type { AppLanguage } from '../types/media';

const zh = {
  title: '键盘快捷键', close: '关闭', general: '导航与检索', studio: '工作台', reader: '阅读器',
  hint: '快捷键随当前页面生效。输入文字时不会触发单字母或方向导航。',
  search: '搜索作品、笔记与截图', find: '聚焦当前页搜索', back: '返回上一页', help: '显示快捷键速览',
  openFile:'在项目文件中打开所选项', tabs: '切换笔记 / 截图画廊 / 项目文件', save: '保存当前作品笔记', images: '灯箱上一张 / 下一张',
  grid: '移动网格、列表或页签焦点', pages: '上一页 / 下一页', fullscreen: '切换全屏', bookmark: '添加 / 移除书签', dismiss: '关闭当前弹窗',
};
type Copy = Record<keyof typeof zh, string>;
export const shortcutCopy: Record<AppLanguage, Copy> = {
  'zh-CN': zh,
  'en-US': { title: 'Keyboard shortcuts', close: 'Close', general: 'Navigation & search', studio: 'Workbench', reader: 'Reader', hint: 'Shortcuts apply to the current view. Typing does not trigger letter or arrow navigation.', search: 'Find works, notes and screenshots', find: 'Focus this page’s search', back: 'Go back', help: 'Show keyboard shortcuts', openFile:'Open the selected project item', tabs: 'Switch notes / gallery / project files', save: 'Save the current work note', images: 'Previous / next lightbox image', grid: 'Move grid, list or tab focus', pages: 'Previous / next page', fullscreen: 'Toggle fullscreen', bookmark: 'Add / remove bookmark', dismiss: 'Close the current dialog' },
  'ja-JP': { title: 'キーボードショートカット', close: '閉じる', general: '移動と検索', studio: 'ワークスペース', reader: 'リーダー', hint: '現在の画面で有効な操作です。文字入力中は文字キーや矢印キーで画面を移動しません。', search: '作品・ノート・スクリーンショットを検索', find: '現在の画面の検索欄へ', back: '前の画面に戻る', help: 'ショートカット一覧を表示', openFile:'選択したプロジェクト項目を開く', tabs: 'メモ / 画像 / プロジェクトファイルを切り替え', save: '現在の作品ノートを保存', images: 'ライトボックスの前 / 次の画像', grid: 'グリッド・リスト・タブのフォーカスを移動', pages: '前 / 次のページ', fullscreen: '全画面表示を切り替え', bookmark: 'しおりを追加 / 削除', dismiss: '現在のダイアログを閉じる' },
  'ko-KR': { title: '키보드 단축키', close: '닫기', general: '탐색 및 검색', studio: '작업 공간', reader: '리더', hint: '현재 화면에서 사용할 수 있는 단축키입니다. 문자 입력 중에는 문자 및 방향키로 화면을 이동하지 않습니다.', search: '작품, 메모 및 스크린샷 검색', find: '현재 화면 검색에 초점', back: '이전 화면으로', help: '단축키 목록 표시', openFile:'선택한 프로젝트 항목 열기', tabs: '메모 / 갤러리 / 프로젝트 파일 전환', save: '현재 작품 메모 저장', images: '라이트박스 이전 / 다음 이미지', grid: '그리드, 목록 또는 탭 초점 이동', pages: '이전 / 다음 페이지', fullscreen: '전체 화면 전환', bookmark: '책갈피 추가 / 삭제', dismiss: '현재 대화상자 닫기' },
};

export function isShortcutHelpKey(event: KeyboardEvent): boolean {
  return !event.defaultPrevented && !event.isComposing && !event.ctrlKey && !event.metaKey && !event.altKey
    && (event.key === '?' || (event.shiftKey && event.code === 'Slash'))
    && !(event.target instanceof HTMLElement && event.target.closest('input,textarea,select,[contenteditable],[role=combobox]'));
}
export function openShortcutHelp() { window.dispatchEvent(new Event('m2shelf-shortcuts')); }
