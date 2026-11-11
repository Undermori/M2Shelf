const NON_POSTER_ASPECT_RATIO = 0.9;
// Geometry only: no image bytes, URLs, Canvas or second persistent cover cache.
const prepared=new Map<string,{width:number;height:number}>();
export const preparedPosterGeometry=(key:string)=>prepared.get(key);
export function recordPosterGeometry(key:string,width:number,height:number){
 if(width<=0||height<=0)return;prepared.delete(key);prepared.set(key,{width,height});while(prepared.size>128)prepared.delete(prepared.keys().next().value!);
}
export function shouldContainPosterArtwork(naturalWidth: number, naturalHeight: number): boolean {
  return naturalWidth > 0
    && naturalHeight > 0
    && naturalWidth / naturalHeight >= NON_POSTER_ASPECT_RATIO;
}

