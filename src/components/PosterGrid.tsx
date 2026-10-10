import type { MediaNode, ViewMode } from "../types/media";
import { MediaCard } from "./MediaCard";
import type {ReactNode} from 'react';

interface PosterGridProps {
  nodes: MediaNode[];
  showModifiedTime?: boolean;
  viewMode: ViewMode;
  onOpen: (node: MediaNode) => void;
  onMenu: (event: React.MouseEvent, node: MediaNode) => void;
  onBangumi: (node: MediaNode) => void;
  onRetryCover: (node: MediaNode, imageDecodeFailed?: boolean) => void;
  coverRevision: number;
  watchedAtByNodeId?: ReadonlyMap<number, string>;
  editMode?: boolean;
  selectedNodeIds?: ReadonlySet<number>;
  onSelect?: (node: MediaNode) => void;
  children?: ReactNode;
}

export function PosterGrid(props: PosterGridProps) {
  return (
    <div className={`poster-grid poster-grid-${props.viewMode}`}>
      {props.nodes.map((node) => <MediaCard showModifiedTime={props.showModifiedTime} key={node.id} node={node} viewMode={props.viewMode} onOpen={props.onOpen} onMenu={props.onMenu} onBangumi={props.onBangumi} onRetryCover={props.onRetryCover} coverRevision={props.coverRevision} watchedAt={props.watchedAtByNodeId?.get(node.id)} editMode={props.editMode} selected={props.selectedNodeIds?.has(node.id)} onSelect={props.onSelect} />)}
      {props.children}
    </div>
  );
}
