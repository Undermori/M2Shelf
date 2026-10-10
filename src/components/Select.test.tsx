// @vitest-environment jsdom
import {afterEach, expect, it, vi} from 'vitest';
import {cleanup, fireEvent, render, screen} from '@testing-library/react';
import {Select} from './Select';
afterEach(() => {cleanup();vi.restoreAllMocks();vi.unstubAllGlobals();});
it('uses one accessible control and emits native values from keyboard and pointer choices', () => {
  const change = vi.fn();
  render(<label><Select aria-label="Type" value="comic" onChange={e => change(e.target.value)}><option value="all">All</option><option value="comic">Comics</option><option value="disabled" disabled>Unavailable</option><option value="text">Text</option></Select></label>);
  const control = screen.getByRole('combobox', {name: 'Type'});
  expect(screen.getAllByRole('combobox')).toHaveLength(1);
  fireEvent.click(control); fireEvent.keyDown(control, {key: 'ArrowDown'}); fireEvent.keyDown(control, {key: 'Enter'});
  expect(change).toHaveBeenLastCalledWith('text');
  expect(screen.queryByRole('listbox')).toBeNull();
  fireEvent.click(control); fireEvent.click(screen.getByRole('option', {name: 'All'}));
  expect(change).toHaveBeenLastCalledWith('all');expect(screen.queryByRole('listbox')).toBeNull();
  fireEvent.click(control); fireEvent.keyDown(control, {key: 'Escape'});
  expect(control.getAttribute('aria-expanded')).toBe('false');
});

it('anchors inward at the window edge and stays visible after resizing', () => {
  vi.stubGlobal('innerWidth',900);vi.stubGlobal('innerHeight',640);
  let rect={left:632,right:752,top:5,bottom:29,width:120,height:24} as DOMRect;
  vi.spyOn(HTMLElement.prototype,'getBoundingClientRect').mockImplementation(()=>rect);
  render(<Select aria-label="Language" popupAlign="end"><option>English</option><option>日本語</option><option>한국어</option><option>简体中文</option></Select>);
  fireEvent.click(screen.getByRole('combobox'));
  const popup=screen.getByRole('listbox');
  expect(popup.style.left).toBe('552px');expect(popup.style.top).toBe('35px');
  expect(Number.parseFloat(popup.style.left)+Number.parseFloat(popup.style.width)).toBeLessThanOrEqual(rect.right);
  vi.stubGlobal('innerWidth',320);vi.stubGlobal('innerHeight',240);rect={left:220,right:308,top:200,bottom:224,width:88,height:24} as DOMRect;
  fireEvent(window,new Event('resize'));
  expect(popup.style.left).toBe('108px');expect(Number.parseFloat(popup.style.top)).toBeGreaterThanOrEqual(12);
  expect(Number.parseFloat(popup.style.top)+174).toBeLessThanOrEqual(240-12);
  fireEvent.keyDown(screen.getByRole('combobox'),{key:'Tab'});expect(screen.queryByRole('listbox')).toBeNull();
});
