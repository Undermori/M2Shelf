"""Author synthetic snapshots and independently declared expectations; never invokes the recognizer.

The `verified` flags model evidence supplied by an existing decoder/index. These are NOT book decoder tests.
Run from anywhere; outputs only to docs/smart-mixed/fixtures in this repository.
"""
import json
from pathlib import Path

OUT = Path(__file__).resolve().parents[3] / 'docs' / 'smart-mixed' / 'fixtures'
OUT.mkdir(parents=True, exist_ok=True)

def directory(p, hint=None):
    return dict(path=p, kind='DIRECTORY', hint=hint)

def file(p, fmt='PDF', **kw):
    return dict(path=p, kind='FILE', format=fmt, verified=bool(fmt), **kw)

def snapshot(entries, kind='COMIC', **kw):
    return dict(root_id='fixture-root', media_kind=kind, complete=True, entries=entries, **kw)

def image(d, names=('001.png', '002.png'), hint=None):
    pages=[f'{d}/{n}' if d else n for n in names]
    entries=[directory(d, hint)]+[file(p,'PNG' if p.lower().endswith('.png') else 'JPEG') for p in pages]
    order=dict(directory=d, pages=[p for p in pages if Path(p).stem.lower() not in ('cover','folder','thumb','poster')], basis='EXISTING_INDEXED_NATURAL_ORDER')
    return entries, order

def unit(p, pages=None):
    return dict(path=p, kind='DIRECT_PAGES' if pages is not None else 'FILE_BOOK', pages=pages or [])

def member(p, volume=None, chapter=None, role=None, decision='APPLY'):
    return dict(path=p, volume=volume, chapter=chapter, role=role or ('CHAPTER' if chapter is not None else 'VOLUME' if volume is not None else 'MAIN'), decision=decision)

def series(d, members, decision='APPLY', title=None):
    return dict(directory=d, title=title or d, members=members, decision=decision)

def expected(units, roots, groups=(), roles=None, diagnostics=(), editions=0, retained=0):
    return dict(units=units, root_paths=roots, series=list(groups), directory_roles=roles or {}, diagnostics=list(diagnostics), editions=editions, retained_prior=retained)

cases=[]
def case(i, title, data, exp, negative):
    cases.append(dict(id=i,title=title,input=data,expected=exp,negative_assertion=negative))

def vols(d, formats=('PDF','PDF'), nums=(1,2)):
    paths=[f'{d}/第{v:02}卷.{fmt.lower()}' for v,fmt in zip(nums,formats)]
    return [directory(d)]+[file(p,f) for p,f in zip(paths,formats)], paths

