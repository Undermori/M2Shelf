// @vitest-environment jsdom
import {afterEach,beforeEach,describe,expect,it,vi} from 'vitest';
import {act,cleanup,renderHook,waitFor} from '@testing-library/react';
import {I18nProvider} from '../lib/i18n';
import {knownBookEnd,useNextVolume} from './useNextVolume';
import type {ComicBook} from '../types/comic';
const mocks=vi.hoisted(()=>({lookup:vi.fn(),open:vi.fn(),confirm:vi.fn()}));
vi.mock('../lib/api',()=>({api:{nextVolume:mocks.lookup,openComicBook:mocks.open,confirmNextVolume:mocks.confirm}}));
const book:ComicBook={id:710,nodeId:71,sourceKind:'IMAGE_FOLDER',displayName:'Atlas Vol.02',revision:'a',pageCount:4,modifiedAt:'2026-01-01',indexError:null,progress:null};
const next={...book,id:711,revision:'b',displayName:'Atlas Vol.03'};
const result={status:'READY',current:book,next,currentVolume:2,nextVolume:3,snapshot:'one'};
const wrapper=({children}:{children:React.ReactNode})=><I18nProvider>{children}</I18nProvider>;
beforeEach(()=>{vi.resetAllMocks();mocks.lookup.mockResolvedValue(result);mocks.open.mockResolvedValue({book:next,pages:[],bookmarks:[]});mocks.confirm.mockResolvedValue(result);});
afterEach(cleanup);
describe('next volume switch boundary',()=>{
 it('flushes immediately before the debounce and opens only after confirmation',async()=>{
  const flush=vi.fn().mockResolvedValue(true),open=vi.fn();const {result:hook}=renderHook(()=>useNextVolume(book),{wrapper});
  await waitFor(()=>expect(hook.current.result?.next?.id).toBe(711));
  await act(()=>hook.current.advance(flush,open));
  expect(flush).toHaveBeenCalledOnce();expect(mocks.confirm).toHaveBeenCalledWith(710,'a',711,'one');expect(open).toHaveBeenCalledWith(next);
  expect(flush.mock.invocationCallOrder[0]).toBeLessThan(mocks.confirm.mock.invocationCallOrder[0]);
 });
 it('keeps the current book when saving fails',async()=>{
  const open=vi.fn();const {result:hook}=renderHook(()=>useNextVolume(book),{wrapper});await waitFor(()=>expect(hook.current.result).toBeTruthy());
  await act(()=>hook.current.advance(async()=>false,open));expect(open).not.toHaveBeenCalled();expect(mocks.confirm).not.toHaveBeenCalled();expect(hook.current.error).toBeTruthy();
 });
 it('rejects a changed series snapshot before opening another book',async()=>{
  const open=vi.fn();const {result:hook}=renderHook(()=>useNextVolume(book),{wrapper});await waitFor(()=>expect(hook.current.result).toBeTruthy());mocks.lookup.mockResolvedValue({...result,snapshot:'changed'});
  await act(()=>hook.current.advance(async()=>true,open));expect(mocks.open).not.toHaveBeenCalled();expect(open).not.toHaveBeenCalled();
 });
 it('accepts a target adapter revision refreshed by its first valid open',async()=>{
  const fresh={...next,revision:'b2'};const updated={...result,next:fresh,snapshot:'two'};mocks.lookup.mockResolvedValueOnce(result).mockResolvedValueOnce(result).mockResolvedValue(updated);mocks.open.mockResolvedValue({book:fresh});mocks.confirm.mockResolvedValue(updated);
  const open=vi.fn();const {result:hook}=renderHook(()=>useNextVolume(book),{wrapper});await waitFor(()=>expect(hook.current.result).toBeTruthy());await act(()=>hook.current.advance(async()=>true,open));
  expect(mocks.confirm).toHaveBeenCalledWith(710,'a',711,'two');expect(open).toHaveBeenCalledWith(fresh);
 });
 it('does not infer a text ending from the last chapter number',()=>{expect(knownBookEnd({...book,id:899,documentFormat:'EPUB',progress:{comicBookId:899,lastPageIndex:3,lastReadAt:''}})).toBe(false);expect(knownBookEnd({...book,id:899,documentFormat:'PDF',progress:{comicBookId:899,lastPageIndex:3,lastReadAt:''}})).toBe(true);});
});
