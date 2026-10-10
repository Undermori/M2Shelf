// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { I18nProvider } from '../lib/i18n';
import { WindowTitlebar, type WindowMenuActions } from './WindowTitlebar';

vi.mock('../lib/api', () => ({ desktopAvailable: false }));
vi.mock('../lib/settingsStore', () => ({ useAppSettings: () => ({ settings: null, change: vi.fn() }) }));
afterEach(() => { cleanup(); document.body.innerHTML=''; });

function caption(canReload=true, extra:Partial<WindowMenuActions>={}) {
 const reload=vi.fn();
 const actions:WindowMenuActions={reload,canReload,addRoot:vi.fn(),revealRoot:vi.fn(),navigate:vi.fn(),about:vi.fn(),checkUpdate:vi.fn(),official:vi.fn(),setView:vi.fn(),canBrowse:true,canAdd:true,canReveal:true,canView:true,canCheck:true,viewMode:'grid'};
 render(<I18nProvider><WindowTitlebar actions={{...actions,...extra}}/></I18nProvider>);
 return reload;
}

it('labels reload F5 and keeps all contextual actions disabled outside a reader', () => {
 const reload=caption();fireEvent.click(screen.getByRole('menuitem',{name:'调试'}));
 const action=screen.getByRole('menuitem',{name:'重新加载 F5'});
 expect(screen.queryByRole('note')).toBeNull();
 for(const key of ['Alt + ←','PgUp','PgDn','F','B'])expect(screen.getByText(key).closest('button')?.disabled).toBe(true);
 expect(screen.queryByText(/F11/)).toBeNull();
 fireEvent.click(action);expect(reload).toHaveBeenCalledOnce();
});

it('invokes the exact reader and back callbacks, closes the menu and returns focus',()=>{
 const back=vi.fn(),previous=vi.fn(),next=vi.fn(),fullscreen=vi.fn(),bookmark=vi.fn();
 caption(true,{canBack:true,back,reader:{previous,next,fullscreen,bookmark,canPrevious:true,canNext:true,canBookmark:true}});
 for(const [key,callback] of [['Alt + ←',back],['PgUp',previous],['PgDn',next],['F',fullscreen],['B',bookmark]] as const) {
  fireEvent.click(screen.getByRole('menuitem',{name:'调试'}));fireEvent.click(screen.getByText(key).closest('button')!);
  expect(callback).toHaveBeenCalledOnce();expect(screen.queryByRole('menu')).toBeNull();expect(document.activeElement).toBe(screen.getByRole('menuitem',{name:'调试'}));
 }
});

it('never executes a disabled boundary action and dismisses through Escape or outside click',()=>{
 const previous=vi.fn();caption(true,{reader:{previous,next:vi.fn(),fullscreen:vi.fn(),bookmark:vi.fn(),canPrevious:false,canNext:true,canBookmark:false}});
 fireEvent.click(screen.getByRole('menuitem',{name:'调试'}));fireEvent.click(screen.getByText('PgUp').closest('button')!);expect(previous).not.toHaveBeenCalled();
 fireEvent.keyDown(screen.getByText('PgDn'),{key:'Escape'});expect(screen.queryByRole('menu')).toBeNull();
 fireEvent.click(screen.getByRole('menuitem',{name:'调试'}));fireEvent.pointerDown(document.body);expect(screen.queryByRole('menu')).toBeNull();
});

it('leaves native F5 and its modifier combinations untouched, as requested', () => {
 const reload=caption();
 for(const ctrlKey of [false,true])for(const shiftKey of [false,true]) {
  const event=new KeyboardEvent('keydown',{key:'F5',ctrlKey,shiftKey,cancelable:true});
  window.dispatchEvent(event);expect(event.defaultPrevented).toBe(false);
 }
 expect(reload).not.toHaveBeenCalled();
});

it('keeps menu disabled during guarded operations and preserves the existing Ctrl+R compatibility', () => {
 let reload=caption(false);fireEvent.click(screen.getByRole('menuitem',{name:'调试'}));
 expect(screen.getByRole('menuitem',{name:'重新加载 F5'}).hasAttribute('disabled')).toBe(true);
 fireEvent.keyDown(document.body,{key:'r',ctrlKey:true});expect(reload).not.toHaveBeenCalled();
 cleanup();reload=caption();
 fireEvent.keyDown(document.body,{key:'r',ctrlKey:true});expect(reload).toHaveBeenCalledOnce();
 const modal=document.createElement('div');modal.setAttribute('aria-modal','true');document.body.append(modal);
 fireEvent.keyDown(document.body,{key:'r',ctrlKey:true});expect(reload).toHaveBeenCalledOnce();
});
