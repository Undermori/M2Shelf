import { useEffect, useLayoutEffect, useRef, useState } from "react";
import {createPortal} from 'react-dom';
import { Icon } from "./Icon";
import { useI18n } from "../lib/i18n";

export type ToastKind = "success" | "error" | "warning" | "info";

export interface ToastMessage {
  id: number;
  kind: ToastKind;
  text: string;
}

interface ToastStackProps {
  toasts: ToastMessage[];
  onDismiss: (id: number) => void;
}

export function ToastStack({ toasts, onDismiss }: ToastStackProps) {
  const { t } = useI18n();
  const [portalTarget, setPortalTarget] = useState<HTMLElement>(() => document.body);
  const stack = useRef<HTMLDivElement>(null);
  const supportsPopover = typeof HTMLElement.prototype.showPopover === 'function';
  // Native dialogs make everything outside themselves inert. Keep the single
  // notification region in the active dialog, including dialogs opened later.
  useLayoutEffect(() => {
    const update = () => setPortalTarget([...document.querySelectorAll<HTMLDialogElement>('dialog[open]')].at(-1) ?? document.body);
    update();
    const observer = new MutationObserver(update);
    observer.observe(document.body, {subtree: true, childList: true, attributes: true, attributeFilter: ['open']});
    return () => observer.disconnect();
  }, []);
  useLayoutEffect(() => {
    const region = stack.current;
    if (!supportsPopover || !region) return;
    // A moved region enters the dialog's top layer. Keep an already-open region
    // stable so receiving another notice never steals keyboard focus.
    const open = region.matches(':popover-open');
    if (!toasts.length && open) region.hidePopover();
    else if (toasts.length && !open) region.showPopover();
  }, [portalTarget, supportsPopover, toasts]);
  const priorFocus = useRef<HTMLElement | null>(null);
  const dismiss = useRef(onDismiss);
  dismiss.current = onDismiss;
  const interaction = useRef({ pointer: false, focus: false });
  const clocks = useRef(new Map<number, { remaining: number; started: number; timer: number | null; expired: boolean }>());

  const schedule = () => {
    const paused = interaction.current.pointer || interaction.current.focus;
    for (const [id, clock] of clocks.current) {
      if (paused && clock.timer !== null) {
        window.clearTimeout(clock.timer);
        clock.remaining = Math.max(0, clock.remaining - (Date.now() - clock.started));
        clock.timer = null;
      } else if (!paused && clock.timer === null && !clock.expired) {
        clock.started = Date.now();
        clock.timer = window.setTimeout(() => {
          clock.timer = null;
          clock.expired = true;
          dismiss.current(id);
        }, clock.remaining);
      }
    }
  };
  useEffect(() => {
    const ids = new Set(toasts.map(toast => toast.id));
    for (const [id, clock] of clocks.current) {
      if (!ids.has(id)) {
        if (clock.timer !== null) window.clearTimeout(clock.timer);
        clocks.current.delete(id);
      }
    }
    for (const {id, kind} of toasts) {
      if (!clocks.current.has(id)) clocks.current.set(id, { remaining: kind === 'error' || kind === 'warning' ? 5000 : 3500, started: 0, timer: null, expired: false });
    }
    if (!toasts.length) interaction.current = {pointer: false, focus: false};
    // Removing a focused toast externally (warning withdrawal/stack limit) may
    // not dispatch blur. Reconcile against the committed DOM before scheduling.
    if (interaction.current.focus && !stack.current?.contains(document.activeElement)) {
      interaction.current.focus = false;
      if (document.activeElement === document.body && priorFocus.current?.isConnected) priorFocus.current.focus();
    }
    schedule();
  }, [toasts]);
  useEffect(() => () => {
    for (const clock of clocks.current.values()) {
      if (clock.timer !== null) window.clearTimeout(clock.timer);
    }
    clocks.current.clear();
  }, []);
  const pointer = (inside: boolean) => { interaction.current.pointer = inside; schedule(); };
  const close = (id: number, button: HTMLButtonElement) => {
    if (document.activeElement === button) {
      const next = Array.from(stack.current?.querySelectorAll<HTMLButtonElement>('button') ?? []).find(item => item !== button);
      if (next) next.focus();
      else if (priorFocus.current?.isConnected) priorFocus.current.focus();
    }
    onDismiss(id);
  };
  return createPortal(
    <div ref={stack} className="toast-stack" data-toast-layer="true" popover={supportsPopover ? 'manual' : undefined} role="region" aria-label={t("common.notifications")} aria-live="polite" aria-relevant="additions text"
      onPointerEnter={() => pointer(true)} onPointerLeave={() => pointer(false)}
      onMouseEnter={() => pointer(true)} onMouseLeave={() => pointer(false)}
      onFocusCapture={event => {
        if (!event.currentTarget.contains(event.relatedTarget)) priorFocus.current = event.relatedTarget instanceof HTMLElement ? event.relatedTarget : null;
        interaction.current.focus = true;
        schedule();
      }}
      onBlurCapture={event => {
        if (!event.currentTarget.contains(event.relatedTarget)) { interaction.current.focus = false; schedule(); }
      }}>
      {toasts.map((toast) => (
        <div className={`toast toast-${toast.kind}`} key={toast.id}>
          <Icon name={toast.kind === "error" || toast.kind === "warning" ? "warning" : toast.kind === "success" ? "check" : "info"} />
          <span>{toast.text}</span>
          <button aria-label={t("common.closeNotice")} onClick={event => close(toast.id, event.currentTarget)} type="button"><Icon name="close" /></button>
        </div>
      ))}
    </div>, portalTarget
  );
}
