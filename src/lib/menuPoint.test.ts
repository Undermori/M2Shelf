// @vitest-environment jsdom
import {expect,it,vi} from 'vitest';
import {menuPoint} from './menuPoint';
it('anchors keyboard activation while retaining literal pointer coordinates',()=>{
 const button=document.createElement('button');vi.spyOn(button,'getBoundingClientRect').mockReturnValue({left:800,bottom:600} as DOMRect);
 expect(menuPoint({clientX:0,clientY:0,currentTarget:button})).toEqual({x:800,y:600});
 expect(menuPoint({clientX:0,clientY:120,currentTarget:button})).toEqual({x:0,y:120});
});
