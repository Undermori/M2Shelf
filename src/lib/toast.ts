import type {ToastKind} from '../components/Toast';

export interface ToastNotice {kind: ToastKind; text: string;}
export const TOAST_EVENT = 'm2shelf-toast';

/** One application-wide receipt channel; the App owns display and lifetime. */
export function notifyToast(kind: ToastKind, text: string) {
  window.dispatchEvent(new CustomEvent<ToastNotice>(TOAST_EVENT, {detail: {kind, text}}));
}

export function subscribeToasts(receive: (notice: ToastNotice) => void) {
  const listener = (event: Event) => receive((event as CustomEvent<ToastNotice>).detail);
  window.addEventListener(TOAST_EVENT, listener);
  return () => window.removeEventListener(TOAST_EVENT, listener);
}
