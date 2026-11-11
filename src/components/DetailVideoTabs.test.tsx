// @vitest-environment jsdom
import {afterEach,expect,it,vi} from 'vitest';
import {cleanup,fireEvent,render,screen} from '@testing-library/react';
import {I18nProvider} from '../lib/i18n';
import {DetailVideoTabs} from './DetailVideoTabs';
import type {NodeDetail,MediaFile} from '../types/media';
import type {EpisodeWatchController} from './EpisodeWatchControl';
afterEach(cleanup);
const file=(id:number)=>({id,nodeId:2,fileName:`Video ${id}.mkv`,absolutePath:`X:/Synthetic/Video ${id}.mkv`,extension:'mkv',fileSize:1}) as MediaFile;
const detail={node:{id:2},mediaFiles:[file(1),file(2)],nestedMediaFiles:[{file:file(3),relativeDirectory:'Other'}],videoSections:[{fileId:1,section:'MAIN'},{fileId:2,section:'SPS'},{fileId:3,section:'OTHER'}]} as NodeDetail;
const watch={data:null,busy:false,playAndMark:vi.fn()} as unknown as EpisodeWatchController;
it('keeps every indexed file in one tab and preserves real play/reveal identity',()=>{
 const play=vi.fn(),reveal=vi.fn();const view=render(<I18nProvider><DetailVideoTabs detail={detail} watch={watch} onPlay={play} onReveal={reveal}/></I18nProvider>);
 expect(screen.getByText('Video 1.mkv')).toBeTruthy();expect(screen.queryByText('Video 2.mkv')).toBeNull();fireEvent.click(screen.getByRole('tab',{name:'SPs 1'}));expect(screen.getByText('Video 2.mkv')).toBeTruthy();expect(screen.queryByText('Video 1.mkv')).toBeNull();
 fireEvent.click(screen.getByRole('button',{name:'点击播放 Video 2.mkv'}));expect(play).toHaveBeenCalledWith(file(2));
 fireEvent.keyDown(screen.getByRole('tab',{name:'SPs 1'}),{key:'ArrowRight'});expect(screen.getByText('Video 3.mkv')).toBeTruthy();
 view.rerender(<I18nProvider><DetailVideoTabs detail={{...detail,node:{...detail.node,id:5}}} watch={watch} onPlay={play} onReveal={reveal}/></I18nProvider>);expect(screen.getByRole('tab',{name:'正片 1'}).getAttribute('aria-selected')).toBe('true');
});
it('omits the repeated SPs directory line while retaining file paths and actions',()=>{
 const view=render(<I18nProvider><DetailVideoTabs detail={{...detail,mediaFiles:[file(1)],nestedMediaFiles:[{file:file(2),sourceNodeId:2,sourceName:'Work',relativeDirectory:'SPs'}]}} watch={watch} onPlay={vi.fn()} onReveal={vi.fn()}/></I18nProvider>);fireEvent.click(screen.getByRole('tab',{name:'SPs 1'}));expect(view.container.querySelector('.file-origin')).toBeNull();expect(view.container.querySelector('.media-file-row')?.getAttribute('title')).toContain(file(2).absolutePath);expect(screen.getByRole('button',{name:'点击播放 Video 2.mkv'})).toBeTruthy();
});

it('renders sparse section deviations identically to complete section rows',()=>{
 const sparse={...detail,videoSections:[{fileId:2,section:'SPS' as const}]};
 render(<I18nProvider><DetailVideoTabs detail={sparse} watch={watch} onPlay={vi.fn()} onReveal={vi.fn()}/></I18nProvider>);
 expect(screen.getByRole('tab',{name:'正片 1'})).toBeTruthy();
 expect(screen.getByText('Video 1.mkv')).toBeTruthy();
 fireEvent.click(screen.getByRole('tab',{name:'SPs 1'}));expect(screen.getByText('Video 2.mkv')).toBeTruthy();
 fireEvent.click(screen.getByRole('tab',{name:'其他视频 1'}));expect(screen.getByText('Video 3.mkv')).toBeTruthy();
});
