// @vitest-environment jsdom
import {cleanup, fireEvent, render} from '@testing-library/react';
import {afterEach, expect, it, vi} from 'vitest';
import {PosterImage} from './PosterImage';

afterEach(cleanup);
const props = {alt: 'Cover', src: 'data:image/jpeg;base64,fixture', cacheKey: 'fixture-1', onError: vi.fn()};

function load(image: HTMLImageElement, width = 600, height = 900) {
  Object.defineProperties(image, {naturalWidth: {value: width, configurable: true}, naturalHeight: {value: height, configurable: true}});
  fireEvent.load(image);
}

it('shows a single prepared image and never replaces it or redraws it after scroll/working-set changes', () => {
  const result = render(<div className="content-scroll"><PosterImage {...props}/></div>);
  const image = result.getByRole('img', {hidden: true}) as HTMLImageElement;
  expect(image.style.visibility).toBe('hidden');
  load(image);
  expect(image.style.visibility).toBe('visible');
  fireEvent.scroll(result.container.firstElementChild!);
  result.rerender(<div className="content-scroll"><PosterImage {...props} active={false}/></div>);
  expect(result.getByRole('img', {hidden: true})).toBe(image);
  expect(image.style.visibility).toBe('visible');
  expect(image.src).toBe(props.src);
  expect(result.container.querySelector('canvas')).toBeNull();
  result.rerender(<div className="content-scroll"><PosterImage {...props}/></div>);
  expect(result.getByRole('img', {hidden: true})).toBe(image);
});

it('uses the final containment before revealing wide artwork and preserves it when scrolling away', () => {
  const result = render(<PosterImage {...props}/>);
  const image = result.getByRole('img', {hidden: true}) as HTMLImageElement;
  load(image, 1200, 800);
  expect(image.classList.contains('is-wide-artwork')).toBe(true);
  expect(image.style.visibility).toBe('visible');
  result.rerender(<PosterImage {...props} active={false}/>);
  expect(image.classList.contains('is-wide-artwork')).toBe(true);
});

it('discards the old source at an explicit revision change and forwards real image errors', () => {
  const onError = vi.fn();
  const result = render(<PosterImage {...props} onError={onError}/>);
  const image = result.getByRole('img', {hidden: true}) as HTMLImageElement;
  load(image);
  result.rerender(<PosterImage {...props} src="data:image/jpeg;base64,new" cacheKey="fixture-2" onError={onError}/>);
  const next = result.getByRole('img', {hidden: true}) as HTMLImageElement;
  expect(next).not.toBe(image);
  expect(next.style.visibility).toBe('hidden');
  fireEvent.error(next);
  expect(onError).toHaveBeenCalledOnce();
});
