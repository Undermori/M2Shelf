// Keep in sync with db::default_video_extensions; parity is tested against Rust.
export const DEFAULT_VIDEO_EXTENSIONS = Object.freeze([
  '.mkv', '.mp4', '.m4v', '.avi', '.mov', '.webm', '.ts', '.m2ts',
]);

export function hasDefaultVideoExtensions(extensions: readonly string[]): boolean {
  // Native settings normalize away dots and sort on save; compare the same set.
  const normalized = new Set(extensions.map(extension => `.${extension.replace(/^\./, '').toLowerCase()}`));
  return extensions.length === DEFAULT_VIDEO_EXTENSIONS.length
    && DEFAULT_VIDEO_EXTENSIONS.every(extension => normalized.has(extension));
}
