import { useLayoutEffect, useRef, useState } from "react";
import { preparedPosterGeometry,recordPosterGeometry,shouldContainPosterArtwork } from "../lib/poster";

interface PosterImageProps {
  active?: boolean;
  alt: string;
  cacheKey: string;
  onError: () => void;
  src: string;
}

/** Rust prepares persistent, DPR-sized thumbnails. Display that one image without a second
 * Canvas renderer or scroll-triggered replacement; the browser loads and caches the native raster resource directly. */
export function PosterImage({ active = true, alt, cacheKey, onError, src }: PosterImageProps) {
  const identity=cacheKey.split(':').slice(0,2).join(':');
  const [loaded, setLoaded] = useState({key: "", identity:"", src: "", contain: false});
  const ref=useRef<HTMLImageElement>(null);
  const prepared=preparedPosterGeometry(cacheKey);
  const retained=loaded.src!==""&&loaded.identity===identity;
  const ready = loaded.src === src||retained||!!prepared;
  const contain=loaded.src===src||retained?loaded.contain:prepared?shouldContainPosterArtwork(prepared.width,prepared.height):false;
  useLayoutEffect(()=>{const image=ref.current;if(image?.complete&&image.naturalWidth>0){recordPosterGeometry(cacheKey,image.naturalWidth,image.naturalHeight);setLoaded({key:cacheKey,identity,src,contain:shouldContainPosterArtwork(image.naturalWidth,image.naturalHeight)});}},[src,cacheKey,identity]);
  return (
    <img
      alt={alt}
      ref={ref}
      className={`poster-image${ready && contain ? " is-wide-artwork" : ""}`}
      decoding="sync"
      fetchPriority={active ? "auto" : "low"}
      loading="lazy"
      onError={onError}
      onLoad={(event) => {if(event.currentTarget.currentSrc&&event.currentTarget.currentSrc!==src)return;recordPosterGeometry(cacheKey,event.currentTarget.naturalWidth,event.currentTarget.naturalHeight);setLoaded({key: cacheKey, identity, src, contain: shouldContainPosterArtwork(
        event.currentTarget.naturalWidth, event.currentTarget.naturalHeight,
      )});}}
      src={src}
      style={{visibility: ready ? "visible" : "hidden"}}
    />
  );
}
