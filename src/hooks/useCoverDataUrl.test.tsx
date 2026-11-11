// @vitest-environment jsdom
import {act,cleanup,renderHook as renderUnmeasuredHook,waitFor} from '@testing-library/react';
import {afterEach,expect,it,vi} from 'vitest';
import type {MediaNode} from '../types/media';
import type {ComicBook} from '../types/comic';
import {useCoverDataUrl} from './useCoverDataUrl';
import {invalidatePosterResources,observePosterCacheDirectory,markPosterNodesChanged} from '../lib/posterResource';
const {getCoverDataUrl,pdfCoverFallback}=vi.hoisted(()=>({getCoverDataUrl:vi.fn(),pdfCoverFallback:vi.fn()}));
vi.mock('../lib/api',()=>({desktopAvailable:true,api:{getCoverDataUrl}}));
vi.mock('../lib/pdfCoverFallback',()=>({pdfCoverFallback}));
afterEach(()=>{cleanup();vi.clearAllMocks();});
const bookNode = (id: number): MediaNode => ({
  id, libraryRootId: 1, parentNodeId: 1, mediaKind: 'DOUJIN', nodeType: 'WORK',
  displayName: 'Image collection', folderName: 'Image collection', absolutePath: 'X:/Fixtures/Images',
  manualTypeOverride: false, createdAt: '2026-01-01', updatedAt: '2026-01-01',
  lastSeenAt: '2026-01-01', latestFileModifiedAt: '2026-01-01', totalComicBookCount: 1,
  totalVideoCount: 0, userTags: [], binding: null, coverSource: 'PLACEHOLDER', coverCachePath: null,
});



function renderHook<T,P>(callback:(props:P)=>T,options?:{initialProps:P}){
 const view=renderUnmeasuredHook(callback,options);const frame=document.createElement('span');Object.defineProperty(frame,'getBoundingClientRect',{value:()=>({width:400})});
 const ref=(view.result.current as {coverFrameRef?:(el:HTMLElement)=>void}).coverFrameRef;if(ref)act(()=>ref(frame));return view;
}

