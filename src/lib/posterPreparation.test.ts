import {it,expect} from 'vitest';
import {recordPosterGeometry,preparedPosterGeometry,shouldContainPosterArtwork} from './poster';
it('bounds optional geometry metadata without controlling the image source or bytes',()=>{
 for(let i=0;i<140;i++)recordPosterGeometry(`geometry-${i}`,200+i,300);
 expect(preparedPosterGeometry('geometry-0')).toBeUndefined();expect(preparedPosterGeometry('geometry-139')).toEqual({width:339,height:300});
 expect(shouldContainPosterArtwork(1200,800)).toBe(true);expect(shouldContainPosterArtwork(600,900)).toBe(false);
});
