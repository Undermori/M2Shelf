// @vitest-environment jsdom
import { describe, expect, it } from 'vitest';
import { isShortcutHelpKey, shortcutCopy } from './shortcutHelp';

describe('shortcut help activation', () => {
  it('supports question-mark layouts without intercepting IME or modified commands', () => {
    expect(isShortcutHelpKey(new KeyboardEvent('keydown', {key: '?', shiftKey: true}))).toBe(true);
    expect(isShortcutHelpKey(new KeyboardEvent('keydown', {key: '/', code: 'Slash', shiftKey: true}))).toBe(true);
    for (const flags of [{isComposing:true}, {ctrlKey:true}, {metaKey:true}, {altKey:true}]) {
      expect(isShortcutHelpKey(new KeyboardEvent('keydown', {key: '?', ...flags}))).toBe(false);
    }
    const used = new KeyboardEvent('keydown', {key:'?',cancelable:true}); used.preventDefault();
    expect(isShortcutHelpKey(used)).toBe(false);
  });
  it('does not steal punctuation from an editor or an editable descendant', () => {
    for (const markup of ['<input>', '<textarea></textarea>', '<div contenteditable=""><span></span></div>']) {
      const host = document.createElement('div'); host.innerHTML = markup; document.body.append(host);
      let result = true; host.addEventListener('keydown', event => { result = isShortcutHelpKey(event); });
      host.querySelector('span,input,textarea')!.dispatchEvent(new KeyboardEvent('keydown', {key:'?',bubbles:true}));
      expect(result).toBe(false); host.remove();
    }
  });
  it('has a complete independent four-language dictionary', () => {
    expect(Object.keys(shortcutCopy)).toEqual(['zh-CN','en-US','ja-JP','ko-KR']);
    for (const copy of Object.values(shortcutCopy)) {
      expect(Object.keys(copy)).toEqual(Object.keys(shortcutCopy['zh-CN']));
      expect(Object.values(copy).every(value => value.trim().length > 0)).toBe(true);
    }
  });
});
