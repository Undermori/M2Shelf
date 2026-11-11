import {DetailUtility} from '../components/DetailUtility';
import {ContinueReadingAction} from '../components/ContinueReadingAction';
import {useBookDetailMetadata} from '../components/DetailMetadata';
import {MetadataBindingPanel} from '../components/MetadataBindingPanel';
import {useEffect, useState} from 'react';
import type {ComicBook} from '../types/comic';
import type {WorkDetailPageProps} from './WorkDetailPage';
import {api} from '../lib/api';
import {useI18n} from '../lib/i18n';
import {canBindBangumi, nodeTypeLabel, nodeDisplayTitle, bindingDisplayTitle, errorMessage} from '../lib/format';
import {useCoverDataUrl} from '../hooks/useCoverDataUrl';
import {PosterImage} from '../components/PosterImage';
import {Icon} from '../components/Icon';
import {Breadcrumb} from '../components/Breadcrumb';
import {ComicBookList} from '../components/ComicBookList';
import {PosterGrid} from '../components/PosterGrid';
import {OtherResourceList} from '../components/OtherResourceList';
import {LoadingState} from '../components/LoadingState';
import {readingFiles} from '../lib/readingFiles';
import {BookPosterCover} from '../components/BookPosterCard';
import {DetailCount} from '../components/DetailCount';
import type {LogicalGroup} from '../types/catalogue';
import type {LibraryRoot} from '../types/media';

/** Unanchored series use the standard book hero and reading table, without a fabricated Node. */
export function LogicalBookDetail({group,root,revision,onBack,onRoot,onRead,onMenu,onBookMenu}:{group:LogicalGroup;root:LibraryRoot;revision:number;onBack:()=>void;onRoot:()=>void;onRead:(book:ComicBook,list?:ComicBook[])=>void;onMenu:(event:React.MouseEvent)=>void;onBookMenu?:(event:React.MouseEvent,book:ComicBook)=>void}) {
 const {t}=useI18n();
 const metadata=useBookDetailMetadata(group.books,group.coverNode);
 const groupCover=useCoverDataUrl(metadata.target?.kind==='NODE'?group.coverNode:null,revision);
 const [groupImageFailed,setGroupImageFailed]=useState(false);
 useEffect(()=>setGroupImageFailed(false),[groupCover.coverUrl]);
 return <section className="detail-page comic-detail logical-book-detail">
  <header className="detail-toolbar"><button className="back-button" type="button" onClick={onBack}><Icon name="arrow-left"/>{t('detail.back')}</button><nav className="breadcrumb" aria-label={t('breadcrumb.location')}><button type="button" onClick={onRoot}>{root.displayName}</button><span className="breadcrumb-segment"><Icon name="chevron"/><span>{group.title}</span></span></nav></header>
  <div className="detail-content"><section className="detail-hero"><div className="detail-cover" ref={groupCover.coverFrameRef}>{groupCover.coverUrl&&!groupImageFailed?<PosterImage alt={group.title} cacheKey={groupCover.coverCacheKey} src={groupCover.coverUrl} onError={()=>{setGroupImageFailed(true);groupCover.onCoverError?.();}}/>:<BookPosterCover book={metadata.selectedBook??group.books[0]} revision={revision} detail title={group.title}/>}</div><div className="detail-copy"><p className="eyebrow">{t('smart.series')}</p><h1>{group.title}</h1><div className="detail-stats"><DetailCount count={group.books.length} message="comic.readableCount"/></div><div className="detail-actions"><ContinueReadingAction books={group.books} onRead={onRead}/>{metadata.actions}<button className="button ghost detail-more" type="button" onClick={onMenu} aria-label={t('detail.organize')} title={t('detail.organize')}><Icon name="more"/></button></div></div></section>
  <DetailUtility note={metadata.note}/>
  <section className="content-section comic-reading-section"><div className="section-heading"><div><p className="eyebrow">{t('book.readableFiles')}</p><h2>{t('comic.doubleClickRead')}</h2></div><span>{t('comic.books',{count:group.books.length})}</span></div><ComicBookList books={group.books} onRead={onRead} onBookMenu={onBookMenu}/></section></div>
 </section>;
}

