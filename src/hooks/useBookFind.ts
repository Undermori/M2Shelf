import {useEffect,useRef,useState} from 'react';
import {matchSections,type SearchSection,type BookMatch} from '../lib/bookFind';
export function useBookFind({identity,count,read,onSelect}:{identity:string;count:number;read:(page:number)=>Promise<SearchSection[]>;onSelect:(match:BookMatch)=>void}){
 const[open,setOpen]=useState(false),[query,setQuery]=useState(''),[matches,setMatches]=useState<BookMatch[]>([]),[index,setIndex]=useState(-1),[scanned,setScanned]=useState(0),[busy,setBusy]=useState(false),[hasText,setHasText]=useState(false),[error,setError]=useState(false),[limited,setLimited]=useState(false),[stopped,setStopped]=useState(false);
 const generation=useRef(0),selection=useRef(onSelect),readRef=useRef(read);selection.current=onSelect;readRef.current=read;
 useEffect(()=>{setOpen(false);setQuery('');setMatches([]);setIndex(-1);generation.current++;},[identity]);
 useEffect(()=>{const key=(e:KeyboardEvent)=>{if(e.isComposing)return;if(e.ctrlKey&&!e.altKey&&!e.metaKey&&e.key.toLowerCase()==='f'&&!document.querySelector('[aria-modal="true"]')){e.preventDefault();e.stopImmediatePropagation();setOpen(true);}else if(open&&e.key==='Escape'){e.preventDefault();e.stopImmediatePropagation();setOpen(false);}};window.addEventListener('keydown',key,true);return()=>window.removeEventListener('keydown',key,true);},[open]);
 useEffect(()=>{const token=++generation.current;setMatches([]);setIndex(-1);setScanned(0);setHasText(false);setError(false);setLimited(false);setStopped(false);setBusy(false);if(!open||!query.trim())return;
 const timer=setTimeout(()=>{setBusy(true);void(async()=>{const all:BookMatch[]=[];let chars=0;const started=performance.now();for(let page=0;page<count;page++){
  if(generation.current!==token)return;
  if(page>=2000||performance.now()-started>30000||chars>20*1024*1024){setStopped(true);break;}
  const sections=await readRef.current(page);if(generation.current!==token)return;
  for(const s of sections)chars+=s.text.length;if(sections.some(s=>s.text.trim()))setHasText(true);
  all.push(...matchSections(sections,query,page,500-all.length));setScanned(page+1);
  if(all.length>=500){setLimited(true);break;}
  if(page%4===0){setMatches([...all]);await new Promise<void>(resolve=>setTimeout(resolve,0));}
 }if(generation.current===token)setMatches(all);})().catch(()=>{if(generation.current===token)setError(true);}).finally(()=>{if(generation.current===token)setBusy(false);});},250);
 return()=>{clearTimeout(timer);generation.current++;};},[open,query,identity,count]);
 const choose=(i:number)=>{if(!matches.length)return;const next=(i+matches.length)%matches.length;setIndex(next);selection.current(matches[next]);};
 return{open,setOpen,query,setQuery,matches,index,scanned,busy,hasText,error,limited,stopped,choose};
}
