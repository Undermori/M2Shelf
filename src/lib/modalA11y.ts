interface SiblingState {
  element: HTMLElement;
  inert: string | null;
  ariaHidden: string | null;
}

/**
 * Keep background UI out of both keyboard and assistive-technology navigation while a modal is
 * open. A MutationObserver also covers banners or toasts mounted after the dialog itself.
 */
export function isolateModalSiblings(modalBackdrop: HTMLElement | null): () => void {
  const container = modalBackdrop?.parentElement;
  if (!modalBackdrop || !container) return () => undefined;

  const isolated = new Map<HTMLElement, SiblingState>();
  const isolate = (element: Element) => {
    if (!(element instanceof HTMLElement) || element === modalBackdrop || element.matches('[data-toast-layer]') || isolated.has(element)) return;
    isolated.set(element, {
      element,
      inert: element.getAttribute("inert"),
      ariaHidden: element.getAttribute("aria-hidden"),
    });
    element.setAttribute("inert", "");
    element.setAttribute("aria-hidden", "true");
  };

  Array.from(container.children).forEach(isolate);
  const observer = new MutationObserver((records) => {
    records.forEach((record) => record.addedNodes.forEach((node) => {
      if (node instanceof Element) isolate(node);
    }));
  });
  observer.observe(container, { childList: true });

  return () => {
    observer.disconnect();
    isolated.forEach(({ element, inert, ariaHidden }) => {
      if (inert == null) element.removeAttribute("inert");
      else element.setAttribute("inert", inert);
      if (ariaHidden == null) element.removeAttribute("aria-hidden");
      else element.setAttribute("aria-hidden", ariaHidden);
    });
  };
}