export function ComicDetailPage(props: WorkDetailPageProps) {
  const {detail, coverRevision, onBack, onRoot, onBreadcrumb, rootLabel, onBangumi,
    onRetryCover, onClearBangumi, onOpenBangumi, onMenu, onReadComic,
    onOpenChild, onOpenResource, onRevealResource} = props;
  const {t} = useI18n();
  const node = detail?.node ?? null;
  const snapshotBooks = detail?.comicBooks;
  const [legacyResult, setLegacyResult] = useState<{id: number; updatedAt: string; books: ComicBook[] | null; error: string} | null>(null);
  const [imageFailed, setImageFailed] = useState(false);
  const {coverFrameRef, coverCacheKey, coverUrl, coverLoading, coverFailed,onCoverError} = useCoverDataUrl(node, coverRevision);
  // Native details include books and folders from one SQLite snapshot. This fallback
  // supports older in-session fixtures without showing a previous Node's books/count.
  useEffect(() => {
    let active = true;
    if (node && snapshotBooks === undefined) {
      const {id, updatedAt} = node;
      void api.comicDetail(id).then(books => {
        if (active) setLegacyResult({id, updatedAt, books, error: ''});
      }).catch(error => {
        if (active) setLegacyResult({id, updatedAt, books: null, error: errorMessage(error)});
      });
    }
    return () => {active = false;};
  }, [node?.id, node?.updatedAt, snapshotBooks]);
  useEffect(() => setImageFailed(false), [coverUrl]);
  const currentResult = node && legacyResult?.id === node.id && legacyResult.updatedAt === node.updatedAt ? legacyResult : null;
  const books = snapshotBooks ?? currentResult?.books ?? null;
  const metadata=useBookDetailMetadata(books??[],node);
  if (!detail || !node) return <LoadingState/>;
  const reading = readingFiles(books ?? [], detail.resourceFiles);
  const error = snapshotBooks === undefined ? currentResult?.error ?? '' : '';
  const title = nodeDisplayTitle(node);
  const failed = coverFailed || imageFailed || (!coverLoading && !!node.binding && !coverUrl);
  const normalize = (path: string) => path.replaceAll('\\', '/').replace(/\/$/, '').toLowerCase();
  const folders = snapshotBooks !== undefined ? detail.children : detail.children.filter(child => !books?.some(book => {
    const source = normalize(book.sourcePath ?? '');
    const directory = normalize(child.absolutePath);
    return source === directory || source.startsWith(directory + '/');
  }));
  const bookFolders = folders.filter(child => (child.totalComicBookCount ?? 0) > 0);
  const otherFolders = folders.filter(child => !(child.totalComicBookCount ?? 0));
  const resourceListProps = {onOpenFile: onOpenResource, onRevealFile: onRevealResource, onOpenFolder: onOpenChild, onFolderMenu: onMenu};
  return <section className="detail-page comic-detail" aria-busy={props.loading}>
    <header className="detail-toolbar"><button className="back-button" onClick={onBack} type="button"><Icon name="arrow-left"/>{t('detail.back')}</button><Breadcrumb rootLabel={rootLabel} items={detail.breadcrumbs} currentNodeId={node.id} onRoot={onRoot} onNode={onBreadcrumb}/></header>
    <div className="detail-content" inert={props.loading}>
      <section className="detail-hero">
        <div ref={coverFrameRef} className={`detail-cover ${coverUrl && !failed ? 'has-cover' : ''}`}>{coverUrl && !failed ? <PosterImage alt={t('detail.coverAlt', {title})} cacheKey={coverCacheKey} src={coverUrl} onError={() => {setImageFailed(true);onCoverError?.();}}/> : <><Icon name="work"/><span>{t(failed ? 'detail.coverFailed' : 'detail.noCover')}</span></>}</div>
        <div className="detail-copy">
          <p className="eyebrow">{nodeTypeLabel(node.nodeType)}{node.manualTypeOverride&&` · ${t('node.manual')}`}</p><h1>{title}</h1><p className="detail-folder-name" title={node.folderName}>{node.folderName}</p><p className="detail-path" title={node.absolutePath}>{node.absolutePath}</p>
          <div className="detail-stats">{books === null ? <span>{t('common.unknown')}</span> : <DetailCount count={reading.count} message="comic.readableCount"/>}</div>
          <div className="detail-actions"><ContinueReadingAction books={books??[]} onRead={(book,list)=>onReadComic?.(book,list)}/>{metadata.actions}<button className="button ghost detail-more" onClick={event => onMenu(event, node)} aria-label={t('detail.organize')} title={t('detail.organize')} type="button"><Icon name="more"/></button></div>
          {canBindBangumi(node)&&<MetadataBindingPanel provider="Bangumi" label={node.binding?t('detail.bound',{id:node.binding.providerSubjectId}):t('provider.bangumi')} title={node.binding?bindingDisplayTitle(node)??title:t('detail.unbound')} onOpen={node.binding?onOpenBangumi:undefined} openLabel={t('detail.openBangumi')} error={node.binding&&(failed||node.binding.coverDownloadError)?t('detail.bindingCoverFailed'):undefined}>
            <button className="button secondary" onClick={onBangumi} type="button">{t(node.binding?'detail.changeBinding':'detail.searchAdd')}</button>
            {node.binding&&(failed||node.binding.coverDownloadError)&&<button className="icon-button" aria-label={t('detail.retryCoverAria')} title={t('detail.retryCover')} onClick={()=>onRetryCover(imageFailed)} type="button"><Icon name="refresh"/></button>}
            {node.binding&&<button className="icon-button" aria-label={t('detail.clearBindingAria')} title={t('detail.clearBinding')} onClick={onClearBangumi} type="button"><Icon name="trash"/></button>}
          </MetadataBindingPanel>}
          {node.userTags.length > 0 && <div className="media-card-tags">{node.userTags.map(tag => <span className="user-tag-pill" key={tag.id}>{tag.name}</span>)}</div>}
        </div>
      </section>
      <DetailUtility note={metadata.note}/>
      {(books === null || reading.count > 0 || !!error) && <section className="content-section comic-reading-section">
        <div className="section-heading"><div><p className="eyebrow">{t(node.mediaKind === 'ARTBOOK' ? 'artbook.files' : node.mediaKind === 'EBOOK' ? 'ebook.files' : 'comic.files')}</p><h2>{t('comic.doubleClickRead')}</h2></div><span>{books === null ? '…' : t('comic.books', {count: reading.count})}</span></div>
        {error ? <p role="alert">{error}</p> : books === null ? <LoadingState label={t('comic.loading')}/> : <ComicBookList onBookMenu={props.onBookMenu} books={books} resources={reading.readable} onReadResource={onOpenResource} onRevealResource={onRevealResource} onRead={(book,list) => onReadComic?.(book,list)}/>}
      </section>}
      {books !== null && bookFolders.length > 0 && <section className="content-section comic-child-section">
        <div className="section-heading"><div><p className="eyebrow">{t('detail.children')}</p><h2>{t('browse.continue')}</h2></div><span>{t('detail.itemCount', {count: bookFolders.length})}</span></div>
        <PosterGrid nodes={bookFolders} viewMode="grid" onOpen={onOpenChild} onMenu={onMenu} onBangumi={props.onBangumiNode} onRetryCover={props.onRetryCoverNode} coverRevision={coverRevision}/>
      </section>}
      {books !== null && (reading.other.length > 0 || otherFolders.length > 0) && <section className="content-section comic-resource-section">
        <div className="section-heading"><div><p className="eyebrow">{t('detail.otherResources')}</p><h2>{t('detail.workResources')}</h2></div><span>{t('detail.itemCount', {count: reading.other.length + otherFolders.length})}</span></div>
        <OtherResourceList {...resourceListProps} files={reading.other} folders={otherFolders}/>
      </section>}
    </div>
  </section>;
}
