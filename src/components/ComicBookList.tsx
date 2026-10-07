import {useState} from 'react';
import {api} from '../lib/api';
import {errorMessage} from '../lib/format';
import type { ComicBook } from '../types/comic';
import { useI18n } from '../lib/i18n';
import { formatBytes, formatDate, naturalCompare } from '../lib/format';
import { Icon } from './Icon';
export function ComicBookList({books,onRead}:{books:ComicBook[];onRead:(book:ComicBook)=>void}){
 const {t}=useI18n();const [error,setError]=useState('');
 const reveal=async(book:ComicBook)=>{setError('');try{await api.revealComicBook(book.id);}catch(e){setError(errorMessage(e));}};
 return <div className="media-file-list comic-book-list">{error&&<p className="book-list-error" role="alert">{error}</p>}<div className="file-list-heading"><span>#</span><span>{t('files.fileName')}</span><span>{t('files.info')}</span><span/></div>{[...books].sort((a,b)=>naturalCompare(a.displayName,b.displayName)||a.id-b.id).map((book,index)=>{
  const unreadable=!!book.indexError||!book.pageCount;const format=book.documentFormat??(book.sourceKind==='IMAGE_FOLDER'?t('comic.imageFolder'):'CBZ');
  return <div className={`media-file-row comic-book-row${unreadable?' is-unreadable':''}`} key={book.id} tabIndex={unreadable?undefined:0} onDoubleClick={event=>{if(!(event.target instanceof Element&&event.target.closest('button'))&&!unreadable)onRead(book);}} onKeyDown={event=>{if(event.target===event.currentTarget&&(event.key==='Enter'||event.key===' ')&&!unreadable){event.preventDefault();onRead(book);}}}>
   <span className="file-index">{String(index+1).padStart(2,'0')}</span><span className="file-name"><Icon name="file"/><span><strong>{book.displayName}</strong><small title={book.sourcePath}>{book.sourcePath}</small>{book.progress&&<small>{t(book.documentFormat==='EPUB'?'ebook.chapterProgress':'comic.progress',{page:book.progress.lastPageIndex+1,count:book.pageCount})} · {formatDate(book.progress.lastReadAt)}</small>}{book.indexError&&<small role="status">{t(book.indexError.includes('ENCRYPTED')?'comic.encrypted':book.indexError.includes('LIMIT')?'comic.limit':'comic.error')}</small>}</span></span>
   <span className="file-meta"><span>{t(book.documentFormat==='EPUB'?'ebook.chapters':'comic.pages',{count:book.pageCount})}</span>{book.sourceSize!==undefined&&book.sourceKind!=='IMAGE_FOLDER'&&<span>{formatBytes(book.sourceSize)}</span>}<span>{format}</span></span>
   <span className="file-actions"><button disabled={unreadable} aria-label={t(book.progress?'comic.continue':'comic.read')} title={t(book.progress?'comic.continue':'comic.read')} onClick={()=>onRead(book)} type="button"><Icon name="play"/></button><button aria-label={t('files.reveal')} title={t('files.reveal')} onClick={()=>void reveal(book)} type="button"><Icon name="external"/></button></span>
  </div>;
 })}</div>;
}
