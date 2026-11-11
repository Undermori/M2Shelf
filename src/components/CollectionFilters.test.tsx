// @vitest-environment jsdom
import {afterEach,expect,it,vi} from 'vitest';
import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {I18nProvider} from '../lib/i18n';
import {CollectionStateProvider} from '../lib/collectionState';
import {CollectionFilters} from './CollectionFilters';
import type {MediaNode} from '../types/media';
vi.mock('../lib/api',()=>({desktopAvailable:true,api:{collectionStatuses:async()=>({entries:[],bookMemberships:[],nodeMemberships:[]})}}));
afterEach(cleanup);
it('groups tag/status controls behind one disclosure and keeps active conditions across closing',async()=>{
 const change=vi.fn();const node={userTags:[{id:1,name:'收藏标签'}]} as MediaNode;
 const view=render(<I18nProvider><CollectionStateProvider><CollectionFilters nodes={[node]} value={null} onChange={change} scope="all"/></CollectionStateProvider></I18nProvider>);
 expect(screen.queryByRole('combobox')).toBeNull();fireEvent.click(screen.getByRole('button',{name:'筛选'}));await waitFor(()=>expect(screen.getByRole('combobox',{name:'按观看 / 阅读状态筛选'}).hasAttribute('disabled')).toBe(false));
 fireEvent.keyDown(screen.getByRole('combobox',{name:'按观看 / 阅读状态筛选'}),{key:'Escape',isComposing:true});expect(screen.getByRole('group',{name:'筛选条件'})).toBeTruthy();
 fireEvent.click(screen.getByRole('combobox',{name:'按观看 / 阅读状态筛选'}));fireEvent.click(screen.getByRole('option',{name:'进行中'}));expect(screen.getByRole('button',{name:'筛选 1'})).toBeTruthy();
 fireEvent.keyDown(screen.getByRole('combobox',{name:'按观看 / 阅读状态筛选'}),{key:'Escape'});expect(screen.queryByRole('combobox')).toBeNull();fireEvent.click(screen.getByRole('button',{name:'筛选 1'}));expect(screen.getByRole('combobox',{name:'按观看 / 阅读状态筛选'}).textContent).toContain('进行中');
 fireEvent.click(screen.getByRole('combobox',{name:'按观看 / 阅读状态筛选'}));fireEvent.keyDown(screen.getByRole('combobox',{name:'按观看 / 阅读状态筛选'}),{key:'Escape'});expect(screen.getByRole('group',{name:'筛选条件'})).toBeTruthy();
 fireEvent.click(screen.getByRole('button',{name:'清除筛选'}));expect(change).toHaveBeenCalledWith(null);expect(screen.getByRole('button',{name:'筛选'})).toBeTruthy();view.unmount();
});
