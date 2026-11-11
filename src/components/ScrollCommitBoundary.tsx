import {Component,type ReactNode,type RefObject} from 'react';

interface Props {
  children:ReactNode;
  scrollRef:RefObject<HTMLElement|null>;
  pending:RefObject<number|null>;
  ready:boolean;
}

/** Reset a new destination against the old, already laid-out DOM, before React
 * inserts a large new list. Nonzero history restores keep the existing layout
 * effect because they require the destination's real scroll range. */
export class ScrollCommitBoundary extends Component<Props> {
  componentDidMount(){
    if(this.props.ready&&this.props.pending.current===0)this.props.pending.current=null;
  }
  getSnapshotBeforeUpdate(){
    const {ready,pending,scrollRef}=this.props;
    if(ready&&pending.current===0&&scrollRef.current){scrollRef.current.scrollTop=0;pending.current=null;}
    return null;
  }
  componentDidUpdate(){/* The reset intentionally happens before DOM mutations. */}
  render(){return this.props.children;}
}