e,p=vols('东京食尸鬼')
case(1,'Root single PDF',snapshot([file('独立.pdf')]),expected([unit('独立.pdf')],['独立.pdf']), 'Root must not become a Series')
p2=[f'独立作品{i:03}.pdf' for i in range(300)]
case(2,'Root 300 unrelated PDFs',snapshot([file(p) for p in p2]),expected([unit(p) for p in p2],p2),'No root-wide book merge')
e3,p3=vols('系列甲'); e4,p4=vols('系列乙'); singles=[f'独立{i:03}.pdf' for i in range(100)]
case(3,'Two series plus 100 independent PDFs',snapshot(e3+e4+[file(p) for p in singles]),expected([unit(p) for p in p3+p4+singles],['系列甲','系列乙']+singles,[series('系列甲',[member(p3[0],1),member(p3[1],2)]),series('系列乙',[member(p4[0],1),member(p4[1],2)])]),'104 sources; 102 parallel root cards; independent PDFs outside both series')
case(4,'Tokyo Ghoul explicit volumes',snapshot(e),expected([unit(x) for x in p],['东京食尸鬼'],[series('东京食尸鬼',[member(p[0],1),member(p[1],2)])],{'东京食尸鬼':'SERIES'}),'No fabricated source or volume')
e,p=vols('作品A',('PDF','CBZ'))
case(5,'PDF and CBZ in one series',snapshot(e),expected([unit(x) for x in p],['作品A'],[series('作品A',[member(p[0],1),member(p[1],2)])]),'Mixed formats retain separate source identities')
e,p=vols('作品A',('PDF','EPUB','TXT'),(1,2,3))
case(6,'PDF EPUB TXT mixed volumes',snapshot(e,'EBOOK'),expected([unit(x) for x in p],['作品A'],[series('作品A',[member(x,i+1) for i,x in enumerate(p)])]),'Three readable files, not one merged book')
e,p=vols('作品A',('PDF','CBZ')); im,o=image('作品A/第03卷')
case(7,'File and image volumes',snapshot(e+im,page_orders=[o]),expected([unit(x) for x in p]+[unit('作品A/第03卷',o['pages'])],['作品A'],[series('作品A',[member(p[0],1),member(p[1],2),member('作品A/第03卷',3)])]),'Child images never become parent pages')
p=[f'作品A/第04-06卷/{v:02}.pdf' for v in (4,5,6)]
case(8,'Bare numbered books through range container',snapshot([directory('作品A','WORK'),directory('作品A/第04-06卷')]+[file(x) for x in p]),expected([unit(x) for x in p],['作品A'],[series('作品A',[member(x,i+4,decision='REVIEW') for i,x in enumerate(p)],'REVIEW')],{'作品A/第04-06卷':'INTERMEDIATE_CONTAINER'}),'Range is not volume 6; no invented volume')
im1,o1=image('作品/第01卷/第001话');im2,o2=image('作品/第01卷/第002话')
case(9,'Volume chapter image hierarchy',snapshot([directory('作品')]+im1+im2,page_orders=[o1,o2]),expected([unit(o1['directory'],o1['pages']),unit(o2['directory'],o2['pages'])],['作品'],[series('作品',[member(o1['directory'],1,1),member(o2['directory'],1,2)])]),'Retain both volume and chapter; never concatenate chapter pages')
p=['藤本树/再见绘梨.pdf','藤本树/蓦然回首.pdf']
case(10,'Author contains distinct works',snapshot([directory('藤本树','AUTHOR')]+[file(x) for x in p]),expected([unit(x) for x in p],['藤本树'],roles={'藤本树':'CATEGORY'}),'No author-wide Series')
p=['某画师/本A.pdf','某画师/本B.pdf']
case(11,'Doujin artist category',snapshot([directory('某画师','ARTIST')]+[file(x) for x in p],'DOUJIN'),expected([unit(x) for x in p],['某画师'],roles={'某画师':'CATEGORY'}),'No Bangumi or artist-wide merge')
im,o=image('某作品/角色原画')
case(12,'Artbook and raw image assets',snapshot([directory('某作品','WORK'),file('某作品/设定集.pdf')]+im,'ARTBOOK',page_orders=[o]),expected([unit('某作品/设定集.pdf')],['某作品'],diagnostics=['IMAGE_ASSETS_ONLY']),'Raw art directory is physical assets, not another book')
case(13,'Numeric book title 20th Century Boys',snapshot([file('20世纪少年.pdf')]),expected([unit('20世纪少年.pdf')],['20世纪少年.pdf']),'20 is not a volume')
case(14,'Numeric book title 86',snapshot([file('86.pdf')]),expected([unit('86.pdf')],['86.pdf']),'86 is not a root volume')
p=['作品A Vol.01.pdf','作品A Vol.02.pdf']
case(15,'Sibling exact-prefix virtual series',snapshot([file(x) for x in p]),expected([unit(x) for x in p],[p[0]],[series(None,[member(p[0],1),member(p[1],2)],title='作品A')]),'Root remains Root and physical files remain separate')
p=['作品A Vol.01.pdf']
case(16,'One prefixed volume is not a series',snapshot([file(p[0])]),expected([unit(p[0])],p),'No one-volume automatic Series')
p=['01.pdf','02.pdf']
case(17,'Root bare numbers stay independent',snapshot([file(x) for x in p]),expected([unit(x) for x in p],p),'No numeric root-wide Series or inferred volumes')
p=['作品A/01.pdf','作品A/02.pdf']
case(18,'Bare numbers with explicit work context',snapshot([directory('作品A','WORK')]+[file(x) for x in p]),expected([unit(x) for x in p],['作品A'],[series('作品A',[member(p[0],1,decision='REVIEW'),member(p[1],2,decision='REVIEW')],'REVIEW')]),'Bare-number proposal must be REVIEW')
e,p=vols('作品A',nums=(1,3))
case(19,'Missing volume diagnosis',snapshot(e),expected([unit(x) for x in p],['作品A'],[series('作品A',[member(p[0],1),member(p[1],3)])],diagnostics=['VOLUME_GAP']),'Volume2 must not be fabricated')
e,p=vols('作品A',('PDF','EPUB'),(1,1))
case(20,'Same volume parallel formats',snapshot(e),expected([unit(x) for x in p],['作品A'],editions=1),'Same volume sources keep independent identities and no forced multi-volume Series')
p=['作者A/同名.pdf','作者B/同名.pdf']
case(21,'Same title distinct parents and Root scopes',snapshot([directory('作者A','AUTHOR'),directory('作者B','AUTHOR')]+[file(x) for x in p]),expected([unit(x) for x in p],['作者A','作者B']),'No title-based global dedup; additional property test checks distinct Root IDs')
e,p=vols('作品A');e.append(file('作品A/全集.pdf'))
case(22,'Collection plus volumes',snapshot(e),expected([unit(x) for x in p+['作品A/全集.pdf']],['作品A'],[series('作品A',[member(p[0],1),member(p[1],2),member('作品A/全集.pdf',role='COLLECTION')])]),'Collection is a third source, not an alias of volumes')
e,p=vols('作品A');e += [directory('作品A/特典'),file('作品A/特典/番外篇.pdf')]
case(23,'Extra nested under series',snapshot(e),expected([unit(x) for x in p+['作品A/特典/番外篇.pdf']],['作品A'],[series('作品A',[member(p[0],1),member(p[1],2),member('作品A/特典/番外篇.pdf',role='EXTRA')])],{'作品A/特典':'EXTRAS'}),'Extra must not become main volume3')
e,p=vols('作品A');e.append(file('作品A/角色设定.pdf'))
case(24,'Character reference alongside volumes',snapshot(e),expected([unit(x) for x in p+['作品A/角色设定.pdf']],['作品A'],[series('作品A',[member(p[0],1),member(p[1],2),member('作品A/角色设定.pdf',role='EXTRA')])]),'Character reference is not a numbered continuation')
im,o=image('图片书',('cover.jpg','001.jpg','002.jpg'))
case(25,'Cover exclusion and direct page order',snapshot(im,page_orders=[o]),expected([unit('图片书',o['pages'])],['图片书'],diagnostics=['COVER_EXCLUDED']),'Cover excluded once; no per-image reading units')
im,o=image('展示图',('wallpaper.jpg','keyvisual.png','poster.jpg'))
case(26,'Wallpaper keyvisual poster assets',snapshot(im,page_orders=[o]),expected([],['展示图'],diagnostics=['IMAGE_ASSETS_ONLY','COVER_EXCLUDED']),'Three pictures do not imply a book')
im,o=image('图片书');im.append(file('图片书/readme.nfo',None))
case(27,'Image book with unrelated attachment',snapshot(im,page_orders=[o]),expected([unit('图片书',o['pages'])],['图片书'],diagnostics=['UNSUPPORTED_OR_UNVERIFIED_FILE']),'readme remains in physical tree, not a page/book')
im,o=image('图片书');im2,o2=image('图片书/不同作品')
case(28,'Distinct nested image book does not join parent',snapshot(im+im2,page_orders=[o,o2]),expected([unit('图片书',o['pages']),unit(o2['directory'],o2['pages'])],['图片书']),'Different named child work remains separate; pages never concatenate')
im,o=image('混合');im += [directory('混合/第02卷'),file('混合/第02卷/第02卷.pdf')]
case(29,'Parent images and nested file volume',snapshot(im,page_orders=[o]),expected([unit('混合',o['pages']),unit('混合/第02卷/第02卷.pdf')],['混合'],[series('混合',[member('混合'),member('混合/第02卷/第02卷.pdf',2)],'REVIEW')]),'No direct-page loss; no automatic volume1')
im,o=image('单图',('001.png',))
case(30,'Single image stays review',snapshot(im,page_orders=[o]),expected([],['单图'],diagnostics=['SINGLE_IMAGE_REVIEW']),'No misleading automatic whole book')
im,o=image('自然顺序',('1.png','2.png','10.png'))
case(31,'Existing natural order 1 2 10',snapshot(im,page_orders=[o]),expected([unit('自然顺序',o['pages'])],['自然顺序']),'Do not lexically reorder to 1 10 2')
case(32,'Empty directory safe fallback',snapshot([directory('空目录')]),expected([],['空目录'],roles={'空目录':'AMBIGUOUS'}),'No ghost book')
case(33,'ZIP attachment and CBZ book',snapshot([file('archive.zip',None),file('comic.cbz','CBZ')]),expected([unit('comic.cbz')],['comic.cbz'],diagnostics=['UNSUPPORTED_OR_UNVERIFIED_FILE']),'ZIP stays traceable physical attachment')
case(34,'Unsupported CBR RAR',snapshot([file('a.cbr',None),file('b.rar',None)]),expected([],[],diagnostics=['UNSUPPORTED_OR_UNVERIFIED_FILE']),'Neither file may be declared readable')
case(35,'ASCII case collision Unicode distinct spellings',snapshot([file('Book.pdf'),file('book.pdf'),file('é.pdf'),file('e\u0301.pdf')]),expected([unit('é.pdf'),unit('e\u0301.pdf')],['é.pdf','e\u0301.pdf'],diagnostics=['PATH_CASE_COLLISION','SOURCE_UNAVAILABLE']),'Quarantine ASCII collision; do not guess NFC/NFD equivalence')
e,p=vols('顺序作品',('MOBI','AZW3'));e.reverse()
case(36,'Shuffled traversal supported MOBI AZW3',snapshot(e,'EBOOK'),expected([unit(x) for x in p],['顺序作品'],[series('顺序作品',[member(p[0],1),member(p[1],2)])]),'Permutation property separately compares entire structured result')
e,p=vols('重复执行')
case(37,'Repeat same snapshot exactly',snapshot(e),expected([unit(x) for x in p],['重复执行'],[series('重复执行',[member(p[0],1),member(p[1],2)])]),'Repeat property separately compares entire serialized plan')
case(38,'Partial offline snapshot retains old source',snapshot([dict(path='离线',kind='DIRECTORY',state='OFFLINE')],prior_units=[dict(source_ref='prior-stable-id',path='离线/第01卷.pdf')]),expected([],['离线'],diagnostics=['PRIOR_RETAINED_INCOMPLETE','SOURCE_UNAVAILABLE'],retained=1),'No deletion proposal even though complete flag is true and child missing')
e,p=vols('作品A')
case(39,'Manual Category overrides series',snapshot(e,overrides=[dict(path='作品A',role='CATEGORY')]),expected([unit(x) for x in p],['作品A'],roles={'作品A':'CATEGORY'}),'Explicit Category prevents automatic series and virtual regrouping')
e=[directory('作品A'),file('作品A/书X.pdf'),file('作品A/书Y.pdf')];p=['作品A/书X.pdf','作品A/书Y.pdf']
case(40,'Manual source series and volume overrides',snapshot(e,overrides=[dict(path=p[0],series_directory='作品A',volume=7),dict(path=p[1],series_directory='作品A',volume=8)]),expected([unit(x) for x in p],['作品A'],[series('作品A',[member(p[0],7),member(p[1],8)])]),'Manual members and numbering override automatic category fallback')
e,p=vols('作品A')
case(41,'Remove override restores automatic series',snapshot(e),expected([unit(x) for x in p],['作品A'],[series('作品A',[member(p[0],1),member(p[1],2)])]),'Property test checks unaffected independent work and source refs')
p=['作品A/pdf/第01卷.pdf','作品A/cbz/第01卷.cbz','作品A/pdf/第02卷.pdf']
case(42,'Same volume across format containers',snapshot([directory('作品A'),directory('作品A/pdf'),directory('作品A/cbz')]+[file(x,'CBZ' if x.endswith('cbz') else 'PDF') for x in p]),expected([unit(x) for x in p],['作品A'],[series('作品A',[member(p[0],1),member(p[1],1),member(p[2],2)])],editions=1),'Same number in different containers cannot overwrite source refs')
im,o=image('某作品');im2,o2=image('某作品/第02卷')
case(43,'Unnumbered direct pages and volume2',snapshot(im+im2,page_orders=[o,o2]),expected([unit('某作品',o['pages']),unit(o2['directory'],o2['pages'])],['某作品'],[series('某作品',[member('某作品'),member(o2['directory'],2)],'REVIEW')]),'Unnumbered direct pages never fabricated as volume1')
p=['某作品/正篇/第01卷.pdf','某作品/正篇/第02卷.pdf','某作品/特典/小册子.pdf','某作品/设定资料/资料.pdf']
case(44,'Main extra reference sections',snapshot([directory('某作品'),directory('某作品/正篇'),directory('某作品/特典'),directory('某作品/设定资料')]+[file(x) for x in p]),expected([unit(x) for x in p],['某作品'],[series('某作品',[member(p[0],1),member(p[1],2),member(p[2],role='EXTRA'),member(p[3],role='EXTRA')])]),'Only main section contributes volume numbering')
p=['某作者/小说.epub','某作者/随笔.pdf','某作者/漫画.cbz']
case(45,'Ebook author mixed formats',snapshot([directory('某作者','AUTHOR')]+[file(x,f) for x,f in zip(p,['EPUB','PDF','CBZ'])],'EBOOK'),expected([unit(x) for x in p],['某作者'],roles={'某作者':'CATEGORY'}),'Mixed formats and common author do not imply Series')
case(46,'Corrupt unverified and unknown files',snapshot([file('损坏.pdf',state='CORRUPT'),file('未知.xyz',None),dict(path='未验证.epub',kind='FILE',format='EPUB',verified=False)]),expected([],[],diagnostics=['SOURCE_UNAVAILABLE','UNSUPPORTED_OR_UNVERIFIED_FILE']),'Physical entries retained with diagnostics, no readable success claim')
e=[directory('__MACOSX'),file('__MACOSX/ignored.pdf'),file('.hidden.pdf'),file('draft.tmp',None),directory('$Recycle.Bin'),file('$Recycle.Bin/book.pdf'),file('desktop.ini',None)]
case(47,'Existing exclusions not new hidden/temp policy',snapshot(e),expected([unit('.hidden.pdf'),unit('$Recycle.Bin/book.pdf')],['.hidden.pdf','$Recycle.Bin'],diagnostics=['EXCLUDED_EXISTING_POLICY','UNSUPPORTED_OR_UNVERIFIED_FILE']),'Only existing four names excluded; hidden and recycle names not silently dropped')
e,p=vols('同名');e.append(file('同名.pdf'))
case(48,'Root file same title as child series',snapshot(e),expected([unit(x) for x in p+['同名.pdf']],['同名','同名.pdf'],[series('同名',[member(p[0],1),member(p[1],2)])]),'Root PDF remains independent and outside child series')

