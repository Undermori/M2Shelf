// @vitest-environment jsdom
import {fireEvent,render,screen} from '@testing-library/react';
import {afterEach,describe,expect,it,vi} from 'vitest';
import {cleanup} from '@testing-library/react';
afterEach(cleanup);
import {I18nProvider} from '../lib/i18n';
import {ReaderFooter} from './ReaderShell';

describe('shared reader navigation',()=>{
 it('keeps jump controls behind progress and restores keyboard focus on Escape',()=>{
  const jump=vi.fn();
  render(<I18nProvider><ReaderFooter status="第 2 / 10 页" page={1} count={10} canPrevious canNext onPrevious={()=>{}} onNext={()=>{}} onJump={jump} bookmarked={false} bookmarkDisabled={false} onBookmark={()=>{}} bookmarks={[4]}/></I18nProvider>);
  expect(screen.queryByRole('spinbutton')).toBeNull();
  const progress=screen.getByRole('button',{name:'第 2 / 10 页'});
  fireEvent.click(progress);
  const input=screen.getByRole('spinbutton');expect(document.activeElement).toBe(input);
  fireEvent.change(input,{target:{value:'9'}});expect(jump).toHaveBeenLastCalledWith(8);
  fireEvent.change(input,{target:{value:'99'}});expect(jump).toHaveBeenCalledTimes(1);
  fireEvent.keyDown(window,{key:'Escape'});expect(screen.queryByRole('spinbutton')).toBeNull();expect(document.activeElement).toBe(progress);
 });
 it('closes the location popup when opening the contents panel',()=>{
  const toc=vi.fn();
  render(<I18nProvider><ReaderFooter status="Chapter 1 / 3" page={0} count={3} chapters canPrevious={false} canNext onPrevious={()=>{}} onNext={()=>{}} onJump={()=>{}} onToc={toc} bookmarked={false} bookmarkDisabled={false} onBookmark={()=>{}} bookmarks={[]}/></I18nProvider>);
  fireEvent.click(screen.getByRole('button',{name:'Chapter 1 / 3'}));
  fireEvent.click(screen.getByRole('button',{name:'目录'}));expect(toc).toHaveBeenCalledOnce();expect(screen.queryByRole('spinbutton')).toBeNull();
 });
});
