import { useEffect, useRef, useState, type RefObject } from "react";
import { observePosterViewport } from "../lib/posterViewportObserver";

interface PosterViewportLifecycleOptions {
  activationMarginPx: number;
  retentionEnabled?: boolean;
  retentionMarginPx: number;
}

interface PosterViewportLifecycle {
  coverRequested: boolean;
  coverVisible: boolean;
}

interface RetainedPoster {
  nearViewport: boolean;
  release: () => void;
}

const MAX_RETAINED_POSTERS = 64;
const retainedPosters = new Map<symbol, RetainedPoster>();

function trimRetainedPosters() {
  while (retainedPosters.size > MAX_RETAINED_POSTERS) {
    let oldestOffscreen: [symbol, RetainedPoster] | undefined;
    for (const entry of retainedPosters) {
      if (!entry[1].nearViewport) { oldestOffscreen = entry; break; }
    }
    if (!oldestOffscreen) return;
    const [token, poster] = oldestOffscreen;
    retainedPosters.delete(token);
    poster.release();
  }
}

function retainPoster(token: symbol, poster: RetainedPoster) {
  retainedPosters.delete(token);
  retainedPosters.set(token, poster);
  trimRetainedPosters();
}

function updateRetainedPosterProximity(token: symbol, nearViewport: boolean) {
  const poster = retainedPosters.get(token);
  if (!poster) return;
  poster.nearViewport = nearViewport;
  if (nearViewport) {
    retainedPosters.delete(token);
    retainedPosters.set(token, poster);
  }
  trimRetainedPosters();
}

function forgetRetainedPoster(token: symbol) {
  retainedPosters.delete(token);
}

/**
 * Preheat thumbnail data before it reaches the visible scrollport. A shared 64-entry target
 * retains request subscriptions across fast scroll reversals, evicting only the oldest one
 * outside the retention zone. Releasing a subscription does not replace the displayed image:
 * useCoverDataUrl retains it until the exact, bounded source-URL cache actually evicts it.
 */
export function usePosterViewportLifecycle<T extends Element>(
  targetRef: RefObject<T | null>,
  identity: string | number,
  {
    activationMarginPx,
    retentionEnabled = true,
    retentionMarginPx,
  }: PosterViewportLifecycleOptions,
): PosterViewportLifecycle {
  const retentionTokenRef = useRef(Symbol("retained-poster"));
  const [lifecycle, setLifecycle] = useState<PosterViewportLifecycle>({
    coverRequested: false,
    coverVisible: false,
  });

  useEffect(() => {
    const target = targetRef.current;
    let disposed = false;
    const retentionToken = retentionTokenRef.current;
    setLifecycle(current => !current.coverRequested && !current.coverVisible
      ? current : { coverRequested: false, coverVisible: false });

    const releaseRetainedPoster = () => {
      if (!disposed) setLifecycle({ coverRequested: false, coverVisible: false });
    };
    const activate = () => {
      if (disposed) return;
      if (retentionEnabled) {
        retainPoster(retentionToken, {
          nearViewport: true,
          release: releaseRetainedPoster,
        });
      }
      setLifecycle((current) => (
        current.coverRequested && current.coverVisible
          ? current
          : { coverRequested: true, coverVisible: true }
      ));
    };

    if (!target) {
      activate();
      return () => {
        disposed = true;
        forgetRetainedPoster(retentionToken);
      };
    }
    if (typeof IntersectionObserver === "undefined") {
      if (retentionEnabled) {
        retainPoster(retentionToken, {
          // Without geometry support the hard cap is safer than an unbounded soft overage.
          nearViewport: false,
          release: releaseRetainedPoster,
        });
      }
      setLifecycle({ coverRequested: true, coverVisible: true });
      return () => {
        disposed = true;
        forgetRetainedPoster(retentionToken);
      };
    }

    // The application scrolls inside this element rather than the browser window. Using it as
    // the explicit root makes rootMargin a real preheat distance instead of having it clipped
    // away by the overflow ancestor.
    const scrollRoot = target.closest(".content-scroll");
    const stopActivation = observePosterViewport(target, scrollRoot, activationMarginPx, nearViewport => {
      if (disposed) return;
      if (nearViewport) activate();
    });
    const stopRetention = observePosterViewport(target, scrollRoot, retentionMarginPx, nearViewport => {
      if (disposed) return;
      if (retentionEnabled) updateRetainedPosterProximity(retentionToken, nearViewport);
    });

    return () => {
      disposed = true;
      forgetRetainedPoster(retentionToken);
      stopActivation();
      stopRetention();
    };
  }, [activationMarginPx, identity, retentionEnabled, retentionMarginPx, targetRef]);

  return lifecycle;
}
