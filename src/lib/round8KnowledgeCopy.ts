import type {AppLanguage} from '../types/media';
const zh = {
  'knowledge.work': '作品', 'knowledge.note': '笔记', 'knowledge.screenshot': '截图', 'knowledge.action': '动作',
  'knowledge.edit': '工作台编辑', 'knowledge.copy': '复制 Markdown', 'knowledge.export': '导出 Markdown',
  'knowledge.exportAll': '导出笔记汇总 (.md)', 'knowledge.copied': 'Markdown 已复制', 'knowledge.exported': 'Markdown 已导出',
  'knowledge.failed': '无法完成操作，请重试。导出位置必须在媒体库和应用数据目录之外。',
  'knowledge.exportLimit': '笔记汇总超出便携导出上限，请单篇导出。',
  'knowledge.recent': '最近编辑优先', 'knowledge.title': '作品名称排序', 'knowledge.sort': '笔记排序',
  'knowledge.library': '所属库', 'knowledge.updated': '更新时间', 'knowledge.untitled': '笔记',
  'knowledge.search': '搜索作品、笔记、截图或动作', 'knowledge.all': '全部来源',
  'knowledge.searchFailed': '部分来源暂时无法检索，请重试。', 'knowledge.notesHeading': '笔记与截图',
} as const;
type Copy = Record<keyof typeof zh, string>;
export const round8KnowledgeCopy: Record<AppLanguage, Copy> = {
  'zh-CN': zh,
  'en-US': {
    'knowledge.work': 'Work', 'knowledge.note': 'Note', 'knowledge.screenshot': 'Screenshot', 'knowledge.action': 'Action',
    'knowledge.edit': 'Edit in workbench', 'knowledge.copy': 'Copy Markdown', 'knowledge.export': 'Export Markdown',
    'knowledge.exportAll': 'Export all notes (.md)', 'knowledge.copied': 'Markdown copied', 'knowledge.exported': 'Markdown exported',
    'knowledge.failed': 'Could not complete this action. Try again; export outside media libraries and application data.',
    'knowledge.exportLimit': 'The notes exceed the portable export limit. Export individual notes instead.',
    'knowledge.recent': 'Recently edited', 'knowledge.title': 'Work title', 'knowledge.sort': 'Note order',
    'knowledge.library': 'Library', 'knowledge.updated': 'Updated', 'knowledge.untitled': 'Notes',
    'knowledge.search': 'Search works, notes, screenshots or actions', 'knowledge.all': 'All sources',
    'knowledge.searchFailed': 'Some sources could not be searched. Please try again.', 'knowledge.notesHeading': 'Notes and screenshots',
  },
  'ja-JP': {
    'knowledge.work': '作品', 'knowledge.note': 'ノート', 'knowledge.screenshot': 'スクリーンショット', 'knowledge.action': '操作',
    'knowledge.edit': '作業台で編集', 'knowledge.copy': 'Markdown をコピー', 'knowledge.export': 'Markdown を書き出す',
    'knowledge.exportAll': '全ノートを書き出す (.md)', 'knowledge.copied': 'Markdown をコピーしました', 'knowledge.exported': 'Markdown を書き出しました',
    'knowledge.failed': '操作を完了できませんでした。保存先はメディアライブラリとアプリデータの外を選んでください。',
    'knowledge.exportLimit': 'ノートが書き出し上限を超えています。個別に書き出してください。',
    'knowledge.recent': '最近の編集順', 'knowledge.title': '作品名順', 'knowledge.sort': 'ノートの並び順',
    'knowledge.library': 'ライブラリ', 'knowledge.updated': '更新日時', 'knowledge.untitled': 'ノート',
    'knowledge.search': '作品・ノート・スクリーンショット・操作を検索', 'knowledge.all': 'すべての種類',
    'knowledge.searchFailed': '一部の検索に失敗しました。もう一度お試しください。', 'knowledge.notesHeading': 'ノートとスクリーンショット',
  },
  'ko-KR': {
    'knowledge.work': '작품', 'knowledge.note': '메모', 'knowledge.screenshot': '스크린샷', 'knowledge.action': '작업',
    'knowledge.edit': '작업대에서 편집', 'knowledge.copy': 'Markdown 복사', 'knowledge.export': 'Markdown 내보내기',
    'knowledge.exportAll': '모든 메모 내보내기 (.md)', 'knowledge.copied': 'Markdown을 복사했습니다', 'knowledge.exported': 'Markdown을 내보냈습니다',
    'knowledge.failed': '작업을 완료하지 못했습니다. 미디어 라이브러리와 앱 데이터 외부에 저장해 주세요.',
    'knowledge.exportLimit': '메모가 내보내기 한도를 초과했습니다. 개별 메모로 내보내 주세요.',
    'knowledge.recent': '최근 편집순', 'knowledge.title': '작품 이름순', 'knowledge.sort': '메모 정렬',
    'knowledge.library': '라이브러리', 'knowledge.updated': '업데이트', 'knowledge.untitled': '메모',
    'knowledge.search': '작품, 메모, 스크린샷 또는 작업 검색', 'knowledge.all': '모든 종류',
    'knowledge.searchFailed': '일부 항목을 검색하지 못했습니다. 다시 시도해 주세요.', 'knowledge.notesHeading': '메모 및 스크린샷',
  },
};