e,p=vols('混合系列',('PDF','CBZ'));im,o=image('混合系列/第03卷');e+=im+[directory('混合系列/第04-06卷'),directory('混合系列/特典'),file('混合系列/特典/附录.pdf')];ranges=[f'混合系列/第04-06卷/{v:02}.pdf' for v in (4,5,6)];e += [file(x) for x in ranges];e[0]['hint']='WORK'
case(49,'Combined mixed-format container acceptance example',snapshot(e,page_orders=[o]),expected([unit(x) for x in p+ranges+['混合系列/特典/附录.pdf']]+[unit(o['directory'],o['pages'])],['混合系列'],[series('混合系列',[member(p[0],1),member(p[1],2),member(o['directory'],3)]+[member(x,i+4,decision='REVIEW') for i,x in enumerate(ranges)]+[member('混合系列/特典/附录.pdf',role='EXTRA')],'REVIEW')]),'Seven unique sources; range container cannot become volume6 or consume direct image pages')

for c in cases:
    (OUT/f'{c["id"]:02}.json').write_text(json.dumps(c,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
for i, label in [(3,'root-parallel'),(49,'mixed-series'),(10,'author-category')]:
    c=next(c for c in cases if c['id']==i)
    (OUT/f'example-{label}.input.json').write_text(json.dumps(c['input'],ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(f'Authored {len(cases)} fixture cases in {OUT}')
