import type {TextPosition} from '../types/comic';

/** A content block and UTF-16 offset, independent of columns, fonts and screen dimensions. */
export function captureTextPosition(frame:HTMLElement, viewport:HTMLElement, paged:boolean):TextPosition|null {
  const bounds=(paged?frame:viewport).getBoundingClientRect();
  if(bounds.width<1||bounds.height<1)return null;
  const doc=frame.ownerDocument as Document & {caretRangeFromPoint?:(x:number,y:number)=>Range|null};
  for(const dy of [12,28,44,64])for(const dx of [12,32,64]) {
    const range=doc.caretRangeFromPoint?.(bounds.left+dx,bounds.top+dy);
    const node=range?.startContainer;
    const block=(node?.nodeType===Node.ELEMENT_NODE?node as Element:node?.parentElement)?.closest<HTMLElement>('[data-book-block]');
    if(!range||!node||!block||!frame.contains(block)||block.dataset.bookAnchor)continue;
    const before=doc.createRange();before.selectNodeContents(block);
    try{before.setEnd(node,range.startOffset);}catch{continue;}
    return {blockIndex:Number(block.dataset.bookBlock),characterOffset:before.toString().length};
  }
  const block=Array.from(frame.querySelectorAll<HTMLElement>('[data-book-block]')).find(el=>!el.dataset.bookAnchor&&Array.from(el.getClientRects()).some(r=>r.bottom>bounds.top&&r.top<bounds.bottom&&r.right>bounds.left&&r.left<bounds.right));
  return block?{blockIndex:Number(block.dataset.bookBlock),characterOffset:0}:null;
}

export function positionRect(frame:HTMLElement,position:TextPosition):DOMRect|null {
  const block=frame.querySelector<HTMLElement>(`[data-book-block="${Math.max(0,Math.floor(position.blockIndex))}"]`);
  if(!block)return null;
  const walker=frame.ownerDocument.createTreeWalker(block,NodeFilter.SHOW_TEXT);
  let remaining=Math.max(0,position.characterOffset),node:Node|null=walker.nextNode(),last:Node|null=null;
  while(node){last=node;const length=node.textContent?.length??0;if(remaining<=length){const range=frame.ownerDocument.createRange();range.setStart(node,remaining);range.setEnd(node,Math.min(length,remaining+1));return range.getBoundingClientRect?.()??block.getBoundingClientRect();}remaining-=length;node=walker.nextNode();}
  if(last){const range=frame.ownerDocument.createRange();range.selectNodeContents(last);range.collapse(false);return range.getBoundingClientRect?.()??block.getBoundingClientRect();}
  return block.getBoundingClientRect();
}

export function friendlyChapterTitle(value:string,fallback:string):string {
  const clean=value.trim();
  return !clean||/\.(?:x?html?|xml)(?:#.*)?$/i.test(clean)||/[\\/]/.test(clean)?fallback:clean;
}
