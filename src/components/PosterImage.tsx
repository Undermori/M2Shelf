import { useState } from "react";
import { shouldContainPosterArtwork } from "../lib/poster";

interface PosterImageProps {
  active?: boolean;
  alt: string;
  cacheKey: string;
  onError: () => void;
  src: string;
}

/** Rust prepares persistent, DPR-sized thumbnails. Display that one image without a second
 * Canvas renderer or scroll-triggered replacement; the parent already preheats bounded IPC. */
export function PosterImage({ active = true, alt, cacheKey, onError, src }: PosterImageProps) {
  const [loaded, setLoaded] = useState({key: "", src: "", contain: false});
  const ready = loaded.key === cacheKey && loaded.src === src;
  return (
    <img
      alt={alt}
      className={`poster-image${ready && loaded.contain ? " is-wide-artwork" : ""}`}
      decoding="async"
      fetchPriority={active ? "auto" : "low"}
      key={cacheKey}
      loading="eager"
      onError={onError}
      onLoad={(event) => setLoaded({key: cacheKey, src, contain: shouldContainPosterArtwork(
        event.currentTarget.naturalWidth, event.currentTarget.naturalHeight,
      )})}
      src={src}
      style={{visibility: ready ? "visible" : "hidden"}}
    />
  );
}
