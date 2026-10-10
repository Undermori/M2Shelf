import {useState} from 'react';
import {api} from '../lib/api';
import {bookErrorKey} from '../lib/bookMessages';
import {errorMessage} from '../lib/format';
import type { ComicBook } from '../types/comic';
import type {ResourceFile} from '../types/media';
import { useI18n } from '../lib/i18n';
import { formatBytes, formatDate, naturalCompare } from '../lib/format';
import { Icon } from './Icon';
export function ComicBookList({books,onRead,resources=[],onReadResource,onRevealResource,onBookMenu}:{books:ComicBook[];onRead:(book:ComicBook)=>void;resources?:ResourceFile[];onReadResource?:(file:ResourceFile)=>void;onRevealResource?:(file:ResourceFile)=>void;onBookMenu?:(event:React.MouseEvent,book:ComicBook)=>void}){
 const {t}=useI18n();const [error,setError]=useState('');
 const reveal=async(book:ComicBook)=>{setError('');try{await api.revealComicBook(book.id);}catch(e){setError(errorMessage(e));}};
 const rows=[...books.map(book=>({key:`book-${book.id}`,name:book.displayName,book,resource:undefined as ResourceFile|undefined})),...resources.map(resource=>({key:`resource-${resource.id}`,name:resource.fileName,book:undefined as ComicBook|undefined,resource}))].sort((a,b)=>naturalCompare(a.name,b.name)||a.key.localeCompare(b.key));
 return <div className="media-file-list comic-book-list">{error&&<p className="book-list-error" role="alert">{error}</p>}<div className="file-list-heading"><span>#</span><span>{t('files.fileName')}</span><span>{t('files.info')}</span><span/></div>{rows.map(({key,book,resource},index)=>{
  if(!book&&resource){const read=()=>onReadResource?.(resource);return <div className="media-file-row comic-book-row" key={key} tabIndex={0} onDoubleClick={event=>{if(!(event.target instanceof Element&&event.target.closest('button')))read();}} onKeyDown={event=>{if(event.target===event.currentTarget&&(event.key==='Enter'||event.key===' ')){event.preventDefault();read();}}}>
   <span className="file-index">{String(index+1).padStart(2,'0')}</span><span className="file-name"><Icon name="file"/><span><strong>{resource.fileName}</strong><small title={resource.absolutePath}>{resource.absolutePath}</small></span></span>
   <span className="file-meta"><span>{formatBytes(resource.fileSize)}</span><span>{resource.extension.replace(/^\./,'').toUpperCase()}</span></span>
   <span className="file-actions"><button aria-label={t('comic.read')} title={t('comic.read')} onClick={read} type="button"><Icon name="play"/></button><button aria-label={t('files.reveal')} title={t('files.reveal')} onClick={()=>onRevealResource?.(resource)} type="button"><Icon name="external"/></button></span>
  </div>;}
  if(!book)return null;
  const unreadable=!!book.indexError||!book.pageCount;const format=book.documentFormat??(book.sourceKind==='IMAGE_FOLDER'?t('comic.imageFolder'):'CBZ');
  return <div className={`media-file-row comic-book-row${unreadable?' is-unreadable':''}`} key={key} tabIndex={unreadable?undefined:0} onContextMenu={event=>onBookMenu?.(event,book)} onDoubleClick={event=>{if(!(event.target instanceof Element&&event.target.closest('button'))&&!unreadable)onRead(book);}} onKeyDown={event=>{if(event.target===event.currentTarget&&(event.key==='Enter'||event.key===' ')&&!unreadable){event.preventDefault();onRead(book);}}}>
   <span className="file-index">{String(index+1).padStart(2,'0')}</span><span className="file-name"><Icon name="file"/><span><strong>{book.displayName}</strong><small title={book.sourcePath}>{book.sourcePath}</small>{book.progress&&<small>{t(['EPUB','MOBI','AZW3'].includes(book.documentFormat??'')?'ebook.chapterProgress':'comic.progress',{page:book.progress.lastPageIndex+1,count:book.pageCount})} · {formatDate(book.progress.lastReadAt)}</small>}{book.indexError&&<small role="status">{t(bookErrorKey(book.indexError))}</small>}</span></span>
   <span className="file-meta"><span>{t(['EPUB','MOBI','AZW3'].includes(book.documentFormat??'')?'ebook.chapters':'comic.pages',{count:book.pageCount})}</span>{book.sourceSize!==undefined&&book.sourceKind!=='IMAGE_FOLDER'&&<span>{formatBytes(book.sourceSize)}</span>}<span>{format}</span></span>
   <span className="file-actions"><button disabled={unreadable} aria-label={t(book.progress?'comic.continue':'comic.read')} title={t(book.progress?'comic.continue':'comic.read')} onClick={()=>onRead(book)} type="button"><Icon name="play"/></button><button aria-label={t('files.reveal')} title={t('files.reveal')} onClick={()=>void reveal(book)} type="button"><Icon name="external"/></button></span>
  </div>;
 })}</div>;
}
