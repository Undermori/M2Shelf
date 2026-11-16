import type {AppLanguage} from '../types/media';

const en = {
  organize:'Select images', done:'Done', selectPage:'Select this page', clear:'Clear selection', selected:'selected',
  addTags:'Add tags', removeTags:'Remove tags', tagInput:'Tags, separated by commas', link:'Link selected images',
  applyLink:'Apply link', changed:'images updated', filterTag:'Filter by tag', fit:'Fit', actual:'100%',
  viewHint:'← / → Browse', batchHint:'Shift+click selects a range on this page.',
  conflict:'The selection changed. Refresh and select the images again; no batch changes were saved.',
};
type Copy = Record<keyof typeof en,string>;
export const round8GalleryCopy: Record<AppLanguage,Copy> = {
  'en-US':en,
  'zh-CN':{organize:'多选整理',done:'完成',selectPage:'全选本页',clear:'取消选择',selected:'项已选',addTags:'追加标签',removeTags:'移除标签',tagInput:'标签，以逗号分隔',link:'关联所选截图',applyLink:'应用关联',changed:'张截图已更新',filterTag:'按标签筛选',fit:'适应窗口',actual:'100%',viewHint:'← / → 翻页',batchHint:'Shift+点击可连续选择本页截图。',conflict:'所选截图已变化，请刷新并重新选择；本次批量修改未保存。'},
  'ja-JP':{organize:'複数選択',done:'完了',selectPage:'このページを選択',clear:'選択を解除',selected:'件選択',addTags:'タグを追加',removeTags:'タグを削除',tagInput:'タグ（カンマ区切り）',link:'選択した画像を関連付け',applyLink:'関連付けを適用',changed:'枚の画像を更新しました',filterTag:'タグで絞り込む',fit:'全体表示',actual:'100%',viewHint:'← / → 移動',batchHint:'Shift+クリックでこのページの範囲を選択します。',conflict:'選択した画像が変更されました。更新して選択し直してください。一括変更は保存されていません。'},
  'ko-KR':{organize:'여러 장 선택',done:'완료',selectPage:'현재 페이지 선택',clear:'선택 해제',selected:'개 선택됨',addTags:'태그 추가',removeTags:'태그 제거',tagInput:'쉼표로 구분한 태그',link:'선택한 이미지 연결',applyLink:'연결 적용',changed:'개 이미지 업데이트됨',filterTag:'태그로 필터링',fit:'창에 맞춤',actual:'100%',viewHint:'← / → 이동',batchHint:'Shift+클릭으로 현재 페이지의 범위를 선택합니다.',conflict:'선택한 이미지가 변경되었습니다. 새로 고친 후 다시 선택하세요. 일괄 변경은 저장되지 않았습니다.'},
};
