// @vitest-environment jsdom
import {afterEach, describe, expect, it, vi} from 'vitest';
import {cleanup, fireEvent, render, screen, within} from '@testing-library/react';
import {useState} from 'react';
import {Select} from '../components/Select';
import {BatchContextMenu} from '../components/BatchContextMenu';
import {CollectionFilters} from '../components/CollectionFilters';
import {CollectionStateProvider, useCollection} from './collectionState';
import {I18nProvider} from './i18n';
import {floatingPanelPosition} from './floatingPanel';
import {isUnboundWork, round8UiCopy} from './round8UiCopy';
import type {MediaNode} from '../types/media';
afterEach(()=>{cleanup();vi.restoreAllMocks();vi.unstubAllGlobals();});

describe('Round 8 viewport and keyboard boundaries',()=>{
  it('uses measured wrapped popup height at the bottom/right and clamps after resize',()=>{
    const anchor={left:820,right:898,top:610,bottom:640};
    const position=floatingPanelPosition(anchor,{width:280,height:286},{width:900,height:640},'end');
    expect(position).toEqual({left:608,top:318,width:280,maxHeight:286});
    const small=floatingPanelPosition(anchor,{width:280,height:800},{width:320,height:240},'end');
    expect(small.left+small.width).toBeLessThanOrEqual(308);expect(small.top).toBeGreaterThanOrEqual(12);
  });
  it('does not consume boundary, modifier or IME keys and skips disabled options',()=>{
    render(<Select aria-label="Options" value="first"><option value="first">First</option><option disabled>Disabled</option><option value="last">Last</option></Select>);
    const control=screen.getByRole('combobox');fireEvent.click(control);
    expect(fireEvent.keyDown(control,{key:'ArrowUp'})).toBe(true);
    expect(fireEvent.keyDown(control,{key:'ArrowDown',isComposing:true})).toBe(true);
    expect(fireEvent.keyDown(control,{key:'ArrowDown',altKey:true})).toBe(true);
    expect(fireEvent.keyDown(control,{key:'ArrowDown'})).toBe(false);
    expect(control.getAttribute('aria-activedescendant')).toBe(screen.getByRole('option',{name:'Last'}).id);
    expect(fireEvent.keyDown(control,{key:'ArrowDown'})).toBe(true);
    fireEvent.keyDown(control,{key:'Escape'});
    expect(fireEvent.keyDown(control,{key:'Escape'})).toBe(true);
  });
  it('leaves an all-disabled Select closed',()=>{
    render(<Select aria-label="Empty"><option disabled>Unavailable</option></Select>);
    const control=screen.getByRole('combobox');expect(fireEvent.keyDown(control,{key:'ArrowDown'})).toBe(true);
    fireEvent.click(control);expect(screen.queryByRole('listbox')).toBeNull();
  });
  it('batch actions inherit focus, arrow navigation and invoker restoration',()=>{
    function Harness(){const [open,setOpen]=useState(false);return <><button onClick={()=>setOpen(true)}>Open</button>{open&&<BatchContextMenu x={890} y={630} count={4} onAction={()=>{}} onClose={()=>setOpen(false)}/>}</>;}
    render(<I18nProvider><Harness/></I18nProvider>);const origin=screen.getByText('Open');origin.focus();fireEvent.click(origin);
    const group=screen.getByRole('group'),buttons=within(group).getAllByRole('button');expect(document.activeElement).toBe(buttons[0]);
    fireEvent.keyDown(buttons[0],{key:'ArrowDown'});expect(document.activeElement).toBe(buttons[1]);
    fireEvent.keyDown(buttons[1],{key:'Escape'});expect(screen.queryByRole('group')).toBeNull();expect(document.activeElement).toBe(origin);
  });
  it('retains the independent metadata filter in the provider and clears it explicitly',()=>{
    function Harness(){const context=useCollection();return <><CollectionFilters nodes={[]} value={null} onChange={()=>{}} scope="all" metadata/><output>{String(context?.unbound.all??false)}</output></>;}
    render(<I18nProvider><CollectionStateProvider><Harness/></CollectionStateProvider></I18nProvider>);
    fireEvent.click(screen.getByRole('button',{name:'筛选'}));fireEvent.click(screen.getByRole('combobox',{name:'元数据'}));fireEvent.click(screen.getByRole('option',{name:'未绑定元数据的作品'}));
    expect(screen.getByText('true')).toBeTruthy();fireEvent.click(screen.getByRole('button',{name:'清除筛选'}));expect(screen.getByText('false')).toBeTruthy();
  });
});

describe('Round 8 metadata scope and four-language copy',()=>{
  it('does not misclassify bound TMDb works or series as missing metadata',()=>{
    const node={nodeType:'WORK',binding:null,tmdbBinding:null} as MediaNode;
    expect(isUnboundWork(node)).toBe(true);expect(isUnboundWork({...node,nodeType:'AUTO_WORK'})).toBe(true);
    expect(isUnboundWork({...node,tmdbBinding:{active:true} as MediaNode['tmdbBinding']})).toBe(false);
    expect(isUnboundWork({...node,binding:{} as MediaNode['binding']})).toBe(false);
    expect(isUnboundWork({...node,nodeType:'CONTAINER'})).toBe(false);expect(isUnboundWork(undefined)).toBe(false);
  });
  it('includes identical nonempty copy keys for every supported locale',()=>{
    expect(Object.keys(round8UiCopy).sort()).toEqual(['en-US','ja-JP','ko-KR','zh-CN']);
    for(const copy of Object.values(round8UiCopy)){expect(Object.keys(copy)).toEqual(Object.keys(round8UiCopy['zh-CN']));for(const value of Object.values(copy))expect(value.trim()).not.toBe('');}
  });
});
