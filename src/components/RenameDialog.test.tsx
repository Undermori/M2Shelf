// @vitest-environment jsdom
import {afterEach,describe,expect,it,vi} from 'vitest';
import {cleanup,fireEvent,render,screen} from '@testing-library/react';
import {RenameDialog} from './RenameDialog';
import {useEffect} from 'react';
import {I18nProvider,useI18n} from '../lib/i18n';
import {nodeDisplayTitle} from '../lib/format';
import type {MediaNode} from '../types/media';

function Locale({language}:{language:'zh-CN'|'en-US'|'ja-JP'|'ko-KR'}){const {setLanguage}=useI18n();useEffect(()=>setLanguage(language),[language,setLanguage]);return null;}
const series={id:7,displayName:'Source (2015)',folderName:'Source (2015)',nodeType:'CONTAINER',manualDisplayName:false,userTags:[],binding:{providerTitle:'Official Season 1',providerTitleCn:'官方第一季',providerTitleEn:'Official Season 1',providerTitleJa:'公式 第1期',providerTitleKo:'공식 시즌 1'}} as unknown as MediaNode;
afterEach(cleanup);
describe('display-name editing',()=>{
 it.each(['zh-CN','en-US','ja-JP','ko-KR'] as const)('prefills the same visible series title in %s and saves it as an explicit choice',language=>{
  const save=vi.fn();render(<I18nProvider><Locale language={language}/><RenameDialog node={series} busy={false} onClose={vi.fn()} onSave={save}/></I18nProvider>);
  expect((screen.getByRole('textbox') as HTMLInputElement).value).toBe(nodeDisplayTitle(series,language));
  fireEvent.submit(screen.getByRole('dialog'));expect(save).toHaveBeenCalledExactlyOnceWith(nodeDisplayTitle(series,language));
 });
 it('uses a saved custom name verbatim above a TMDb TV binding and never reattaches a series suffix',()=>{
  const node={...series,displayName:'My series [2015]',manualDisplayName:true,tmdbBinding:{active:true,movie:{title:'TMDb Show',mediaType:'TV'}}} as MediaNode;
  render(<I18nProvider><RenameDialog node={node} busy={false} onClose={vi.fn()} onSave={vi.fn()}/></I18nProvider>);
  expect((screen.getByRole('textbox') as HTMLInputElement).value).toBe('My series [2015]');
 });
 it('retains a draft and prevents duplicate saves or closing while the write is pending',()=>{
  const save=vi.fn(),close=vi.fn(),view=render(<I18nProvider><RenameDialog node={series} busy={false} onClose={close} onSave={save}/></I18nProvider>);
  fireEvent.change(screen.getByRole('textbox'),{target:{value:'Edited title'}});
  view.rerender(<I18nProvider><RenameDialog node={series} busy onClose={close} onSave={save}/></I18nProvider>);
  fireEvent.submit(screen.getByRole('dialog'));fireEvent.mouseDown(document.querySelector('.modal-backdrop')!);
  expect(save).not.toHaveBeenCalled();expect(close).not.toHaveBeenCalled();expect((screen.getByRole('textbox') as HTMLInputElement).disabled).toBe(true);
  expect((screen.getByRole('textbox') as HTMLInputElement).value).toBe('Edited title');
 });
});
