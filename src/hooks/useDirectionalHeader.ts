import {useEffect,type RefObject} from 'react';

/** Direction hysteresis on the real scrollport; transforms never change layout height. */
export function useDirectionalHeader(ref:RefObject<HTMLDivElement|null>,destination:string) {
 useEffect(()=>{
  const element=ref.current;if(!element)return;
  let previous=element.scrollTop,up=0,down=0;
  element.classList.remove('navigation-hidden');
  const scroll=()=>{
   const current=element.scrollTop;const delta=current-previous;previous=current;
   if(current<32||Math.abs(delta)>element.clientHeight/2){up=down=0;element.classList.remove('navigation-hidden');return;}
   if(delta<0){up-=delta;down=0;if(up>=8)element.classList.remove('navigation-hidden');}
   if(delta>0){down+=delta;up=0;if(down>=24)element.classList.add('navigation-hidden');}
  };
  element.addEventListener('scroll',scroll,{passive:true});return()=>{element.removeEventListener('scroll',scroll);element.classList.remove('navigation-hidden');};
 },[ref,destination]);
}
