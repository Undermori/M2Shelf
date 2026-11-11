// @vitest-environment jsdom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {act,cleanup,fireEvent,render,renderHook,screen,waitFor} from '@testing-library/react';
import {I18nProvider} from './i18n';
import {matchSections,normalizedText} from './bookFind';
import {useBookFind} from '../hooks/useBookFind';
import {PrivateNote} from '../components/PrivateNote';
import {CommandPalette} from '../components/CommandPalette';
import {LibraryPathDialog} from '../components/LibraryDropImport';
import {next6Messages} from './next6Messages';
const mocks=vi.hoisted(()=>({get:vi.fn(),save:vi.fn(),search:vi.fn(),path:vi.fn(),choose:vi.fn()}));
vi.mock('./api',()=>({desktopAvailable:false,chooseDirectory:mocks.choose,api:{getNote:mocks.get,saveNote:mocks.save,search:mocks.search,validateLibraryPath:mocks.path}}));
beforeEach(()=>{vi.resetAllMocks();mocks.get.mockResolvedValue({body:'',revision:0});mocks.save.mockImplementation(async(_t,body,revision)=>({body,revision:revision+1}));mocks.search.mockResolvedValue([]);});
afterEach(()=>{cleanup();vi.useRealTimers();});
it('uses literal Unicode width/case matching with original UTF-16 positions',()=>{
 const found=matchSections([{text:'序😀 ＡＢＣ\n\t東京 <script>',blockIndex:4}],'abc 東京',7);
 expect(found).toHaveLength(1);expect(found[0]).toMatchObject({page:7,blockIndex:4,start:4,end:11});
 expect(matchSections([{text:'東',blockIndex:0},{text:'京',blockIndex:1}],'東京',0)).toHaveLength(1);
 expect(normalizedText('Ａ，Ｂ')).toMatchObject({value:'a,b'});expect(matchSections([{text:'a.b a+b',blockIndex:0}],'a.b',0)).toHaveLength(1);
 expect(matchSections([{text:'東京'.repeat(600),blockIndex:0}],'東京',0)).toHaveLength(500);
});
it('searches without navigating, cancels stale asynchronous pages, and navigates only on selection',async()=>{
 let release!:(v:Array<{text:string;blockIndex:number}>)=>void;
 const read=vi.fn().mockImplementationOnce(()=>new Promise(resolve=>release=resolve)).mockResolvedValue([{text:'new 東京',blockIndex:2}]);const select=vi.fn();
 const {result,unmount}=renderHook(()=>useBookFind({identity:'book-a',count:2,read,onSelect:select}));
 act(()=>{result.current.setOpen(true);result.current.setQuery('old');});await waitFor(()=>expect(read).toHaveBeenCalledOnce());
 act(()=>result.current.setQuery('東京'));act(()=>release([{text:'old old',blockIndex:0}]));
 await waitFor(()=>expect(result.current.matches).toHaveLength(2));expect(select).not.toHaveBeenCalled();act(()=>result.current.choose(1));expect(select).toHaveBeenCalledWith(expect.objectContaining({page:1,blockIndex:2}));unmount();
});
it('autosaves a draft on leaving its real book and retains a failed draft for retry',async()=>{
 const view=render(<I18nProvider><PrivateNote target={{kind:'BOOK',id:80001}}/></I18nProvider>);fireEvent.click(screen.getByText('笔记'));const input=await screen.findByRole('textbox');fireEvent.change(input,{target:{value:'line one\n<script>literal</script>'}});view.unmount();
 await waitFor(()=>expect(mocks.save).toHaveBeenCalledWith({kind:'BOOK',id:80001},'line one\n<script>literal</script>',0));
 mocks.save.mockRejectedValueOnce(Error('offline'));const other=render(<I18nProvider><PrivateNote target={{kind:'BOOK',id:80002}}/></I18nProvider>);fireEvent.click(screen.getByText('笔记'));fireEvent.change(await screen.findByRole('textbox'),{target:{value:'retain me'}});fireEvent.click(screen.getByRole('button',{name:'保存'}));await screen.findByRole('alert');expect((screen.getByRole('textbox') as HTMLTextAreaElement).value).toBe('retain me');fireEvent.click(screen.getByRole('button',{name:'重试'}));await waitFor(()=>expect(screen.queryByRole('alert')).toBeNull());other.unmount();
});
it('keeps Ctrl+K out of readers, inputs, IME and higher priority dialogs; uses existing callbacks',async()=>{
 const action=vi.fn(),hit=vi.fn();const view=render(<I18nProvider><CommandPalette enabled actions={[{id:'all',label:'全部资源',run:action}]} onHit={hit}/></I18nProvider>);
 fireEvent.keyDown(window,{key:'k',ctrlKey:true,isComposing:true});expect(screen.queryByRole('dialog')).toBeNull();
 fireEvent.keyDown(window,{key:'k',ctrlKey:true});await screen.findByRole('dialog');const input=screen.getByRole('combobox');fireEvent.keyDown(input,{key:'Enter'});expect(action).toHaveBeenCalledOnce();expect(screen.queryByRole('dialog')).toBeNull();
 view.rerender(<I18nProvider><CommandPalette enabled={false} actions={[]} onHit={hit}/></I18nProvider>);fireEvent.keyDown(window,{key:'k',ctrlKey:true});expect(screen.queryByRole('dialog')).toBeNull();
});
it('validates pasted paths natively and leaves the existing wizard to the parent',async()=>{
 const choose=vi.fn();mocks.path.mockRejectedValueOnce(Error('file')).mockResolvedValueOnce('X:/Synthetic/Books');render(<I18nProvider><LibraryPathDialog onChoose={choose} onClose={vi.fn()}/></I18nProvider>);fireEvent.change(screen.getByRole('textbox'),{target:{value:'X:/Synthetic/file.mkv'}});fireEvent.click(screen.getByRole('button',{name:'确认'}));await screen.findByRole('alert');expect(choose).not.toHaveBeenCalled();fireEvent.change(screen.getByRole('textbox'),{target:{value:'X:/Synthetic/Books'}});fireEvent.click(screen.getByRole('button',{name:'确认'}));await waitFor(()=>expect(choose).toHaveBeenCalledWith('X:/Synthetic/Books'));
});
it('has the same typed copy keys in all four locales',()=>{const keys=Object.keys(next6Messages['zh-CN']).sort();for(const locale of Object.keys(next6Messages) as Array<keyof typeof next6Messages>){expect(Object.keys(next6Messages[locale]).sort()).toEqual(keys);for(const text of Object.values(next6Messages[locale]))expect(text.trim()).not.toBe('');}});
