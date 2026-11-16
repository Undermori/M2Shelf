// @vitest-environment jsdom
import {afterEach,expect,it,vi} from 'vitest';
import {cleanup,fireEvent,render,screen,within} from '@testing-library/react';
import {I18nProvider} from '../lib/i18n';
import {DetailVideoTabs} from './DetailVideoTabs';
import type {NodeDetail,MediaFile,MediaNode,ResourceFile} from '../types/media';
import type {EpisodeWatchController} from './EpisodeWatchControl';
afterEach(cleanup);
const file=(id:number)=>({id,nodeId:2,fileName:`Video ${id}.mkv`,absolutePath:`X:/Synthetic/Video ${id}.mkv`,extension:'mkv',fileSize:1}) as MediaFile;
const detail={node:{id:2},mediaFiles:[file(1),file(2)],nestedMediaFiles:[{file:file(3),relativeDirectory:'Other'}],videoSections:[{fileId:1,section:'MAIN'},{fileId:2,section:'SPS'},{fileId:3,section:'OTHER'}]} as NodeDetail;
const watch={data:null,busy:false,playAndMark:vi.fn()} as unknown as EpisodeWatchController;
it('combines SP videos and resources, preserves exact callbacks and restores the selected SP tab',()=>{
 const spFile={id:20,fileName:'Commentary.mka',absolutePath:'X:/Synthetic/SPs/Commentary.mka',extension:'.mka',resourceType:'AUDIO',fileSize:10} as ResourceFile;
 const folder={id:21,folderName:'Menu scans',absolutePath:'X:/Synthetic/SPs/Menu scans',totalVideoCount:0} as MediaNode;
 const callbacks={onPlay:vi.fn(),onReveal:vi.fn(),onOpenResource:vi.fn(),onRevealResource:vi.fn(),onOpenChild:vi.fn(),onMenu:vi.fn(),onSectionChange:vi.fn()};
 const view=render(<I18nProvider><DetailVideoTabs detail={detail} watch={watch} cdsFiles={[]} cdsFolders={[]} spFiles={[spFile]} spFolders={[folder]} selectedSection="SPS" {...callbacks}/></I18nProvider>);
 expect(screen.getByRole('tab',{name:'SPs 3'}).getAttribute('aria-selected')).toBe('true');
 expect(screen.getByText('Video 2.mkv')).toBeTruthy();
 fireEvent.doubleClick(screen.getByText('Commentary.mka').closest('button')!);expect(callbacks.onOpenResource).toHaveBeenCalledWith(spFile);
 fireEvent.click(screen.getByText('Menu scans',{selector:'strong'}).closest('button')!);expect(callbacks.onOpenChild).toHaveBeenCalledWith(folder);
 view.rerender(<I18nProvider><DetailVideoTabs detail={{...detail,mediaFiles:[],nestedMediaFiles:[],videoSections:[]}} watch={watch} cdsFiles={[]} cdsFolders={[]} spFiles={[spFile]} spFolders={[folder]} selectedSection="SPS" {...callbacks}/></I18nProvider>);
 expect(screen.getByRole('tab',{name:'SPs 2'}).getAttribute('aria-selected')).toBe('true');
 expect(view.container.querySelector('.video-tabs-heading > small')).toBeNull();
 expect(screen.getByText('Commentary.mka')).toBeTruthy();
});
it('keeps every indexed file in one tab and preserves real play/reveal identity',()=>{
 const play=vi.fn(),reveal=vi.fn();const view=render(<I18nProvider><DetailVideoTabs cdsFiles={[]} cdsFolders={[]} onOpenResource={vi.fn()} onRevealResource={vi.fn()} onOpenChild={vi.fn()} onMenu={vi.fn()} detail={detail} watch={watch} onPlay={play} onReveal={reveal}/></I18nProvider>);
 expect(screen.getByText('Video 1.mkv')).toBeTruthy();expect(screen.queryByText('Video 2.mkv')).toBeNull();fireEvent.click(screen.getByRole('tab',{name:'SPs 1'}));expect(screen.getByText('Video 2.mkv')).toBeTruthy();expect(screen.queryByText('Video 1.mkv')).toBeNull();
 fireEvent.click(screen.getByRole('button',{name:'点击播放 Video 2.mkv'}));expect(play).toHaveBeenCalledWith(file(2));
 fireEvent.keyDown(screen.getByRole('tab',{name:'SPs 1'}),{key:'ArrowRight'});expect(screen.getByText('Video 3.mkv')).toBeTruthy();
 view.rerender(<I18nProvider><DetailVideoTabs cdsFiles={[]} cdsFolders={[]} onOpenResource={vi.fn()} onRevealResource={vi.fn()} onOpenChild={vi.fn()} onMenu={vi.fn()} detail={{...detail,node:{...detail.node,id:5}}} watch={watch} onPlay={play} onReveal={reveal}/></I18nProvider>);expect(screen.getByRole('tab',{name:'正片 1'}).getAttribute('aria-selected')).toBe('true');
});
it('omits the repeated SPs directory line while retaining file paths and actions',()=>{
 const view=render(<I18nProvider><DetailVideoTabs cdsFiles={[]} cdsFolders={[]} onOpenResource={vi.fn()} onRevealResource={vi.fn()} onOpenChild={vi.fn()} onMenu={vi.fn()} detail={{...detail,mediaFiles:[file(1)],nestedMediaFiles:[{file:file(2),sourceNodeId:2,sourceName:'Work',relativeDirectory:'SPs'}]}} watch={watch} onPlay={vi.fn()} onReveal={vi.fn()}/></I18nProvider>);fireEvent.click(screen.getByRole('tab',{name:'SPs 1'}));expect(view.container.querySelector('.file-origin')).toBeNull();expect(view.container.querySelector('.media-file-row')?.getAttribute('title')).toContain(file(2).absolutePath);expect(screen.getByRole('button',{name:'点击播放 Video 2.mkv'})).toBeTruthy();
});

