import {bookReadingOrder} from '../lib/readingSequence';
import {useMemo} from 'react';
import {useReadingSequence} from '../hooks/useReadingSequence';
import {knownBookEnd,useNextVolume} from '../hooks/useNextVolume';
import {useI18n} from '../lib/i18n';
import {Icon} from './Icon';
import type {ComicBook} from '../types/comic';
export function ContinueReadingAction({books,onRead}:{books:ComicBook[];onRead:(book:ComicBook,list?:ComicBook[])=>void}){
 const latest=books.filter(book=>book.progress&&!book.indexError&&book.pageCount>0).sort((a,b)=>(b.progress?.lastReadAt??'').localeCompare(a.progress?.lastReadAt??''))[0]??null;
 const text=!!latest&&['EPUB','TXT','MOBI','AZW3'].includes(latest.documentFormat??'');
 const order=useMemo(()=>bookReadingOrder(books),[books]);
 const sequence=useReadingSequence(text?null:latest,order);
 const next=useNextVolume(text?latest:null);const {t}=useI18n();
 const first=order.find(book=>!book.indexError&&book.pageCount>0);
 if(!latest)return first?<span className="continue-reading-action"><button className="button primary" type="button" onClick={()=>onRead(first,order)}><Icon name="play"/>{t('comic.read')}</button></span>:null;
 const transition=text?next:sequence;
 const target=knownBookEnd(latest)?text?next.result?.next:sequence.next:null;
 return <span className="continue-reading-action"><button className="button primary" type="button" disabled={transition.busy} onClick={()=>{if(target)void transition.advance(async()=>true,book=>onRead(book,order));else onRead(latest,order);}}><Icon name="play"/>{target?t('nextVolume.open',{title:target.displayName}):t('nextVolume.continue')}</button>{transition.error&&<small role="alert">{transition.error}</small>}</span>;
}
