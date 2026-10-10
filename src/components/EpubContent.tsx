import { createElement, useEffect, useMemo, useState, type ReactNode } from "react";
import {ComicPageCache} from '../lib/comicReader';
import {api} from '../lib/api';
import {EpubIllustration} from './EpubIllustration';
import { useI18n } from "../lib/i18n";
import type {TextReaderSettings} from '../types/textReader';

export type EpubRun = { text: string; bold?: boolean; italic?: boolean; superscript?: boolean; subscript?: boolean };
export type EpubBlock = {kind:'anchor';id:string} | { kind: "text"; text: string; tag?: string; runs?: EpubRun[] } | { kind: "image"; data_url: string } | {kind:'imageReference';locator:string;size:number;crc32:number};
const tags = new Set(["p", "h1", "h2", "h3", "h4", "h5", "h6", "blockquote", "pre", "li"]);
export const isLocalBookImage = (value: string) => /^data:image\/(?:png|jpeg|gif|webp|avif|bmp);base64,/.test(value);

/** Only application-selected text elements and validated local raster images reach React. */
export function EpubContent({ blocks, zoom, height, settings, columnWidth, source, onLayout }: { blocks: EpubBlock[]; zoom: number; height: number; settings?: TextReaderSettings; columnWidth?: number;source?:{bookId:number;chapter:number;revision:string};onLayout?:()=>void }) {
  const { t } = useI18n();
  const visible=useMemo(()=>new Set<number>(),[blocks]);
  const [cache,setCache]=useState<ComicPageCache|null>(null);
  useEffect(()=>{
    const next=new ComicPageCache(source?.bookId??0,blocks.map((_,pageIndex)=>({pageIndex,pageName:''})),()=>undefined,(_book,index)=>api.readEpubIllustration(source!.bookId,source!.chapter,source!.revision,index));
    setCache(next);return()=>next.dispose();
  },[blocks,source?.bookId,source?.chapter,source?.revision]); // eslint-disable-line react-hooks/exhaustive-deps
  const contentBlocks=blocks.filter(block=>block.kind!=='anchor');
  const illustrated = contentBlocks.length > 0 && contentBlocks.every(block => block.kind === "image"||block.kind==='imageReference');
  const font = settings?.fontFamily === 'SERIF' ? '"Yu Mincho", "SimSun", Georgia, serif' : settings?.fontFamily === 'MONO' ? 'Consolas, "MS Gothic", monospace' : 'var(--font-ui)';
  return <article className={`epub-chapter${illustrated ? " is-illustration" : ""}${settings ? ` text-content${settings.mode === 'PAGED' ? ' is-paged' : ''}` : ''}`} style={{ fontSize: `${settings?.fontSize ?? 18 * zoom}px`, "--reading-height": `${height}px`, ...(settings ? {fontFamily: font, fontWeight: settings.fontWeight, fontStyle: settings.italic ? 'italic' : 'normal', textAlign: settings.alignment, lineHeight: settings.lineHeight, letterSpacing: `${settings.letterSpacing}em`, wordSpacing: `${settings.wordSpacing}em`, '--paragraph-spacing': `${settings.paragraphSpacing}em`, height: settings.mode === 'PAGED' ? height : undefined, columnWidth: settings.mode === 'PAGED' ? columnWidth : undefined} : {}) } as React.CSSProperties}>
    {blocks.map((block, index) => {
      if(block.kind==='anchor')return <span key={index} data-book-anchor={block.id} data-book-block={index}/>;
      if(block.kind==='imageReference')return source&&cache?<div data-book-block={index} key={index}><EpubIllustration cache={cache} index={index} visible={visible} onLayout={onLayout}/></div>:null;
      if (block.kind === "image") return isLocalBookImage(block.data_url)
        ? <img data-book-block={index} key={index} src={block.data_url} alt={t("ebook.illustration")} onLoad={onLayout}/> : null;
      const tag = tags.has(block.tag ?? "") ? block.tag! : "p";
      const content = block.runs?.length ? block.runs.map((run, key) => {
        let child: ReactNode = run.text;
        if (run.bold) child = <strong>{child}</strong>;
        if (run.italic) child = <em>{child}</em>;
        if (run.superscript) child = <sup>{child}</sup>;
        if (run.subscript) child = <sub>{child}</sub>;
        return <span key={key}>{child}</span>;
      }) : block.text;
      return createElement(tag === "li" ? "p" : tag, { key: index, 'data-book-block':index, className: tag === "li" ? "epub-list-item" : undefined }, content);
    })}
  </article>;
}
