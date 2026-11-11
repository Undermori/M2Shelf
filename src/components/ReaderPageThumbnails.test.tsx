// @vitest-environment jsdom
import {afterEach,beforeEach,describe,expect,it,vi} from 'vitest';
import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {ReaderPageThumbnails,type PageThumbnailLabels} from './ReaderPageThumbnails';
const labels:PageThumbnailLabels={title:'页面总览',close:'关闭页面总览',page:n=>`第 ${n} 页`,loading:'加载中',failed:'预览不可用',retry:'重试',current:'当前页'};
beforeEach(()=>{Object.defineProperty(URL,'createObjectURL',{configurable:true,value:vi.fn(()=>`blob:thumbnail-${Math.random()}`)});Object.defineProperty(URL,'revokeObjectURL',{configurable:true,value:vi.fn()});});
afterEach(cleanup);
describe('virtualized shared page overview',()=>{
 it('opens at physical page 80, renders only a window of a 1000-page book and uses the original jump callback',async()=>{
  const source=vi.fn(async()=>({blob:new Blob([new Uint8Array(20)]),width:100,height:150})),jump=vi.fn(),close=vi.fn();const view=render(<ReaderPageThumbnails identity="book:r1" count={1000} currentPage={79} currentRange={[79,80]} source={source} onJump={jump} onClose={close} labels={labels}/>);
  await waitFor(()=>expect(source).toHaveBeenCalled());expect(document.querySelectorAll('[data-thumbnail-index]').length).toBeLessThanOrEqual(24);expect(source.mock.calls.length).toBeLessThanOrEqual(24);
  expect(screen.getByRole('button',{name:'第 80 页 · 当前页'}).getAttribute('aria-current')).toBe('page');expect(document.querySelectorAll('.is-current')).toHaveLength(2);
  fireEvent.click(screen.getByRole('button',{name:'第 81 页 · 当前页'}));expect(jump).toHaveBeenCalledWith(80);
  fireEvent.keyDown(screen.getByRole('complementary'),{key:'Escape'});expect(close).toHaveBeenCalledOnce();view.unmount();expect(URL.revokeObjectURL).toHaveBeenCalled();
 });
 it('discards offscreen requests, scrolls to page 200, and exposes bounded diagnostics without development copy',async()=>{
  const source=vi.fn(async()=>({blob:new Blob(['tiny']),width:100,height:150}));render(<ReaderPageThumbnails identity="pdf:r1" count={500} currentPage={0} source={source} onJump={vi.fn()} onClose={vi.fn()} labels={labels}/>);
  const scroller=document.querySelector('.reader-page-thumbnails-scroll')!;fireEvent.scroll(scroller,{target:{scrollTop:Math.floor(199/2)*210}});
  await screen.findByRole('button',{name:'第 200 页'});await waitFor(()=>expect(Number(document.querySelector('[data-thumbnail-entries]')?.getAttribute('data-thumbnail-entries'))).toBeLessThanOrEqual(32));
  expect(Number(document.querySelector('[data-thumbnail-peak-active]')?.getAttribute('data-thumbnail-peak-active'))).toBe(1);expect(Number(document.querySelector('[data-thumbnail-bytes]')?.getAttribute('data-thumbnail-bytes'))).toBeLessThanOrEqual(8*1024*1024);
  expect(screen.queryByText(/MiB|queued|requests/)).toBeNull();expect(document.querySelectorAll('[data-thumbnail-index]').length).toBeLessThanOrEqual(24);
 });
 it('keeps a failed page navigable and retryable without breaking the surrounding reader',async()=>{
  let fail=true;const source=vi.fn(async()=>{if(fail)throw Error('invalid compressed image');return{blob:new Blob(['ok']),width:100,height:150};}),jump=vi.fn();render(<ReaderPageThumbnails identity="bad:r1" count={30} currentPage={0} source={source} onJump={jump} onClose={vi.fn()} labels={labels}/>);
  const retry=await screen.findByRole('button',{name:'重试 · 第 1 页'});fireEvent.click(screen.getByRole('button',{name:'第 1 页 · 当前页'}));expect(jump).toHaveBeenCalledWith(0);fail=false;fireEvent.click(retry);await waitFor(()=>expect(screen.queryByRole('button',{name:'重试 · 第 1 页'})).toBeNull());
 });
});
