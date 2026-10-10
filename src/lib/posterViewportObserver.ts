type VisibilityListener = (nearViewport: boolean) => void;

interface ObserverGroup {
  observer: IntersectionObserver;
  targets: Map<Element, Set<VisibilityListener>>;
}

const observerRoots = new Map<Element | null, Map<number, ObserverGroup>>();

/** Batch cards in the same scrollport instead of creating two observers per poster. */
export function observePosterViewport(
  target: Element,
  root: Element | null,
  marginPx: number,
  listener: VisibilityListener,
): () => void {
  let groups = observerRoots.get(root);
  if (!groups) {
    groups = new Map();
    observerRoots.set(root, groups);
  }
  let group = groups.get(marginPx);
  if (!group) {
    const targets = new Map<Element, Set<VisibilityListener>>();
    const observer = new IntersectionObserver((entries) => {
      for (const entry of entries) {
        // The last subscriber may have been removed since this batch was queued.
        targets.get(entry.target)?.forEach(callback => callback(entry.isIntersecting));
      }
    }, {root, rootMargin: `${marginPx}px 0px`});
    group = {observer, targets};
    groups.set(marginPx, group);
  }
  let listeners = group.targets.get(target);
  if (!listeners) {
    listeners = new Set();
    group.targets.set(target, listeners);
    group.observer.observe(target);
  }
  listeners.add(listener);
  const subscribedGroup = group;
  const subscribedGroups = groups;
  let subscribed = true;
  return () => {
    if (!subscribed) return;
    subscribed = false;
    listeners.delete(listener);
    if (listeners.size === 0) {
      subscribedGroup.targets.delete(target);
      subscribedGroup.observer.unobserve(target);
    }
    if (subscribedGroup.targets.size === 0) {
      subscribedGroup.observer.disconnect();
      subscribedGroups.delete(marginPx);
      if (subscribedGroups.size === 0) observerRoots.delete(root);
    }
  };
}
