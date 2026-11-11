// @vitest-environment jsdom
import {afterEach,describe,expect,it,vi} from 'vitest';
import {cleanup,fireEvent,render,screen} from '@testing-library/react';
import {I18nProvider} from '../lib/i18n';
import {LibraryRecognitionModeDialog} from './LibraryRecognitionModeDialog';
afterEach(cleanup);
describe('new library recognition choice',()=>{
 it.each(['漫画','电子书','同人本','设定集'])('requires an explicit %s mode and preserves no-auto policy',name=>{
  const choose=vi.fn();render(<I18nProvider><LibraryRecognitionModeDialog path="X:/Synthetic" busy={false} onChoose={choose} onClose={vi.fn()}/></I18nProvider>);
  fireEvent.click(screen.getByRole('button',{name}));
  const create=screen.getByRole('button',{name:'创建资源库'}) as HTMLButtonElement;
  expect(screen.queryByRole('alert')).toBeNull();expect(choose).not.toHaveBeenCalled();
  fireEvent.click(create);expect(choose).not.toHaveBeenCalled();expect(screen.getByRole('alert').textContent).toBe('请选择识别方式');
  expect(document.querySelectorAll('.root-mode-options [aria-pressed="true"]')).toHaveLength(0);
  fireEvent.click(screen.getByRole('button',{name:/智能混合识别/}));expect(create.disabled).toBe(false);expect(screen.queryByRole('alert')).toBeNull();expect(choose).not.toHaveBeenCalled();
  fireEvent.click(create);expect(choose).toHaveBeenCalledWith('FOLDER',expect.any(String),false,'SMART_MIXED');
 });
 it('clears mode when the media kind changes, and explicitly saves the legacy file mode',()=>{
  const choose=vi.fn();render(<I18nProvider><LibraryRecognitionModeDialog path="X:/Synthetic" busy={false} onChoose={choose} onClose={vi.fn()}/></I18nProvider>);
  fireEvent.click(screen.getByRole('button',{name:'漫画'}));fireEvent.click(screen.getByRole('button',{name:/智能混合识别/}));
  fireEvent.click(screen.getByRole('button',{name:'电子书'}));expect(document.querySelectorAll('.root-mode-options [aria-pressed="true"]')).toHaveLength(0);fireEvent.click(screen.getByRole('button',{name:'创建资源库'}));expect(choose).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button',{name:/按单个文件识别/}));fireEvent.click(screen.getByRole('button',{name:'创建资源库'}));expect(choose).toHaveBeenLastCalledWith('VIDEO_FILE','EBOOK',false,'LEGACY');
 });
 it.each([['动画','ANIMATION'],['真人电影/剧','LIVE_ACTION']])('requires confirmation for %s while preserving the two native video modes', (label,kind)=>{
  const choose=vi.fn();render(<I18nProvider><LibraryRecognitionModeDialog path="X:/Synthetic" busy={false} onChoose={choose} onClose={vi.fn()}/></I18nProvider>);
  fireEvent.click(screen.getByRole('button',{name:label}));expect(document.querySelector('.smart-mode-row')).toBeNull();expect(screen.queryByRole('alert')).toBeNull();
  fireEvent.click(screen.getByRole('button',{name:'创建资源库'}));expect(choose).not.toHaveBeenCalled();expect(screen.getByRole('alert')).toBeTruthy();
  for(const [name,mode] of [[/按文件夹识别/,'FOLDER'],[/按视频文件识别/,'VIDEO_FILE']] as const){fireEvent.click(screen.getByRole('button',{name}));expect(screen.queryByRole('alert')).toBeNull();expect(choose).toHaveBeenCalledTimes(mode==='FOLDER'?0:1);fireEvent.click(screen.getByRole('button',{name:'创建资源库'}));expect(choose).toHaveBeenLastCalledWith(mode,kind,true);}
 });
});

it('shows the full literal path and preserves kind/mode while the path editor is suspended',()=>{
 const choose=vi.fn(),edit=vi.fn(),path='X:/Synthetic/中文 [路径]/本子';const props={path,busy:false,onChoose:choose,onClose:vi.fn(),onEditPath:edit};const view=render(<I18nProvider><LibraryRecognitionModeDialog {...props}/></I18nProvider>);
 expect(screen.getByText(path).getAttribute('title')).toBe(path);expect(screen.queryByRole('button',{name:'文件夹路径'})).toBeNull();fireEvent.click(screen.getByRole('button',{name:'同人本'}));fireEvent.click(screen.getByRole('button',{name:/智能混合识别/}));fireEvent.click(screen.getByRole('button',{name:'更改…'}));expect(edit).toHaveBeenCalledOnce();
 view.rerender(<I18nProvider><LibraryRecognitionModeDialog {...props} suspended/></I18nProvider>);expect(screen.queryByRole('dialog')).toBeNull();view.rerender(<I18nProvider><LibraryRecognitionModeDialog {...props} path="X:/Another"/></I18nProvider>);fireEvent.click(screen.getByRole('button',{name:'创建资源库'}));expect(choose).toHaveBeenCalledWith('FOLDER','DOUJIN',false,'SMART_MIXED');
});
