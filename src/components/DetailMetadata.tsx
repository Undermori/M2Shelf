import {useState} from 'react';
import type {ComicBook} from '../types/comic';
import type {MediaNode} from '../types/media';
import type {CollectionTarget} from '../types/collection';
import {CollectionStatusControl} from '../lib/collectionState';
import {CoverButton} from './CoverSelector';
import {PrivateNote} from './PrivateNote';
import {Select} from './Select';
import {useI18n} from '../lib/i18n';

export const editableDetailNode=(node:MediaNode|null|undefined)=>!!node&&node.id>0&&node.parentNodeId!=null&&!node.workView&&['WORK','AUTO_WORK','CONTAINER'].includes(node.nodeType)&&((node.totalVideoCount??0)>0||(node.totalComicBookCount??0)>0);

/** One editor area. A virtual group explicitly selects a real source instead of inventing an entity. */
export function useBookDetailMetadata(books:ComicBook[],node?:MediaNode|null){
 const {t}=useI18n();const [selectedId,setSelectedId]=useState<number|null>(null);
 const single=books.length===1?books[0]:null;
 const nodeTarget=editableDetailNode(node)&&(!single||node?.nodeType==='CONTAINER')?node:null;
 const selected=books.find(book=>book.id===selectedId)??single??null;
 const target:CollectionTarget|null=nodeTarget?{kind:'NODE',id:nodeTarget.id}:single?{kind:'BOOK',id:single.id}:selected?{kind:'BOOK',id:selected.id}:null;
 const selectSource=!single&&!nodeTarget&&books.length>1;
 return {target,selectedBook:selected,
  actions:<>{selectSource&&<Select className="detail-book-source" aria-label={t('detail.bookSource')} value={selected?.id??''} onChange={event=>setSelectedId(Number(event.target.value)||null)}><option value="">{t('works.chooseSource')}</option>{books.map(book=><option key={book.id} value={book.id}>{book.displayName}</option>)}</Select>}{target&&<><CollectionStatusControl target={target} readable/><CoverButton target={target}/></>}</>,
  note:target&&<PrivateNote key={`${target.kind}:${target.id}`} target={target} detail/>,
 };
}
