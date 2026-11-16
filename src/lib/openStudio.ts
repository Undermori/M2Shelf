import type {CollectionTarget} from '../types/collection';
export type StudioLink = {view: 'notes'; target: CollectionTarget; nodeId?: number} | {view: 'gallery'; screenshotId: number} | {view:'files';folderId:number};
let pending: StudioLink | null = null;
export function takeStudioLink() {const link = pending; pending = null; return link;}
export function openStudio(link: StudioLink) {
  pending = link;
  window.dispatchEvent(new CustomEvent<StudioLink>('m2shelf-open-studio', {detail: link}));
}
