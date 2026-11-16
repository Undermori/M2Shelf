import {readFileSync} from 'node:fs';
import {describe,expect,it} from 'vitest';
import {DEFAULT_VIDEO_EXTENSIONS,hasDefaultVideoExtensions} from './videoExtensions';

describe('video extension defaults', () => {
  it('matches the native persisted defaults exactly', () => {
    const source=readFileSync(new URL('../../src-tauri/src/db.rs',import.meta.url),'utf8');
    const body=source.match(/fn default_video_extensions\(\)[^{]*\{([\s\S]*?)\n\}/)?.[1];
    expect(body).toBeTruthy();
    const native=[...body!.matchAll(/"([a-z0-9]+)"/g)].map(match=>`.${match[1]}`);
    expect(DEFAULT_VIDEO_EXTENSIONS).toEqual(native);
  });
  it('detects modified defaults while accepting native sorting and dot normalization', () => {
    expect(hasDefaultVideoExtensions(DEFAULT_VIDEO_EXTENSIONS)).toBe(true);
    expect(hasDefaultVideoExtensions(DEFAULT_VIDEO_EXTENSIONS.slice(1))).toBe(false);
    expect(hasDefaultVideoExtensions([...DEFAULT_VIDEO_EXTENSIONS,'.wmv'])).toBe(false);
    expect(hasDefaultVideoExtensions([...DEFAULT_VIDEO_EXTENSIONS].reverse())).toBe(true);
    expect(hasDefaultVideoExtensions(DEFAULT_VIDEO_EXTENSIONS.map(value=>value.slice(1)).sort())).toBe(true);
  });
});
