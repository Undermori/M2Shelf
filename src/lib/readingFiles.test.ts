import {describe, expect, it} from 'vitest';
import {readingFiles} from './readingFiles';
import type {ResourceFile} from '../types/media';
import type {ComicBook} from '../types/comic';

describe('shared readable-file presentation',()=>{
 it('supports old file indexes in every library without treating ZIP or loose images as book files',()=>{
  const extensions=['mobi','AZW3','.epub','txt','pdf','cbz','zip','png','md'];
  const files=extensions.map((extension,id)=>({id,extension,absolutePath:`X:/Work/file${id}.${extension}`} as ResourceFile));
  const result=readingFiles([],files);
  expect(result.count).toBe(6);
  expect(result.other.map(file=>file.extension)).toEqual(['zip','png','md']);
  expect(result.readable.map(file=>file.id)).toEqual([0,1,2,3,4,5]);
 });
 it('keeps a single row after opening/rescanning a book, including Windows case and separator differences',()=>{
  const files=[{id:9,extension:'mobi',absolutePath:'X:\\Work\\Book.MOBI'} as ResourceFile];
  const books=[{id:20,sourcePath:'x:/work/book.mobi'} as ComicBook];
  expect(readingFiles(books,files)).toEqual({books,readable:[],other:[],count:1});
  expect(files).toHaveLength(1);
 });
});
