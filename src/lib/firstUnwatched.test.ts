import {expect,it} from 'vitest';
import {firstUnwatched} from './firstUnwatched';
import type {MediaFile} from '../types/media';
import type {EpisodeWatchRow} from '../types/episodes';
it('locates the first known incomplete row in display order without inventing an unwatched state',()=>{
 const rows=[10,2,1,3].map(id=>({file:{id,fileName:`Episode ${id}`} as MediaFile,directory:''}));
 const state=(fileId:number,watched=false,partiallyWatched=false)=>({fileId,watched,partiallyWatched} as EpisodeWatchRow);
 expect(firstUnwatched(rows,[])).toBeUndefined();
 expect(firstUnwatched(rows,[state(1,true),state(10),state(2,true,true)])).toBe(2);
 expect(firstUnwatched(rows,[state(1,true),state(10),state(2,true)])).toBe(10);
 expect(rows.map(row=>row.file.id)).toEqual([10,2,1,3]);
});
