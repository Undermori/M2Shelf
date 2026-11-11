// @vitest-environment jsdom
import {cleanup, fireEvent, render} from '@testing-library/react';
import {afterEach, expect, it, vi} from 'vitest';
import {PosterImage} from './PosterImage';

afterEach(()=>{cleanup();vi.restoreAllMocks();});
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
  expect(image.getAttribute('loading')).toBe('lazy');
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

it('hides an unrelated identity at a revision change and forwards real image errors', () => {
  const onError = vi.fn();
  const result = render(<PosterImage {...props} onError={onError}/>);
  const image = result.getByRole('img', {hidden: true}) as HTMLImageElement;
  load(image);
  result.rerender(<PosterImage {...props} src="data:image/jpeg;base64,new" cacheKey="fixture-2" onError={onError}/>);
  const next = result.getByRole('img', {hidden: true}) as HTMLImageElement;
  expect(next).toBe(image);
  expect(next.style.visibility).toBe('hidden');
  fireEvent.error(next);
  expect(onError).toHaveBeenCalledOnce();
});

it('keeps the same decoded element when metadata changes but the image bytes do not',()=>{
 const result=render(<PosterImage {...props}/>);const image=result.getByRole('img',{hidden:true}) as HTMLImageElement;load(image);result.rerender(<PosterImage {...props} cacheKey="metadata-only-revision"/>);expect(result.getByRole('img')).toBe(image);expect(image.style.visibility).toBe('visible');
});
it('reveals an already cached decoded image before the first route paint',()=>{
 vi.spyOn(HTMLImageElement.prototype,'complete','get').mockReturnValue(true);vi.spyOn(HTMLImageElement.prototype,'naturalWidth','get').mockReturnValue(1200);vi.spyOn(HTMLImageElement.prototype,'naturalHeight','get').mockReturnValue(800);
 const result=render(<PosterImage {...props}/>);const image=result.getByRole('img') as HTMLImageElement;expect(image.style.visibility).toBe('visible');expect(image.classList.contains('is-wide-artwork')).toBe(true);
});

it('retains the same visible image while a higher tier of the same indexed cover loads',()=>{
 const result=render(<PosterImage {...props} cacheKey="node:9:256"/>);const image=result.getByRole('img',{hidden:true}) as HTMLImageElement;load(image);
 result.rerender(<PosterImage {...props} src="http://m2poster.localhost/node/9/512?v=0123456789abcdef" cacheKey="node:9:512"/>);
 expect(result.getByRole('img')).toBe(image);expect(image.style.visibility).toBe('visible');expect(image.getAttribute('decoding')).toBe('sync');
 load(image,512,768);expect(image.classList.contains('is-wide-artwork')).toBe(false);
});
