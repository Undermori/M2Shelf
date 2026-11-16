// @vitest-environment jsdom
import {useEffect,useState} from 'react';
import {act,cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {afterEach,beforeEach,describe,expect,it,vi} from 'vitest';
import {I18nProvider} from './i18n';
import {notifyToast,subscribeToasts} from './toast';
import {canImportLibraryDrop} from './libraryDrop';
import {ToastStack,type ToastMessage} from '../components/Toast';
import {LibraryDropImport} from '../components/LibraryDropImport';
import {StudioNoteEditor} from '../components/PrivateNote';
import {HiddenNodesDialog} from '../components/HiddenNodesDialog';
import type {MediaNode} from '../types/media';

const mocks=vi.hoisted(()=>({validate:vi.fn(),listen:vi.fn(),hidden:vi.fn(),restore:vi.fn(),getNote:vi.fn(),setNote:vi.fn(),clipboard:vi.fn()}));
vi.mock('./api',()=>({desktopAvailable:true,api:{validateLibraryPath:mocks.validate,listHiddenNodes:mocks.hidden,resetNodeType:mocks.restore,getNote:mocks.getNote,setNote:mocks.setNote}}));
vi.mock('@tauri-apps/api/webviewWindow',()=>({getCurrentWebviewWindow:()=>({onDragDropEvent:mocks.listen})}));
const idle={initialized:true,showOnboarding:false,page:'all',detail:null,currentNode:null,readerBookId:null,rootAddBusy:false};
function Receipts(){const [items,setItems]=useState<ToastMessage[]>([]);useEffect(()=>subscribeToasts(notice=>setItems(current=>[...current,{...notice,id:(current.at(-1)?.id??0)+1}])),[]);return <ToastStack toasts={items} onDismiss={id=>setItems(current=>current.filter(item=>item.id!==id))}/>;}
beforeEach(()=>{vi.resetAllMocks();mocks.listen.mockResolvedValue(vi.fn());mocks.validate.mockResolvedValue('X:/Synthetic');mocks.hidden.mockResolvedValue([]);mocks.getNote.mockResolvedValue({body:'note',revision:0});mocks.setNote.mockResolvedValue({body:'note',revision:1});mocks.restore.mockResolvedValue(undefined);Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:mocks.clipboard}});mocks.clipboard.mockResolvedValue(undefined);});
afterEach(()=>{cleanup();vi.useRealTimers();vi.restoreAllMocks();});

describe('Round 11 global feedback',()=>{
 it('expires info/success at 3500ms and warning/error at 5000ms',async()=>{
  vi.useFakeTimers();render(<I18nProvider><Receipts/></I18nProvider>);
  act(()=>{for(const kind of ['info','success','warning','error'] as const)notifyToast(kind,kind);});
  await act(()=>vi.advanceTimersByTimeAsync(3499));expect(document.querySelectorAll('.toast')).toHaveLength(4);
  await act(()=>vi.advanceTimersByTimeAsync(1));expect(document.querySelectorAll('.toast')).toHaveLength(2);
  await act(()=>vi.advanceTimersByTimeAsync(1500));expect(document.querySelectorAll('.toast')).toHaveLength(0);
 });
 it('shows restore receipt outside the inert modal background, with reachable dismiss and no static row',async()=>{
  mocks.hidden.mockResolvedValue([{id:45,displayName:'Synthetic',folderName:'Synthetic',libraryRootId:1,userTags:[]} as unknown as MediaNode]);
  render(<I18nProvider><Receipts/><HiddenNodesDialog roots={[]} refreshKey={null} onClose={()=>{}} onRestored={async()=>{}}/></I18nProvider>);
  await waitFor(()=>expect(document.querySelector('.hidden-node-row button')).toBeTruthy());fireEvent.click(document.querySelector('.hidden-node-row button')!);
  await waitFor(()=>expect(document.querySelector('.toast-success')).toBeTruthy());
  const toast=document.querySelector('.toast-stack')!;expect(toast.closest('[inert],[aria-hidden=true]')).toBeNull();expect(document.querySelector('.hidden-nodes-notice')).toBeNull();
  const dismiss=screen.getByRole('button',{name:'关闭提示'});dismiss.focus();expect(document.activeElement).toBe(dismiss);fireEvent.click(dismiss);expect(document.querySelector('.toast')).toBeNull();
 });
 it('routes note export success and failure to the shared stack',async()=>{
  render(<I18nProvider><Receipts/><StudioNoteEditor target={{kind:'NODE',id:99111}} entry={{target:{kind:'NODE',id:99111},nodeId:99111,title:'Synthetic',libraryId:1,libraryName:'Synthetic',mediaKind:'ANIMATION',body:'note',revision:0,updatedAt:null}}/></I18nProvider>);
  await waitFor(()=>expect((screen.getByRole('textbox') as HTMLTextAreaElement).disabled).toBe(false));
  fireEvent.click(screen.getByRole('button',{name:'复制 Markdown'}));await waitFor(()=>expect(document.querySelector('.toast-success')).toBeTruthy());expect(document.querySelector('.studio-export-status')).toBeNull();
  mocks.clipboard.mockRejectedValueOnce(Error('denied'));fireEvent.click(screen.getByRole('button',{name:'复制 Markdown'}));await waitFor(()=>expect(document.querySelector('.toast-error')).toBeTruthy());
 });
});
describe('Round 11 native drag entry boundary',()=>{
 it.each(['all','organize','library','detail','settings','search','recent','favorites'])('validates a dropped path only on the all top-level page: %s',async page=>{
  const folders=vi.fn();render(<I18nProvider><LibraryDropImport enabled={canImportLibraryDrop({...idle,page})} onFolders={folders} onError={vi.fn()}/></I18nProvider>);
  await act(async()=>{mocks.listen.mock.calls[0][0]({payload:{type:'drop',paths:['X:/Synthetic']}});});
  expect(mocks.validate).toHaveBeenCalledTimes(page==='all'?1:0);expect(folders).toHaveBeenCalledTimes(page==='all'?1:0);
 });
 it.each([{initialized:false},{showOnboarding:true},{detail:{}},{currentNode:{}},{readerBookId:1},{rootAddBusy:true}])('disables the all page when its top-level preconditions fail: %j',patch=>{expect(canImportLibraryDrop({...idle,...patch})).toBe(false);});
 it('discards validation after leaving and returning, without reading the next queued path',async()=>{
  let finish!:(value:string)=>void;mocks.validate.mockReturnValue(new Promise<string>(resolve=>{finish=resolve;}));const folders=vi.fn(),error=vi.fn();
  const ui=(enabled:boolean)=><I18nProvider><LibraryDropImport enabled={enabled} onFolders={folders} onError={error}/></I18nProvider>;
  const view=render(ui(true));act(()=>mocks.listen.mock.calls[0][0]({payload:{type:'drop',paths:['X:/One','X:/Two']}}));
  view.rerender(ui(false));view.rerender(ui(true));await act(async()=>{finish('X:/One');});expect(mocks.validate).toHaveBeenCalledOnce();expect(folders).not.toHaveBeenCalled();expect(error).not.toHaveBeenCalled();
 });
});
