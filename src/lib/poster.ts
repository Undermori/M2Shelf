const NON_POSTER_ASPECT_RATIO = 0.9;

export function shouldContainPosterArtwork(naturalWidth: number, naturalHeight: number): boolean {
  return naturalWidth > 0
    && naturalHeight > 0
    && naturalWidth / naturalHeight >= NON_POSTER_ASPECT_RATIO;
}