it('renders sparse section deviations identically to complete section rows',()=>{
 const sparse={...detail,videoSections:[{fileId:2,section:'SPS' as const}]};
 render(<I18nProvider><DetailVideoTabs cdsFiles={[]} cdsFolders={[]} onOpenResource={vi.fn()} onRevealResource={vi.fn()} onOpenChild={vi.fn()} onMenu={vi.fn()} detail={sparse} watch={watch} onPlay={vi.fn()} onReveal={vi.fn()}/></I18nProvider>);
 expect(screen.getByRole('tab',{name:'正片 1'})).toBeTruthy();
 expect(screen.getByText('Video 1.mkv')).toBeTruthy();
 fireEvent.click(screen.getByRole('tab',{name:'SPs 1'}));expect(screen.getByText('Video 2.mkv')).toBeTruthy();
 fireEvent.click(screen.getByRole('tab',{name:'其他视频 1'}));expect(screen.getByText('Video 3.mkv')).toBeTruthy();
});

it('orders CDs after SPs and before Other with keyboard navigation and original callbacks',()=>{
 const play=vi.fn(),reveal=vi.fn();
 const cds={...detail,nestedMediaFiles:[...detail.nestedMediaFiles!,{file:file(4),sourceNodeId:2,sourceName:'Work',relativeDirectory:'CDs'}],videoSections:[...detail.videoSections!,{fileId:4,section:'CDS' as const}]};
 render(<I18nProvider><DetailVideoTabs cdsFiles={[]} cdsFolders={[]} onOpenResource={vi.fn()} onRevealResource={vi.fn()} onOpenChild={vi.fn()} onMenu={vi.fn()} detail={cds} watch={watch} onPlay={play} onReveal={reveal}/></I18nProvider>);
 expect(screen.getAllByRole('tab').map(tab=>tab.textContent)).toEqual(['正片1','SPs1','CDs1','其他视频1']);
 fireEvent.click(screen.getByRole('tab',{name:'SPs 1'}));
 fireEvent.keyDown(screen.getByRole('tab',{name:'SPs 1'}),{key:'ArrowRight'});
 expect(screen.getByRole('tab',{name:'CDs 1'}).getAttribute('aria-selected')).toBe('true');
 expect(screen.getByText('Video 4.mkv')).toBeTruthy();
 expect(screen.queryByText('Video 3.mkv')).toBeNull();
 fireEvent.click(screen.getByRole('button',{name:'点击播放 CDs / Video 4.mkv'}));expect(play).toHaveBeenCalledWith(file(4));
 fireEvent.click(screen.getByRole('button',{name:'在资源管理器中显示 CDs / Video 4.mkv'}));expect(reveal).toHaveBeenCalledWith(file(4));
 fireEvent.keyDown(screen.getByRole('tab',{name:'CDs 1'}),{key:'ArrowRight'});
 expect(screen.getByText('Video 3.mkv')).toBeTruthy();
 expect(screen.queryByText('Video 4.mkv')).toBeNull();
});

