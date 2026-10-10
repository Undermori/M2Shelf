// @vitest-environment jsdom
import {useRef} from 'react';
import {act, cleanup, render, screen} from '@testing-library/react';
import {afterEach, beforeEach, expect, it, vi} from 'vitest';
import {usePosterViewportLifecycle} from './usePosterViewportLifecycle';
import {observePosterViewport} from '../lib/posterViewportObserver';

class Observer {
  static instances: Observer[] = [];
  targets = new Set<Element>();
  disconnect = vi.fn(() => this.targets.clear());
  observe = vi.fn((target: Element) => this.targets.add(target));
  unobserve = vi.fn((target: Element) => this.targets.delete(target));
  constructor(public callback: IntersectionObserverCallback, public options: IntersectionObserverInit) {
    Observer.instances.push(this);
  }
  deliver(targets: Element[], isIntersecting: boolean) {
    this.callback(targets.map(target => ({target, isIntersecting}) as IntersectionObserverEntry), this as unknown as IntersectionObserver);
  }
}

function Poster({id}: {id: number}) {
  const ref = useRef<HTMLDivElement>(null);
  const state = usePosterViewportLifecycle(ref, id, {activationMarginPx: 1000, retentionMarginPx: 1800});
  return <div ref={ref} data-testid={`poster-${id}`} data-active={state.coverVisible}>{id}</div>;
}

beforeEach(() => {Observer.instances = []; vi.stubGlobal('IntersectionObserver', Observer);});
afterEach(() => {cleanup(); vi.unstubAllGlobals();});

it('batches a large poster grid in two observers rooted in its actual scroll container', () => {
  const {container, unmount} = render(<div className="content-scroll">{Array.from({length: 180}, (_, id) => <Poster key={id} id={id}/>)}</div>);
  expect(Observer.instances).toHaveLength(2);
  const [activation, retention] = Observer.instances;
  expect(activation.options).toEqual({root: container.firstChild, rootMargin: '1000px 0px'});
  expect(retention.options.rootMargin).toBe('1800px 0px');
  expect(activation.targets.size).toBe(180);
  act(() => activation.deliver([screen.getByTestId('poster-3'), screen.getByTestId('poster-4')], true));
  expect(screen.getByTestId('poster-3').getAttribute('data-active')).toBe('true');
  expect(screen.getByTestId('poster-20').getAttribute('data-active')).toBe('false');
  unmount();
  expect(activation.disconnect).toHaveBeenCalledOnce();
  expect(retention.disconnect).toHaveBeenCalledOnce();
  // A queued native delivery after unmount must have no live subscribers.
  act(() => activation.deliver([...activation.targets], true));
});

it('keeps nearby posters during soft overage and evicts only an offscreen poster, then reactivates it', () => {
  render(<div className="content-scroll">{Array.from({length: 65}, (_, id) => <Poster key={id} id={id}/>)}</div>);
  const [activation, retention] = Observer.instances;
  act(() => activation.deliver([...activation.targets], true));
  expect(screen.getByTestId('poster-0').getAttribute('data-active')).toBe('true');
  act(() => retention.deliver([screen.getByTestId('poster-0')], false));
  expect(screen.getByTestId('poster-0').getAttribute('data-active')).toBe('false');
  expect(screen.getByTestId('poster-1').getAttribute('data-active')).toBe('true');
  act(() => activation.deliver([screen.getByTestId('poster-0')], true));
  expect(screen.getByTestId('poster-0').getAttribute('data-active')).toBe('true');
});

it('separates scroll roots and keeps the shared observation until its final subscriber leaves', () => {
  const root = document.createElement('div'), otherRoot = document.createElement('div'), target = document.createElement('div');
  const first = vi.fn(), second = vi.fn(), other = vi.fn();
  const stopFirst = observePosterViewport(target, root, 1000, first);
  const stopSecond = observePosterViewport(target, root, 1000, second);
  const stopOther = observePosterViewport(target, otherRoot, 1000, other);
  const [observer, otherObserver] = Observer.instances;
  expect(Observer.instances).toHaveLength(2);
  stopFirst(); observer.deliver([target], true);
  expect(first).not.toHaveBeenCalled(); expect(second).toHaveBeenCalledWith(true);
  expect(observer.unobserve).not.toHaveBeenCalled();
  stopSecond(); observer.deliver([target], false);
  expect(second).toHaveBeenCalledOnce(); expect(observer.disconnect).toHaveBeenCalledOnce();
  stopFirst(); // Repeated cleanup must not discard a different root's active pool.
  otherObserver.deliver([target], true); expect(other).toHaveBeenCalledWith(true);
  stopOther(); expect(otherObserver.disconnect).toHaveBeenCalledOnce();
});
