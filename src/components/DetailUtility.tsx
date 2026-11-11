import type {ReactNode} from 'react';

/** Shared presentation only; note and progress editors retain their original state/targets. */
export function DetailUtility({note,progress}:{note:ReactNode;progress?:ReactNode}) {
 if(!note&&!progress)return null;
 return <div className="detail-utility">{note}{progress}</div>;
}
