import type {AppLanguage, MediaNode} from '../types/media';

type Copy = {metadata: string; unbound: string; allMetadata: string};
export const round8UiCopy: Record<AppLanguage, Copy> = {
  'zh-CN': {metadata: '元数据', unbound: '未绑定元数据的作品', allMetadata: '全部绑定状态'},
  'en-US': {metadata: 'Metadata', unbound: 'Works without metadata', allMetadata: 'All binding states'},
  'ja-JP': {metadata: 'メタデータ', unbound: 'メタデータ未連携の作品', allMetadata: 'すべての連携状態'},
  'ko-KR': {metadata: '메타데이터', unbound: '메타데이터 미연결 작품', allMetadata: '모든 연결 상태'},
};

/** Only genuine work sources; a series or loose attachment is not a missing binding. */
export function isUnboundWork(node: MediaNode | undefined): boolean {
  return !!node && (node.nodeType === 'WORK' || node.nodeType === 'AUTO_WORK') && !node.binding && !node.tmdbBinding?.active;
}
