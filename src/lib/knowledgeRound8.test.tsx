// @vitest-environment jsdom
import {act, cleanup, fireEvent, render, screen, waitFor} from '@testing-library/react';
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {CommandPalette, matchExcerpt} from '../components/CommandPalette';
import {I18nProvider} from './i18n';
import {searchKnowledge} from './knowledgeSearch';
import {noteMarkdown, markdownFilename} from './noteMarkdown';
import {takeStudioLink} from './openStudio';
import {SearchPage} from '../pages/SearchPage';
const mocks=vi.hoisted(()=>({search:vi.fn(),notes:vi.fn(),shots:vi.fn()}));
vi.mock('./api',()=>({desktopAvailable:false,api:{search:mocks.search,listWorkNotes:mocks.notes,screenshots:mocks.shots}}));
vi.mock('../hooks/useCoverDataUrl',()=>({useCoverDataUrl:()=>({coverUrl:null,coverCacheKey:'',coverFrameRef:{current:null}})}));
vi.mock('../hooks/usePosterViewportLifecycle',()=>({usePosterViewportLifecycle:()=>({coverRequested:false,coverVisible:false})}));
const note={target:{kind:'NODE' as const,id:70},nodeId:70,title:'Blue night',libraryId:1,libraryName:'Library',mediaKind:'ANIMATION' as const,body:'The Blue train',revision:1,updatedAt:'2026-11-16T00:00:00Z'};
const shot={id:1,fileName:'Blue.png',note:'Blue sky',tags:['Blue']};
beforeEach(()=>{vi.clearAllMocks();mocks.search.mockResolvedValue([]);mocks.notes.mockResolvedValue({items:[note],total:1});mocks.shots.mockResolvedValue({items:[shot],total:1});});
afterEach(()=>{cleanup();vi.useRealTimers();});
describe('Round 8 knowledge integration',()=>{
 it('bounds and highlights work note excerpts and counts the displayed mixed or single-source rows',async()=>{
  mocks.search.mockResolvedValue([{kind:'NODE',node:{id:70,libraryRootId:1,parentNodeId:1,displayName:'Blue work',folderName:'Blue work',absolutePath:'X:/Synthetic/Blue',mediaKind:'ANIMATION',nodeType:'WORK',totalVideoCount:1,userTags:[]},noteExcerpt:'Before '.repeat(1000)+'Blue memory'+' After'.repeat(1000)}]);
  render(<I18nProvider><SearchPage initialQuery="Blue" onOpen={()=>{}} onError={()=>{}} coverRevision={0}/></I18nProvider>);
  await screen.findByText('3 个结果');
  const excerpt=document.querySelector('.search-hit .note-excerpt')!;
  expect(excerpt.textContent!.length).toBeLessThan(170);expect(excerpt.querySelector('mark')?.textContent).toBe('Blue');
  for(const [source,selector] of [['笔记','.knowledge-results > button'],['截图','.knowledge-results > button'],['作品','.search-hit']]){
   fireEvent.click(screen.getByRole('combobox',{name:'全部来源'}));fireEvent.click(screen.getByRole('option',{name:source}));
   expect(screen.getByText('1 个结果')).toBeTruthy();expect(document.querySelectorAll(selector)).toHaveLength(1);
  }
 });
 it.each(['notes','screenshots'] as const)('shows a count when only %s match and no works match',async(source)=>{
  if(source==='notes')mocks.shots.mockResolvedValue({items:[],total:0});else mocks.notes.mockResolvedValue({items:[],total:0});
  render(<I18nProvider><SearchPage initialQuery="Blue" onOpen={()=>{}} onError={()=>{}} coverRevision={0}/></I18nProvider>);
  await screen.findByText('1 个结果');expect(document.querySelectorAll('.search-hit')).toHaveLength(0);expect(document.querySelectorAll('.knowledge-results > button')).toHaveLength(1);
 });
 it('uses portable filenames for Windows reserved names and separators',()=>{for(const name of ['CON','prn','NUL.txt','COM1','LPT9','AUX','...'])expect(markdownFilename(name)).toBe('M2Shelf-notes.md');expect(markdownFilename('A/B:C')).toBe('A-B-C.md');});
 it('keeps a deep body match visible in a bounded excerpt',()=>{const excerpt=matchExcerpt('a'.repeat(5000)+'Blue'+'b'.repeat(2000),'Blue');expect(excerpt).toContain('Blue');expect(excerpt.length).toBeLessThan(170);});
 it('budgets each source and preserves notes when the work search fails',async()=>{
  mocks.search.mockRejectedValue(new Error('synthetic failure'));
  const result=await searchKnowledge('Blue');expect(result.failed).toBe(true);expect(result.notes).toEqual([note]);expect(result.screenshots).toEqual([shot]);
  expect(mocks.notes).toHaveBeenCalledWith(expect.objectContaining({limit:12,notesOnly:true}));
 });
 it('opens a precise note target and highlights the matched text',async()=>{
  render(<I18nProvider><CommandPalette enabled actions={[]} onHit={()=>{}}/></I18nProvider>);
  fireEvent.keyDown(window,{key:'k',ctrlKey:true});fireEvent.change(screen.getByRole('combobox'),{target:{value:'Blue'}});
  const option=await screen.findByRole('option',{name:/笔记 Blue night The Blue train/});
  expect(option.querySelector('mark')?.textContent).toBe('Blue');fireEvent.click(option);
  expect(takeStudioLink()).toEqual({view:'notes',target:note.target,nodeId:70});
 });
 it('fences obsolete in-flight search completions before the next debounce fires',async()=>{
  let finish!:(value:unknown)=>void;mocks.notes.mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve;}));
  render(<I18nProvider><CommandPalette enabled actions={[]} onHit={()=>{}}/></I18nProvider>);
  fireEvent.keyDown(window,{key:'k',ctrlKey:true});const input=screen.getByRole('combobox');fireEvent.change(input,{target:{value:'old'}});
  await waitFor(()=>expect(mocks.notes).toHaveBeenCalled());fireEvent.change(input,{target:{value:'new'}});
  await act(async()=>finish({items:[{...note,title:'obsolete result'}],total:1}));expect(screen.queryByText('obsolete result')).toBeNull();
 });
 it('does not swallow arrows without a focus candidate',async()=>{
  render(<I18nProvider><CommandPalette enabled actions={[]} onHit={()=>{}}/></I18nProvider>);fireEvent.keyDown(window,{key:'k',ctrlKey:true});
  expect(fireEvent.keyDown(screen.getByRole('combobox'),{key:'ArrowDown'})).toBe(true);
 });
 it('exports title/library/time plus unchanged Markdown body without source paths in four languages',()=>{
  for(const locale of ['zh-CN','en-US','ja-JP','ko-KR'] as const){const markdown=noteMarkdown([{...note,title:'# Heading\nnew',body:'**bold**\n\n日本語 한국어 中文'}],locale);expect(markdown).toContain('# \\# Heading new');expect(markdown).toContain('Library');expect(markdown).toContain(note.updatedAt);expect(markdown).toContain('**bold**\n\n日本語 한국어 中文');expect(markdown).not.toContain('sourcePath');}
 });
});
