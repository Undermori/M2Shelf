// @vitest-environment jsdom
import {afterEach,expect,it,vi} from 'vitest';
import {cleanup,render} from '@testing-library/react';
import {createRef} from 'react';
import {ScrollCommitBoundary} from './ScrollCommitBoundary';

afterEach(cleanup);
it('resets a ready new destination before the old DOM is replaced, only once',()=>{
 const scrollRef=createRef<HTMLDivElement>(),pending={current:null as number|null};
 const content=(text:string,ready=true)=><ScrollCommitBoundary scrollRef={scrollRef} pending={pending} ready={ready}><div ref={scrollRef}>{text}</div></ScrollCommitBoundary>;
 const view=render(content('old destination'));
 const mutations:string[]=[];
 Object.defineProperty(scrollRef.current!,'scrollTop',{configurable:true,set:()=>{mutations.push(scrollRef.current!.textContent!);}});
 pending.current=0;view.rerender(content('new destination',false));
 expect(mutations).toEqual([]);expect(pending.current).toBe(0);
 view.rerender(content('ready destination'));
 expect(mutations).toEqual(['new destination']);expect(pending.current).toBeNull();
 view.rerender(content('metadata update'));expect(mutations).toHaveLength(1);
});
it('leaves nonzero Back/history restores for the destination layout effect',()=>{
 const scrollRef=createRef<HTMLDivElement>(),pending={current:null as number|null};
 const content=(text:string)=><ScrollCommitBoundary scrollRef={scrollRef} pending={pending} ready><div ref={scrollRef}>{text}</div></ScrollCommitBoundary>;
 const view=render(content('old')),set=vi.fn();
 Object.defineProperty(scrollRef.current!,'scrollTop',{configurable:true,set});
 pending.current=1200;view.rerender(content('restored'));
 expect(set).not.toHaveBeenCalled();expect(pending.current).toBe(1200);
});
