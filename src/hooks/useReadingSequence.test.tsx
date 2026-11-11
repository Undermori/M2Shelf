// @vitest-environment jsdom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {act,cleanup,renderHook} from '@testing-library/react';
import {I18nProvider} from '../lib/i18n';
import {useReadingSequence} from './useReadingSequence';
import type {ComicBook} from '../types/comic';
const calls=vi.hoisted(()=>({open:vi.fn()}));vi.mock('../lib/api',()=>({api:{openComicBook:calls.open}}));
const book=(id:number,name:string):ComicBook=>({id,nodeId:1,sourceKind:'ZIP_ARCHIVE',displayName:name,sourcePath:`X:/Fixture/${name}`,revision:`rev${id}`,pageCount:3,modifiedAt:'fixture',progress:null,indexError:null});
const books=[book(8,'chapter0.pdf'),book(2,'chapter10.cbz'),book(9,'chapter200.cbz')];
const wrapper=({children}:{children:React.ReactNode})=><I18nProvider>{children}</I18nProvider>;
beforeEach(()=>{vi.resetAllMocks();calls.open.mockResolvedValue({book:{...books[1],progress:{comicBookId:2,lastPageIndex:1,lastReadAt:'saved'}},pages:[],bookmarks:[]});});afterEach(cleanup);
it('follows visible order with no volume-number inference and restores the target position',async()=>{
 const flush=vi.fn().mockResolvedValue(true),open=vi.fn();const {result}=renderHook(()=>useReadingSequence(books[0],books),{wrapper});expect(result.current.next?.id).toBe(2);
 await act(()=>result.current.advance(flush,open));expect(calls.open.mock.calls.map(c=>c[0])).toEqual([2,2]);expect(open.mock.calls[0][0].progress.lastPageIndex).toBe(1);expect(flush).toHaveBeenCalledOnce();
});
it('stays on the current book if saving or opening fails',async()=>{
 const open=vi.fn();const {result}=renderHook(()=>useReadingSequence(books[0],books),{wrapper});await act(()=>result.current.advance(async()=>false,open));expect(open).not.toHaveBeenCalled();expect(result.current.error).toBeTruthy();
 calls.open.mockRejectedValue(new Error('COMIC_READ_FAILED'));await act(()=>result.current.advance(async()=>true,open));expect(open).not.toHaveBeenCalled();
});
it('never skips the last entry or an invalid revision',async()=>{
 const open=vi.fn();const {result}=renderHook(()=>useReadingSequence(books[2],books),{wrapper});expect(result.current.next).toBeNull();await act(()=>result.current.advance(async()=>true,open));expect(calls.open).not.toHaveBeenCalled();
});
it('rejects a target revision that changes while the current position is saving',async()=>{
 calls.open.mockResolvedValueOnce({book:books[1]}).mockResolvedValueOnce({book:{...books[1],revision:'changed'}});const open=vi.fn();const {result}=renderHook(()=>useReadingSequence(books[0],books),{wrapper});await act(()=>result.current.advance(async()=>true,open));expect(open).not.toHaveBeenCalled();expect(result.current.error).toBeTruthy();
});
