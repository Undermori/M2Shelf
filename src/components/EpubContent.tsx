import { createElement, type ReactNode } from "react";
import { useI18n } from "../lib/i18n";

export type EpubRun = { text: string; bold?: boolean; italic?: boolean; superscript?: boolean; subscript?: boolean };
export type EpubBlock = { kind: "text"; text: string; tag?: string; runs?: EpubRun[] } | { kind: "image"; data_url: string };
const tags = new Set(["p", "h1", "h2", "h3", "h4", "h5", "h6", "blockquote", "pre", "li"]);
export const isLocalBookImage = (value: string) => /^data:image\/(?:png|jpeg|gif|webp|avif|bmp);base64,/.test(value);

/** Only application-selected text elements and validated local raster images reach React. */
export function EpubContent({ blocks, zoom, height }: { blocks: EpubBlock[]; zoom: number; height: number }) {
  const { t } = useI18n();
  const illustrated = blocks.length > 0 && blocks.every(block => block.kind === "image");
  return <article className={`epub-chapter${illustrated ? " is-illustration" : ""}`} style={{ fontSize: `${18 * zoom}px`, "--reading-height": `${height}px` } as React.CSSProperties}>
    {blocks.map((block, index) => {
      if (block.kind === "image") return isLocalBookImage(block.data_url)
        ? <img key={index} src={block.data_url} alt={t("ebook.illustration")} /> : null;
      const tag = tags.has(block.tag ?? "") ? block.tag! : "p";
      const content = block.runs?.length ? block.runs.map((run, key) => {
        let child: ReactNode = run.text;
        if (run.bold) child = <strong>{child}</strong>;
        if (run.italic) child = <em>{child}</em>;
        if (run.superscript) child = <sup>{child}</sup>;
        if (run.subscript) child = <sub>{child}</sub>;
        return <span key={key}>{child}</span>;
      }) : block.text;
      return createElement(tag === "li" ? "p" : tag, { key: index, className: tag === "li" ? "epub-list-item" : undefined }, content);
    })}
  </article>;
}