const manual=(id:number):MediaNode=>({...bookNode(id),mediaKind:'ANIMATION',coverSource:'MANUAL',coverCachePath:'X:/SyntheticCache/cover.png'});
it('immediately assigns ready local resource URLs even before the scroll observer activates',()=>{
 const view=renderHook(()=>useCoverDataUrl(manual(9001),0,false));
 expect(view.result.current.coverUrl).toContain('http://m2poster.localhost/node/9001/512?v=');expect(getCoverDataUrl).not.toHaveBeenCalled();
});
it('keeps src across scroll release, route remount and more than 128 other posters',()=>{
 const view=renderHook(({enabled})=>useCoverDataUrl(manual(9002),0,enabled),{initialProps:{enabled:true}});const src=view.result.current.coverUrl;
 view.rerender({enabled:false});expect(view.result.current.coverUrl).toBe(src);view.unmount();
 for(let id=9100;id<9300;id++){const other=renderHook(()=>useCoverDataUrl(manual(id),0));expect(other.result.current.coverUrl).toBeTruthy();other.unmount();}
 const returned=renderHook(()=>useCoverDataUrl(manual(9002),0,false));expect(returned.result.current.coverUrl).toBe(src);expect(getCoverDataUrl).not.toHaveBeenCalled();
});
it('does not change a cached image URL for unrelated metadata but does for an explicit cover change',()=>{
 const node=manual(9003),view=renderHook(({node})=>useCoverDataUrl(node,0),{initialProps:{node}}),src=view.result.current.coverUrl;
 view.rerender({node:{...node,updatedAt:'later',displayName:'New title'}});expect(view.result.current.coverUrl).toBe(src);
 view.rerender({node:{...node,clientCoverRevision:1}});expect(view.result.current.coverUrl).not.toBe(src);
});
it('does not manufacture a cover for a video without a cover source',()=>{
 const view=renderHook(()=>useCoverDataUrl({...manual(9004),coverSource:'PLACEHOLDER',coverCachePath:null},0));expect(view.result.current.coverUrl).toBeNull();expect(getCoverDataUrl).not.toHaveBeenCalled();
});
it('retains real image failure feedback and a changed revision can retry',()=>{
 const view=renderHook(({rev})=>useCoverDataUrl(manual(9005),rev),{initialProps:{rev:0}});
 act(()=>view.result.current.onCoverError());expect(view.result.current.coverFailed).toBe(true);expect(view.result.current.coverUrl).toBeNull();
 view.rerender({rev:1});expect(view.result.current.coverFailed).toBe(false);expect(view.result.current.coverUrl).toBeTruthy();
});
it('uses the current typed book revision and invalidates a selected manual book cover',()=>{
 const book={id:9006,revision:'revision one',documentFormat:'PDF'} as ComicBook;
 const view=renderHook(()=>useCoverDataUrl(null,0,true,book)),src=view.result.current.coverUrl;expect(src).toContain('/book/9006/512?');expect(src).toContain('revision=revision%20one');
 act(()=>window.dispatchEvent(new CustomEvent('m2shelf-cover-changed',{detail:{target:{kind:'BOOK',id:9006}}})));expect(view.result.current.coverUrl).not.toBe(src);
});
it('prepares a missing PDF once on demand and finishes when it goes offscreen',async()=>{
 let finish!:(ready:boolean)=>void;pdfCoverFallback.mockImplementation(()=>new Promise<boolean>(resolve=>finish=resolve));const node=bookNode(9007);
 const view=renderHook(({enabled})=>useCoverDataUrl(node,0,enabled),{initialProps:{enabled:false}});const src=view.result.current.coverUrl;
 act(()=>view.result.current.onCoverError());expect(pdfCoverFallback).not.toHaveBeenCalled();view.rerender({enabled:true});await waitFor(()=>expect(pdfCoverFallback).toHaveBeenCalledOnce());view.rerender({enabled:false});
 await act(async()=>finish(true));expect(view.result.current.coverUrl).toBeTruthy();expect(view.result.current.coverUrl).not.toBe(src);expect(view.result.current.coverLoading).toBe(false);
});
it('discards an old PDF completion after a different indexed source is shown',async()=>{
 let finish!:(ready:boolean)=>void;pdfCoverFallback.mockImplementation(()=>new Promise<boolean>(resolve=>finish=resolve));
 const view=renderHook(({node})=>useCoverDataUrl(node,0),{initialProps:{node:bookNode(9008)}});act(()=>view.result.current.onCoverError());await waitFor(()=>expect(pdfCoverFallback).toHaveBeenCalledOnce());
 view.rerender({node:manual(9009)});const src=view.result.current.coverUrl;await act(async()=>finish(true));expect(view.result.current.coverUrl).toBe(src);expect(view.result.current.coverUrl).toContain('/node/9009/');
});
it('invalidates browser resource keys on cache clear/location changes while keeping stable reads unchanged',()=>{
 act(()=>observePosterCacheDirectory('X:/SyntheticCache/first'));const view=renderHook(()=>useCoverDataUrl(manual(9010),0));const first=view.result.current.coverUrl;
 act(()=>observePosterCacheDirectory('X:/SyntheticCache/first'));expect(view.result.current.coverUrl).toBe(first);
 act(()=>observePosterCacheDirectory('X:/SyntheticCache/second'));const second=view.result.current.coverUrl;expect(second).not.toBe(first);
 act(()=>invalidatePosterResources());expect(view.result.current.coverUrl).not.toBe(second);
});

it('never assigns an unmeasured default tier before the real frame is known',()=>{
 const view=renderUnmeasuredHook(()=>useCoverDataUrl(manual(9050),0));expect(view.result.current.coverUrl).toBeNull();
 const frame=document.createElement('span');Object.defineProperty(frame,'getBoundingClientRect',{value:()=>({width:180})});act(()=>view.result.current.coverFrameRef(frame));
 expect(view.result.current.coverUrl).toContain('/node/9050/256?');expect(getCoverDataUrl).not.toHaveBeenCalled();
});

it('retains an explicit provider-cover revision across fresh Node DTOs and route remounts',()=>{
 const first=renderHook(()=>useCoverDataUrl(manual(9060),0));const old=first.result.current.coverUrl;const unaffected=renderHook(()=>useCoverDataUrl(manual(9061),0));const other=unaffected.result.current.coverUrl;
 act(()=>markPosterNodesChanged([9060]));const changed=first.result.current.coverUrl;expect(changed).not.toBe(old);expect(unaffected.result.current.coverUrl).toBe(other);first.unmount();
 const returned=renderHook(()=>useCoverDataUrl({...manual(9060)},0));expect(returned.result.current.coverUrl).toBe(changed);
});
