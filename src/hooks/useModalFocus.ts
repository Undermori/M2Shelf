import {useLayoutEffect, useRef, useState, type RefObject} from 'react';
import {isolateModalSiblings} from '../lib/modalA11y';

const focusable = 'button:not(:disabled),input:not(:disabled):not([type="hidden"]),textarea:not(:disabled),select:not(:disabled),a[href],[tabindex="0"]';
interface ActiveModal {backdrop: HTMLElement; restore: () => void;}
const activeModals: ActiveModal[] = [];

/** Focus lifecycle for the existing custom overlays (not native <dialog>). */
export function useModalFocus(
  backdrop: RefObject<HTMLElement | null>,
  dialog: RefObject<HTMLElement | null>,
  onClose: () => void,
  initialSelector = '[data-modal-initial-focus],input:not(:disabled),textarea:not(:disabled)',
  busy = false,
  enabled = true,
) {
  // Several dialogs stay mounted while hidden. Capture each activation during
  // render, before a descendant can autofocus during the commit, not at mount.
  const [activation, setActivation] = useState(() => ({
    enabled,
    previous: enabled && document.activeElement instanceof HTMLElement ? document.activeElement : null,
  }));
  if (activation.enabled !== enabled) setActivation({
    enabled,
    previous: enabled && document.activeElement instanceof HTMLElement ? document.activeElement : null,
  });
  const previous = activation.previous;
  const latest = useRef({onClose, busy, initialSelector});
  latest.current = {onClose, busy, initialSelector};
  useLayoutEffect(() => {
    if (!enabled) return;
    const element = dialog.current;
    if (!element) return;
    const candidates = () => [...element.querySelectorAll<HTMLElement>(focusable), ...document.querySelectorAll<HTMLElement>('[data-toast-layer] button')]
      .filter(item => item.tabIndex >= 0 && !item.closest('[hidden],[inert],[aria-hidden="true"]'));
    const overlay = backdrop.current;
    if (!overlay) return;
    // Only the top overlay owns sibling isolation. Otherwise an older observer
    // would mark a newly mounted nested portal inert, including its own controls.
    const parent = activeModals.at(-1);
    parent?.restore();
    if (parent) parent.restore = () => undefined;
    const entry: ActiveModal = {backdrop: overlay, restore: isolateModalSiblings(overlay)};
    activeModals.push(entry);
    (element.querySelector<HTMLElement>(latest.current.initialSelector) ?? candidates()[0] ?? element).focus();
    const key = (event: KeyboardEvent) => {
      if (activeModals.at(-1) !== entry || event.isComposing || event.defaultPrevented) return;
      if (event.key === 'Escape') {
        event.preventDefault();
        event.stopImmediatePropagation();
        if (!latest.current.busy) latest.current.onClose();
      } else if (event.key === 'Tab') {
        const items = candidates();
        const current = document.activeElement;
        if (!items.length) {
          event.preventDefault();
          element.focus();
        } else if (!items.some(item => item === current) || event.shiftKey && current === items[0]) {
          event.preventDefault();
          (event.shiftKey ? items.at(-1) : items[0])?.focus();
        } else if (!event.shiftKey && current === items.at(-1)) {
          event.preventDefault();
          items[0].focus();
        }
      }
    };
    window.addEventListener('keydown', key, true);
    return () => {
      window.removeEventListener('keydown', key, true);
      const top = activeModals.at(-1) === entry;
      entry.restore();
      activeModals.splice(activeModals.indexOf(entry), 1);
      const resumed = activeModals.at(-1);
      if (top && resumed) resumed.restore = isolateModalSiblings(resumed.backdrop);
      if (previous?.isConnected && !previous.closest('[inert]')) previous.focus({preventScroll: true});
    };
  }, [backdrop, dialog, enabled, previous]);
}
