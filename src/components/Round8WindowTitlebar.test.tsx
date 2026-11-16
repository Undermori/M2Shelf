// @vitest-environment jsdom
import {afterEach, expect, it, vi} from 'vitest';
import {cleanup, fireEvent, render, screen, within} from '@testing-library/react';
import {I18nProvider} from '../lib/i18n';
import {WindowTitlebar, type WindowMenuActions} from './WindowTitlebar';

vi.mock('../lib/api',()=>({desktopAvailable:false}));
vi.mock('../lib/settingsStore',()=>({useAppSettings:()=>({settings:null,change:vi.fn()})}));
afterEach(()=>{cleanup();vi.restoreAllMocks();});
function renderCaption(extra:Partial<WindowMenuActions>={}){
 const actions:WindowMenuActions={addRoot:vi.fn(),revealRoot:vi.fn(),navigate:vi.fn(),about:vi.fn(),checkUpdate:vi.fn(),official:vi.fn(),setView:vi.fn(),canBrowse:true,canAdd:true,canReveal:true,canView:true,canCheck:true,viewMode:'grid',...extra};
 render(<I18nProvider><WindowTitlebar actions={actions}/></I18nProvider>);
 return screen.getByRole('menuitem',{name:'文件'});
}
it('preserves arrow scrolling when the File popup has no enabled destinations',()=>{
 const trigger=renderCaption({canAdd:false,canReveal:false});trigger.focus();fireEvent.click(trigger);
 expect(within(screen.getByRole('menu')).getAllByRole('menuitem').every(item=>item.hasAttribute('disabled'))).toBe(true);
 for(const key of ['ArrowDown','ArrowUp','Home','End'])expect(fireEvent.keyDown(trigger,{key})).toBe(true);
 expect(document.activeElement).toBe(trigger);
});
it('moves to a valid destination, scrolls it into view and leaves the Home boundary native',()=>{
 const scroll=vi.fn();Object.defineProperty(HTMLElement.prototype,'scrollIntoView',{configurable:true,value:scroll});
 const trigger=renderCaption();fireEvent.click(trigger);const items=within(screen.getByRole('menu')).getAllByRole('menuitem');items[0].focus();
 expect(fireEvent.keyDown(items[0],{key:'ArrowDown'})).toBe(false);expect(document.activeElement).toBe(items[1]);expect(scroll).toHaveBeenCalledWith({block:'nearest',inline:'nearest'});
 expect(fireEvent.keyDown(items[1],{key:'Home'})).toBe(false);expect(document.activeElement).toBe(items[0]);expect(fireEvent.keyDown(items[0],{key:'Home'})).toBe(true);
});
it('ignores composing and modified arrows both before and after opening a popup',()=>{
 const trigger=renderCaption();trigger.focus();
 expect(fireEvent.keyDown(trigger,{key:'ArrowDown',isComposing:true})).toBe(true);expect(screen.queryByRole('menu')).toBeNull();
 expect(fireEvent.keyDown(trigger,{key:'ArrowDown',altKey:true})).toBe(true);expect(screen.queryByRole('menu')).toBeNull();
 fireEvent.click(trigger);const items=within(screen.getByRole('menu')).getAllByRole('menuitem');items[0].focus();
 expect(fireEvent.keyDown(items[0],{key:'ArrowDown',isComposing:true})).toBe(true);expect(document.activeElement).toBe(items[0]);
 expect(fireEvent.keyDown(items[0],{key:'ArrowRight',ctrlKey:true})).toBe(true);expect(screen.getByRole('menu').getAttribute('aria-label')).toBe('文件');
});
it('provides one top-level tab stop and moves closed menus with horizontal arrows',()=>{
 const trigger=renderCaption();trigger.focus();
 const roots=screen.getAllByRole('menuitem');expect(roots.filter(item=>item.tabIndex===0)).toEqual([trigger]);
 expect(fireEvent.keyDown(trigger,{key:'ArrowRight'})).toBe(false);expect(document.activeElement).toBe(roots[1]);expect(roots[1].tabIndex).toBe(0);expect(trigger.tabIndex).toBe(-1);expect(screen.queryByRole('menu')).toBeNull();
 expect(fireEvent.keyDown(roots[1],{key:'ArrowLeft'})).toBe(false);expect(document.activeElement).toBe(trigger);expect(roots.filter(item=>item.tabIndex===0)).toEqual([trigger]);
});
