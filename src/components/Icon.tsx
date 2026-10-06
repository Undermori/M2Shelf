import type { ReactNode, SVGProps } from "react";

export type IconName =
  | "archive" | "arrow-left" | "bangumi" | "check" | "chevron" | "clock"
  | "close" | "database" | "download" | "edit" | "external" | "file" | "folder"
  | "folder-open" | "grid" | "image" | "info" | "list" | "more"
  | "play" | "plus" | "refresh" | "search" | "settings" | "shield"
  | "stop" | "trash" | "warning" | "work" | "audio" | "subtitle" | "font"
  | "globe" | "tag" | "bookmark" | "eye-off";

interface IconProps extends SVGProps<SVGSVGElement> { name: IconName }

const paths: Record<IconName, ReactNode> = {
  "eye-off": <><path d="m3 3 18 18M10.5 5.2 12 5c5.5 0 9 7 9 7a19 19 0 0 1-3.2 4M6.2 6.2C4.2 8 3 12 3 12s3.5 7 9 7a9 9 0 0 0 4-1"/><path d="M9.9 9.9a3 3 0 0 0 4.2 4.2"/></>,
  archive: <><path d="M4 7.5h16v12H4z"/><path d="M3 4.5h18v3H3zM9 12h6"/></>,
  audio: <><path d="M9 18V6l9-2v12"/><circle cx="6.5" cy="18" r="2.5"/><circle cx="15.5" cy="16" r="2.5"/></>,
  "arrow-left": <path d="m15 18-6-6 6-6M9 12h11"/>,
  bangumi: <><circle cx="12" cy="12" r="9"/><path d="M8 9.5h.01M16 9.5h.01M8.5 15c2.3 1.6 4.7 1.6 7 0"/></>,
  bookmark: <path d="M6 3.5h12v17l-6-4-6 4v-17Z"/>,
  check: <path d="m5 12 4 4L19 6"/>,
  chevron: <path d="m9 5 7 7-7 7"/>,
  clock: <><circle cx="12" cy="12" r="8.5"/><path d="M12 7v5l3.4 2"/></>,
  close: <path d="m6 6 12 12M18 6 6 18"/>,
  database: <><ellipse cx="12" cy="5.5" rx="7.5" ry="3"/><path d="M4.5 5.5v6c0 1.7 3.4 3 7.5 3s7.5-1.3 7.5-3v-6M4.5 11.5v6c0 1.7 3.4 3 7.5 3s7.5-1.3 7.5-3v-6"/></>,
  download: <><path d="M12 3v12m0 0 5-5m-5 5-5-5"/><path d="M5 20h14"/></>,
  edit: <><path d="M4 20h4l11-11-4-4L4 16v4Z"/><path d="m13.5 6.5 4 4"/></>,
  external: <><path d="M14 4h6v6M20 4l-9 9"/><path d="M18 13v6H5V6h6"/></>,
  file: <><path d="M6 3h8l4 4v14H6z"/><path d="M14 3v5h5"/></>,
  folder: <path d="M3 6.5h7l2 2h9v10.5H3z"/>,
  font: <><path d="M5 5h14M12 5v14M8 19h8"/><path d="m8.5 14 3.5-9 3.5 9"/></>,
  "folder-open": <><path d="M3 8V6h7l2 2h9v3"/><path d="m4 19 3-8h15l-3 8H4Z"/></>,
  grid: <path d="M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z"/>,
  globe: <><circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c2.3 2.5 3.5 5.5 3.5 9S14.3 18.5 12 21M12 3c-2.3 2.5-3.5 5.5-3.5 9S9.7 18.5 12 21"/></>,
  image: <><rect x="3" y="4" width="18" height="16" rx="2"/><circle cx="9" cy="9" r="2"/><path d="m4 18 5-5 3 3 2-2 6 5"/></>,
  info: <><circle cx="12" cy="12" r="9"/><path d="M12 11v6M12 7h.01"/></>,
  list: <><path d="M9 6h11M9 12h11M9 18h11"/><path d="M4 6h.01M4 12h.01M4 18h.01"/></>,
  more: <><circle cx="5" cy="12" r="1" fill="currentColor" stroke="none"/><circle cx="12" cy="12" r="1" fill="currentColor" stroke="none"/><circle cx="19" cy="12" r="1" fill="currentColor" stroke="none"/></>,
  play: <path d="m8 5 11 7-11 7V5Z"/>,
  plus: <path d="M12 5v14M5 12h14"/>,
  refresh: <><path d="M20 7v5h-5"/><path d="M19 12a7 7 0 1 0-2 5"/></>,
  search: <><circle cx="11" cy="11" r="7"/><path d="m16.5 16.5 4 4"/></>,
  settings: <><circle cx="12" cy="12" r="3"/><path d="M19 13.5v-3l-2-.7-.7-1.7.9-1.9-2.1-2.1-1.9.9-1.7-.7-.7-2h-3l-.7 2-1.7.7-1.9-.9-2.1 2.1.9 1.9-.7 1.7-2 .7v3l2 .7.7 1.7-.9 1.9 2.1 2.1 1.9-.9 1.7.7.7 2h3l.7-2 1.7-.7 1.9.9 2.1-2.1-.9-1.9.7-1.7 2-.7Z"/></>,
  shield: <path d="M12 2.8 19 6v5.4c0 4.6-3 8.1-7 9.8-4-1.7-7-5.2-7-9.8V6l7-3.2Zm-3 9.1 2 2 4.5-4.5"/>,
  stop: <rect x="6" y="6" width="12" height="12" rx="2"/>,
  subtitle: <><rect x="3" y="5" width="18" height="14" rx="2"/><path d="M6.5 11h5M6.5 15h3M13.5 11h4M11.5 15h6"/></>,
  tag: <><path d="M3.5 12.5V5.5a2 2 0 0 1 2-2h7l8 8-9 9-8-8Z"/><circle cx="8.25" cy="8.25" r="1.25"/></>,
  trash: <><path d="M4 7h16M9 3h6l1 4H8l1-4ZM7 7l1 14h8l1-14"/><path d="M10 11v6M14 11v6"/></>,
  warning: <><path d="m12 3 10 18H2L12 3Z"/><path d="M12 9v5M12 18h.01"/></>,
  work: <><rect x="5" y="3" width="14" height="18" rx="2"/><path d="m10 9 5 3-5 3V9Z"/></>,
};

export function Icon({ name, ...props }: IconProps) {
  return <svg aria-hidden="true" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" {...props}>{paths[name]}</svg>;
}
