import {workspaceCopy} from './workspaceCopy';
import {studioMessages} from './studioMessages';
import {shortcutCopy} from './shortcutHelp';
import {round8GalleryCopy} from './round8GalleryCopy';
import {round8KnowledgeCopy} from './round8KnowledgeCopy';
import {round8UiCopy} from './round8UiCopy';
import {readFileSync} from 'node:fs';
import {describe, expect, it} from 'vitest';
import {bookMessages} from './bookMessages';
import {comicMessages} from './comicMessages';
import {collectionMessages} from './collectionMessages';
import {backupMessages} from './backupMessages';
import {incrementalMessages} from './incrementalMessages';
import {layoutMessages} from './layoutMessages';
import {next6Messages} from './next6Messages';
import {nextReleaseMessages} from './nextReleaseMessages';
import {phase3Messages} from './phase3Messages';
import {posterMessages} from './posterMessages';
import {readerFeatureMessages} from './readerFeatureMessages';
import {translate, type MessageKey} from './i18n';
import type {AppLanguage} from '../types/media';

const locales: AppLanguage[] = ['zh-CN', 'en-US', 'ja-JP', 'ko-KR'];
type Bundle = Record<AppLanguage, Record<string, string>>;
const bundles: Array<[string, Bundle]> = [
  ['workspaceCopy',workspaceCopy], ['shortcutCopy', shortcutCopy], ['round8GalleryCopy', round8GalleryCopy],
  ['round8KnowledgeCopy', round8KnowledgeCopy], ['round8UiCopy', round8UiCopy],
  ['studioMessages', studioMessages], ['bookMessages', bookMessages], ['comicMessages', comicMessages],
  ['collectionMessages', collectionMessages], ['backupMessages', backupMessages],
  ['incrementalMessages', incrementalMessages], ['layoutMessages', layoutMessages],
  ['next6Messages', next6Messages], ['nextReleaseMessages', nextReleaseMessages],
  ['phase3Messages', phase3Messages], ['posterMessages', posterMessages],
  ['readerFeatureMessages', readerFeatureMessages],
];
const placeholders = (value: string) =>
  [...new Set([...value.matchAll(/\{([a-zA-Z0-9_]+)\}/g)].map(match => match[1]))].sort();

describe('independent four-language message bundles', () => {
  it.each(bundles)('%s has identical keys, nonempty values and interpolation parameters', (_name, bundle) => {
    expect(Object.keys(bundle).sort()).toEqual([...locales].sort());
    const keys = Object.keys(bundle['zh-CN']).sort();
    expect(keys.length).toBeGreaterThan(0);
    for (const locale of locales) {
      expect(Object.keys(bundle[locale]).sort(), locale).toEqual(keys);
      for (const key of keys) {
        const value = bundle[locale][key];
        expect(typeof value, locale + ': ' + key).toBe('string');
        expect(value.trim(), locale + ': ' + key).not.toBe('');
        expect(placeholders(value), locale + ': ' + key).toEqual(placeholders(bundle['zh-CN'][key]));
      }
    }
  });
});

// Inspect explicit dictionaries as text: translating at runtime alone would hide
// untranslated keys behind ...zhCN / ...enUS fallback. The scanner does not execute source text.
const source = readFileSync(new URL('./i18n.tsx', import.meta.url), 'utf8');
const declarations = [...source.matchAll(/^const (zhCN|enUSBase|enUS|jaJPBase|jaJP|koKRBase|koKR|resources)\b/gm)];
const explicit: Record<string, Record<string, string>> = {};
for (let index = 0; index < declarations.length - 1; index++) {
  const declaration = declarations[index];
  const name = declaration[1].replace(/Base$/, '');
  const dictionary = explicit[name] ??= {};
  const body = source.slice(declaration.index, declarations[index + 1].index);
  // The source's dictionary entries use JSON-compatible quoted literals. Later
  // local overrides intentionally replace earlier Base entries in source order.
  for (const match of body.matchAll(/"((?:\\.|[^"\\])+)"\s*:\s*"((?:\\.|[^"\\])*)"/g)) {
    dictionary[JSON.parse('"' + match[1] + '"')] = JSON.parse('"' + match[2] + '"');
  }
}
const names: Record<AppLanguage, string> = {'zh-CN': 'zhCN', 'en-US': 'enUS', 'ja-JP': 'jaJP', 'ko-KR': 'koKR'};

// Existing exceptions are measured and reported in CODEX_SECURITY_AUDIT_20261115.md,
// not silently accepted as complete independent translations. Any new exception
// fails this audit. These inherited values are shared brand/technical constants.
const knownInheritedKeys = [
  'bangumi.subjectId', 'brand.author', 'brand.name', 'brand.subtitle',
  'common.bangumi', 'common.notAvailable', 'provider.bangumi', 'settings.playerPlaceholder',
];
const knownEmptyKeys = ['search.placeholder', 'search.promptDescription'];

describe('base i18n dictionaries: static audit plus runtime interpolation', () => {
  it('finds all four dictionaries and their Base/override blocks without an empty scan passing', () => {
    expect(declarations.map(match => match[1])).toEqual([
      'zhCN', 'enUSBase', 'enUS', 'jaJPBase', 'jaJP', 'koKRBase', 'koKR', 'resources',
    ]);
    expect(Object.keys(explicit).sort()).toEqual(['enUS', 'jaJP', 'koKR', 'zhCN']);
    for (const dictionary of Object.values(explicit)) expect(Object.keys(dictionary).length).toBeGreaterThan(500);
  });

  it.each(locales)('%s introduces no additional fallback, orphan key, empty string or parameter mismatch', locale => {
    const reference = explicit.zhCN, dictionary = explicit[names[locale]];
    const keys = Object.keys(reference).sort();
    expect(keys.filter(key => !(key in dictionary))).toEqual(locale === 'zh-CN' ? [] : knownInheritedKeys);
    expect(Object.keys(dictionary).filter(key => !(key in reference))).toEqual([]);
    expect(Object.keys(dictionary).filter(key => dictionary[key].trim() === '').sort()).toEqual(knownEmptyKeys);
    for (const key of keys) {
      const value = dictionary[key] ?? reference[key];
      expect(placeholders(value), locale + ': ' + key).toEqual(placeholders(reference[key]));
      // This also verifies the actual composed resource and shared fallback.
      expect(translate(locale, key as MessageKey), locale + ': ' + key).toBe(value);
      const values = Object.fromEntries(placeholders(reference[key]).map(name => [name, 'TEST_' + name]));
      const rendered = translate(locale, key as MessageKey, values);
      expect(placeholders(rendered), locale + ': ' + key).toEqual([]);
      for (const replacement of Object.values(values)) expect(rendered).toContain(replacement);
      if (!knownEmptyKeys.includes(key)) expect(rendered.trim(), locale + ': ' + key).not.toBe('');
    }
  });
});

