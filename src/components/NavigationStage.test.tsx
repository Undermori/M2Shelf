// @vitest-environment jsdom
import {afterEach, expect, it, vi} from 'vitest';
import {cleanup, render, screen} from '@testing-library/react';
import {useEffect} from 'react';
import {NavigationStage} from './NavigationStage';
afterEach(cleanup);

it('replays only for committed destinations, retaining child instances and focus', () => {
  const mounted = vi.fn();
  function Child({query}: {query: string}) { useEffect(mounted, []); return <input aria-label="query" value={query} readOnly/>; }
  const view = render(<NavigationStage navigationKey="all" direction="section"><Child query=""/></NavigationStage>);
  const input = screen.getByRole('textbox'); input.focus();
  const stage = () => view.container.firstElementChild!;
  expect(stage().getAttribute('data-nav-active')).toBe('false');
  view.rerender(<NavigationStage navigationKey="all" direction="section"><Child query="filter"/></NavigationStage>);
  expect(stage().getAttribute('data-nav-active')).toBe('false');
  view.rerender(<NavigationStage navigationKey="detail:1" direction="forward"><Child query="filter"/></NavigationStage>);
  expect(stage().getAttribute('data-nav-direction')).toBe('forward');
  expect(stage().getAttribute('data-nav-cycle')).toBe('a');
  view.rerender(<NavigationStage navigationKey="detail:2" direction="forward"><Child query="filter"/></NavigationStage>);
  expect(stage().getAttribute('data-nav-cycle')).toBe('b');
  view.rerender(<NavigationStage navigationKey="all" direction="backward"><Child query="filter"/></NavigationStage>);
  expect(stage().getAttribute('data-nav-direction')).toBe('backward');
  expect(document.activeElement).toBe(input);
  expect(mounted).toHaveBeenCalledTimes(1);
});
