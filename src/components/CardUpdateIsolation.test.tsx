// @vitest-environment jsdom
import {act,cleanup,render,screen} from '@testing-library/react';
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {I18nProvider} from '../lib/i18n';
import type {MediaNode} from '../types/media';
import type {ComicBook} from '../types/comic';

const state=vi.hoisted(()=>({value:{nodeId:1,workView:false,watched:0,total:12,started:false},completed:false,listeners:new Set<()=>void>(),coverReads:0}));
vi.mock('../lib/cardProgress',async original=>{
 const module=await original<typeof import('../lib/cardProgress')>(),react=await import('react');
 return {...module,useCardProgress:()=>react.useSyncExternalStore(listener=>{state.listeners.add(listener);return()=>state.listeners.delete(listener);},()=>state.value)};
});
vi.mock('../lib/collectionState',async()=>{
 const react=await import('react');return {useCollection:()=>{
  const completed=react.useSyncExternalStore(listener=>{state.listeners.add(listener);return()=>state.listeners.delete(listener);},()=>state.completed);
  return {nodeTargets:()=>[{kind:'NODE',id:1}],get:()=>completed?'COMPLETED':undefined};
 }};
});
vi.mock('../hooks/useCoverDataUrl',()=>({useCoverDataUrl:()=>{state.coverReads++;return {coverUrl:'m2poster://localhost/ready',coverCacheKey:'node:1:256',coverFailed:false,coverLoading:false};}}));
vi.mock('../hooks/usePosterViewportLifecycle',()=>({usePosterViewportLifecycle:()=>({coverRequested:true,coverVisible:true,visibleRef:{current:true}})}));
vi.mock('./PosterImage',()=>({PosterImage:({src,alt}:{src:string;alt:string})=><img src={src} alt={alt}/>}));
import {MediaCard} from './MediaCard';
import {BookPosterCard} from './BookPosterCard';
beforeEach(()=>{state.value={nodeId:1,workView:false,watched:0,total:12,started:false};state.completed=false;state.coverReads=0;});
afterEach(()=>{cleanup();state.listeners.clear();});
const notify=()=>{for(const f of state.listeners)f();};

it('updates video progress without reentering the cover hooks or replacing the image',()=>{
 const node={id:1,libraryRootId:1,parentNodeId:2,displayName:'Synthetic',folderName:'Synthetic',nodeType:'WORK',mediaKind:'ANIMATION',coverCachePath:'synthetic-cache',totalVideoCount:12,userTags:[]} as unknown as MediaNode;
 render(<I18nProvider><MediaCard node={node} viewMode="grid" coverRevision={0} onOpen={vi.fn()} onMenu={vi.fn()} onBangumi={vi.fn()} onRetryCover={vi.fn()}/></I18nProvider>);
 const image=screen.getByRole('img'),reads=state.coverReads;
 act(()=>{state.value={...state.value,watched:1,started:true};notify();});
 expect(document.querySelector('.card-summary')?.textContent).toContain('1/12');
 expect(screen.getByRole('img')).toBe(image);expect(state.coverReads).toBe(reads);
 act(()=>{state.completed=true;notify();});
 expect(document.querySelector('.card-summary')?.textContent).not.toContain('1/12');expect(state.coverReads).toBe(reads);
});

it('updates book collection status without rerendering its cover',()=>{
 const book={id:1,nodeId:1,pageCount:12,progress:null} as ComicBook;
 render(<I18nProvider><BookPosterCard book={book} title="Synthetic book" count={1} series={false} root={{mediaKind:'EBOOK'}} viewMode="grid" revision={0} onOpen={vi.fn()} onMenu={vi.fn()}/></I18nProvider>);
 const image=screen.getByRole('img'),reads=state.coverReads,before=document.querySelector('.card-summary')?.textContent;
 act(()=>{state.completed=true;notify();});
 expect(document.querySelector('.card-summary')?.textContent).not.toBe(before);
 expect(screen.getByRole('img')).toBe(image);expect(state.coverReads).toBe(reads);
});
