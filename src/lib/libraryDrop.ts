export function canImportLibraryDrop(state: {
  initialized: boolean; showOnboarding: boolean; page: string;
  detail: unknown; currentNode: unknown; readerBookId: number | null; rootAddBusy: boolean;
}): boolean {
  return state.initialized && !state.showOnboarding && state.page === 'all' &&
    !state.detail && !state.currentNode && state.readerBookId === null && !state.rootAddBusy;
}
