import {useState, type ReactNode} from 'react';

export type NavigationDirection = 'forward' | 'backward' | 'section';

/** Animate a committed destination without remounting its children or delaying navigation. */
export function NavigationStage({navigationKey, direction, children, className = '', animateInitial = false}: {
  navigationKey: string; direction: NavigationDirection; children: ReactNode; className?: string; animateInitial?: boolean;
}) {
  const [entry, setEntry] = useState({key: navigationKey, direction, cycle: animateInitial ? 1 : 0});
  if (entry.key !== navigationKey) {
    setEntry({key: navigationKey, direction, cycle: entry.cycle + 1});
  }
  return <div className={`nav-stage-surface ${className}`} data-nav-key={navigationKey}
    data-nav-direction={entry.direction} data-nav-cycle={entry.cycle % 2 ? 'a' : 'b'}
    data-nav-active={entry.cycle > 0}>{children}</div>;
}
