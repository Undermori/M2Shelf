// @vitest-environment jsdom
import {afterEach,expect,it,vi} from 'vitest';
import {cleanup,render,screen} from '@testing-library/react';
import {ContextMenu} from './ContextMenu';
import {I18nProvider} from '../lib/i18n';
import type {MediaNode} from '../types/media';
afterEach(cleanup);
it('identifies a multi-provider live-action series entry without labelling it Bangumi-only',()=>{
 const node={id:7,parentNodeId:1,mediaKind:'LIVE_ACTION',nodeType:'CONTAINER',displayName:'TV series',folderName:'TV series',totalVideoCount:63,userTags:[]} as unknown as MediaNode;
 const menu=(value:MediaNode)=><I18nProvider><ContextMenu node={value} x={20} y={20} onAction={vi.fn()} onClose={vi.fn()}/></I18nProvider>;
 const view=render(menu(node));expect(screen.getByRole('button',{name:'搜索并添加'})).toBeTruthy();expect(screen.queryByRole('button',{name:'从 Bangumi 搜索并添加'})).toBeNull();
 view.rerender(menu({...node,tmdbBinding:{active:true,movie:{id:22,mediaType:'TV',title:'Show'}}} as MediaNode));expect(screen.getByRole('button',{name:'修改绑定'})).toBeTruthy();
 view.rerender(menu({...node,mediaKind:'ANIMATION'}));expect(screen.getByRole('button',{name:'从 Bangumi 搜索并添加'})).toBeTruthy();
});
