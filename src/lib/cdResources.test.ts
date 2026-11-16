import {expect, it} from 'vitest';
import {isCdsDirectoryName, splitCdsResources} from './cdResources';
import type {MediaNode, NodeDetail, ResourceFile} from '../types/media';

it('keeps native SP ownership disjoint from CDs and the lower resources, including empty shells', () => {
  const node={id:1,absolutePath:'X:/Work'} as MediaNode;
  const shell={id:2,folderName:'SPs',absolutePath:'X:/Work/SPs'} as MediaNode;
  const child={id:3,folderName:'Scans',absolutePath:'x:/work/SPs/Scans'} as MediaNode;
  const independent={id:4,folderName:'Specials',absolutePath:'X:/Work/Specials',manualTypeOverride:true} as MediaNode;
  const sp={id:8,absolutePath:'X:/Work/SPs/Commentary.mka',resourceType:'AUDIO'} as ResourceFile;
  const cd={id:9,absolutePath:'X:/Work/CDs/Theme.flac',resourceType:'AUDIO'} as ResourceFile;
  const result=splitCdsResources({node,children:[shell,child,independent],resourceFiles:[sp,cd],spFiles:[sp,sp],spFolders:[child,child],expandedFolderIds:[2]} as NodeDetail);
  expect(result.spFiles).toEqual([sp]);expect(result.spFolders).toEqual([child]);
  expect(result.cdsFiles).toEqual([cd]);expect(result.otherFiles).toEqual([]);
  expect(result.otherFolders).toEqual([independent]);
  const empty=splitCdsResources({node,children:[shell],resourceFiles:[],expandedFolderIds:[2],spFiles:[],spFolders:[]} as unknown as NodeDetail);
  expect(empty.otherFolders).toEqual([]);
});

it('shares conservative music directory names without absorbing numbered movie discs', () => {
  for (const name of ['CD', 'CDs', 'CDs [FLAC]', '[CDs]', 'OST', 'OST [Hi-Res]', 'OSTs', 'Soundtrack', 'Soundtrack (Lossless)', 'Soundtracks', 'Chara Song', 'Drama CDs', '特典CD', '音乐特典', '原声集']) expect(isCdsDirectoryName(name), name).toBe(true);
  for (const name of ['CD1', 'CD 01', 'Disc 1', 'OSTrich', 'Soundtrack Story', 'CDs 2', 'CDs 2 [FLAC]']) expect(isCdsDirectoryName(name), name).toBe(false);
});

it('excludes unwrapped outer CDs wrappers and expanded folders while keeping inner folders and files', () => {
  const node = {id:1, absolutePath:'X:/OST/Work'} as MediaNode;
  const resource = (id:number, path:string, resourceType:ResourceFile['resourceType']='OTHER') => ({id,absolutePath:path,resourceType} as ResourceFile);
  const files = [resource(1, 'X:/OST/Work/subtitles.ass'), resource(2,'x:\\ost\\work\\CDs\\cover.jpg','IMAGE'), resource(3,'X:/OST/Work/music.flac','AUDIO'), resource(4,'X:/OST/Work/CD1/readme.md')];
  const folders = ['CDs','Disc 1','SPs'].map((folderName,id) => ({id:id+10,folderName,absolutePath:`X:/OST/Work/${folderName}`} as MediaNode));
  const innerOst = {id:99, folderName:'OST Album', absolutePath:'X:/OST/Work/CDs/OST Album'} as MediaNode;
  const innerBooklet = resource(98, 'X:/OST/Work/CDs/Booklet.pdf', 'DOCUMENT');
  const result = splitCdsResources({node,children:folders,resourceFiles:files,cdsFolders:[innerOst],cdsFiles:[innerBooklet],expandedFolderIds:[12]} as NodeDetail);
  expect(result.cdsFiles.map(f=>f.id)).toEqual([98,2,3]);
  expect(result.otherFiles).toEqual([files[0],files[3]]);
  expect(result.cdsFolders).toEqual([innerOst]);
  expect(result.otherFolders).toEqual([folders[1]]);
});

it('uses every real aggregate source and never matches a sibling by path prefix', () => {
  const sources = [{id:1,absolutePath:'X:/Work'}, {id:2,absolutePath:'Y:/Edition'}] as MediaNode[];
  const files = ['Y:/Edition/OST/cover.jpg','X:/Work sibling/CDs/cover.jpg'].map((absolutePath,id)=>({id,absolutePath,resourceType:'IMAGE'} as ResourceFile));
  const result = splitCdsResources({node:sources[0],workSources:sources,children:[],resourceFiles:files} as unknown as NodeDetail);
  expect(result.cdsFiles).toEqual([files[0]]);
  expect(result.otherFiles).toEqual([files[1]]);
});

it('keeps every indexed CD attachment in one partition without absorbing names outside its source', () => {
  const node={id:1,absolutePath:'X:/CDs Library/作品'} as MediaNode;
  const resources=[
    {id:1,absolutePath:'X:/CDs Library/作品/CDs/Disc 1/Booklet.pdf',resourceType:'DOCUMENT'},
    {id:2,absolutePath:'X:/CDs Library/作品/Theme.flac',resourceType:'AUDIO'},
    {id:3,absolutePath:'X:/CDs Library/作品/CD1/notes.txt',resourceType:'DOCUMENT'},
    {id:4,absolutePath:'X:/CDs Library/作品2/CDs/cover.png',resourceType:'IMAGE'},
  ] as ResourceFile[];
  const folders=[
    {id:10,folderName:'Disc 1',absolutePath:'X:/CDs Library/作品/CDs/Disc 1'},
    {id:11,folderName:'CD1',absolutePath:'X:/CDs Library/作品/CD1'},
  ] as MediaNode[];
  const result=splitCdsResources({node,resourceFiles:resources,children:folders} as NodeDetail);
  expect(result.cdsFiles).toEqual(resources.slice(0,2));
  expect(result.otherFiles).toEqual(resources.slice(2));
  expect(result.cdsFolders).toEqual([folders[0]]);
  expect(result.otherFolders).toEqual([folders[1]]);
  expect(new Set([...result.cdsFiles,...result.otherFiles].map(file=>file.id)).size).toBe(resources.length);
  expect(new Set([...result.cdsFolders,...result.otherFolders].map(folder=>folder.id)).size).toBe(folders.length);
});