it('combines music videos, audio, booklets and folders in the same CDs panel with exact callbacks',()=>{
 const musicFile=file(4);
 const folder={id:9,folderName:'OST Album',absolutePath:'X:/Synthetic/CDs/OST Album',totalVideoCount:1} as MediaNode;
 const track={id:20,fileName:'Theme.flac',absolutePath:'X:/Synthetic/Theme.flac',extension:'.flac',resourceType:'AUDIO',fileSize:10} as ResourceFile;
 const booklet={...track,id:21,fileName:'Booklet.pdf',absolutePath:'X:/Synthetic/CDs/Booklet.pdf',extension:'.pdf',resourceType:'DOCUMENT' as const};
 const musicDetail={...detail,nestedMediaFiles:[...detail.nestedMediaFiles!,{file:musicFile,sourceNodeId:2,sourceName:'Work',relativeDirectory:'CDs'}],videoSections:[...detail.videoSections!,{fileId:4,section:'CDS' as const}],expandedFolderIds:[]};
 const callbacks={onPlay:vi.fn(),onReveal:vi.fn(),onOpenResource:vi.fn(),onRevealResource:vi.fn(),onOpenChild:vi.fn(),onMenu:vi.fn()};
 render(<I18nProvider><DetailVideoTabs detail={musicDetail} watch={watch} cdsFiles={[track,booklet]} cdsFolders={[folder]} {...callbacks}/></I18nProvider>);
 expect(screen.getAllByRole('tab').map(tab=>tab.textContent)).toEqual(['正片1','SPs1','CDs4','其他视频1']);
 expect(screen.queryByText('Theme.flac')).toBeNull();
 fireEvent.click(screen.getByRole('tab',{name:'CDs 4'}));
 const panel=screen.getByRole('tabpanel',{name:'CDs 4'});
 expect(screen.getAllByRole('tabpanel')).toHaveLength(1);
 expect(within(panel).getByText('Video 4.mkv')).toBeTruthy();
 expect(within(panel).getByText('Theme.flac')).toBeTruthy();
 expect(within(panel).getByText('Booklet.pdf')).toBeTruthy();
 expect(within(panel).queryByText('视频已在上方展示')).toBeNull();
 fireEvent.click(within(panel).getByRole('button',{name:'点击播放 CDs / Video 4.mkv'}));expect(callbacks.onPlay).toHaveBeenCalledWith(musicFile);
 fireEvent.click(within(panel).getByRole('button',{name:'在资源管理器中显示 CDs / Video 4.mkv'}));expect(callbacks.onReveal).toHaveBeenCalledWith(musicFile);
 for(const resource of [track,booklet]) {
  fireEvent.doubleClick(within(panel).getByText(resource.fileName).closest('button')!);expect(callbacks.onOpenResource).toHaveBeenCalledWith(resource);
  fireEvent.click(within(panel).getByRole('button',{name:`在资源管理器中显示 ${resource.fileName}`}));expect(callbacks.onRevealResource).toHaveBeenCalledWith(resource);
 }
 fireEvent.click(within(panel).getByText('OST Album',{selector:'strong'}).closest('button')!);expect(callbacks.onOpenChild).toHaveBeenCalledWith(folder);
 fireEvent.contextMenu(within(panel).getByText('OST Album',{selector:'strong'}).closest('article')!);expect(callbacks.onMenu.mock.calls[0][1]).toBe(folder);
 fireEvent.keyDown(screen.getByRole('tab',{name:'CDs 4'}),{key:'End'});
 expect(document.activeElement).toBe(screen.getByRole('tab',{name:'其他视频 1'}));
 expect(screen.queryByText('Theme.flac')).toBeNull();
 fireEvent.keyDown(document.activeElement!,{key:'Home'});
 expect(document.activeElement).toBe(screen.getByRole('tab',{name:'正片 1'}));
 fireEvent.keyDown(document.activeElement!,{key:'ArrowLeft'});
 expect(document.activeElement).toBe(screen.getByRole('tab',{name:'其他视频 1'}));
});

it('starts at CDs for resource-only works and falls back safely when the selected group disappears',()=>{
 const track={id:20,fileName:'Theme.flac',absolutePath:'X:/Synthetic/Theme.flac',extension:'.flac',resourceType:'AUDIO',fileSize:10} as ResourceFile;
 const callbacks={onPlay:vi.fn(),onReveal:vi.fn(),onOpenResource:vi.fn(),onRevealResource:vi.fn(),onOpenChild:vi.fn(),onMenu:vi.fn()};
 const emptyDetail={...detail,mediaFiles:[],nestedMediaFiles:[],videoSections:[]};
 const view=render(<I18nProvider><DetailVideoTabs detail={emptyDetail} watch={watch} cdsFiles={[track]} cdsFolders={[]} {...callbacks}/></I18nProvider>);
 const cds=screen.getByRole('tab',{name:'CDs 1'});
 expect(cds.getAttribute('aria-selected')).toBe('true');expect(cds.tabIndex).toBe(0);
 expect(view.container.querySelector('.media-file-list')).toBeNull();
 expect(view.container.querySelector('.video-tabs-heading > small')).toBeNull();
 cds.focus();expect(fireEvent.keyDown(cds,{key:'ArrowRight'})).toBe(true);expect(document.activeElement).toBe(cds);
 view.rerender(<I18nProvider><DetailVideoTabs detail={detail} watch={watch} cdsFiles={[]} cdsFolders={[]} {...callbacks}/></I18nProvider>);
 expect(screen.queryByRole('tab',{name:'CDs 1'})).toBeNull();
 expect(screen.getByRole('tab',{name:'正片 1'}).getAttribute('aria-selected')).toBe('true');
 expect(view.container.querySelector('.video-tabs-heading > small')).toBeTruthy();
 view.rerender(<I18nProvider><DetailVideoTabs detail={emptyDetail} watch={watch} cdsFiles={[]} cdsFolders={[]} {...callbacks}/></I18nProvider>);
 expect(screen.queryByRole('tablist')).toBeNull();
});
