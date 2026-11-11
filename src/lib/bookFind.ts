export type SearchSection={text:string;blockIndex:number;rect?:[number,number,number,number]};
export type BookMatch={page:number;blockIndex:number;start:number;end:number;excerpt:string;rect?:[number,number,number,number];rects?:[number,number,number,number][];ranges?:Array<{blockIndex:number;start:number;end:number}>};
/** Literal matching: NFKC/full-width folding, case-insensitive, whitespace/newlines ignored;
 * punctuation is literal. Maps back to original UTF-16 offsets used by the existing reader. */
export function normalizedText(text:string){let value='';const starts:number[]=[],ends:number[]=[];let offset=0;
 for(const ch of text){const from=offset;offset+=ch.length;for(const part of ch.normalize('NFKC').toLowerCase()){
  if(/\s/u.test(part))continue;const next=part;
  value+=next;for(let i=0;i<next.length;i++){starts.push(from);ends.push(offset);}
 }}return{value,starts,ends};}
export function matchSections(sections:SearchSection[],query:string,page:number,remaining=500):BookMatch[]{
 const needle=normalizedText(query.trim()).value;if(!needle)return[];const matches:BookMatch[]=[];
 let text='';const spans=sections.map(section=>{const start=text.length;text+=section.text+'\n';return{...section,start,end:start+section.text.length};});
 const norm=normalizedText(text);let at=0;
 while(matches.length<remaining&&(at=norm.value.indexOf(needle,at))>=0){const from=norm.starts[at],to=norm.ends[at+needle.length-1];const selected=spans.filter(s=>s.start<to&&s.end>from);const first=selected[0];if(first){const ranges=selected.map(s=>({blockIndex:s.blockIndex,start:Math.max(0,from-s.start),end:Math.min(s.text.length,to-s.start)}));const rects=selected.flatMap(s=>s.rect?[s.rect]:[]).slice(0,32);matches.push({page,blockIndex:first.blockIndex,start:ranges[0].start,end:ranges[0].end,excerpt:text.slice(Math.max(0,from-30),Math.min(to+55,from+180)),rect:rects[0],rects,ranges});}at+=Math.max(needle.length,1);
 }return matches;
}
