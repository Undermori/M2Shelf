// @vitest-environment jsdom
import {afterEach,describe,expect,it,vi} from 'vitest';
import {cleanup,fireEvent,render,screen} from '@testing-library/react';
import {I18nProvider} from '../lib/i18n';
import {MatchingResults} from './MatchingResults';
import type {MatchResult} from './TmdbMatch';
const results:MatchResult[]=[
 {provider:'BANGUMI',id:7,title:'长标题 日本語 한국어',originalTitle:'Original title',date:'2020-01-01',poster:null,subject:{subjectId:7,title:'Original title',titleCn:null,titleEn:null,titleJa:null,titleKo:null,matchAliases:[],date:'2020-01-01',imageUrl:null,summary:null,subjectType:6}},
 {provider:'BANGUMI',id:8,title:'Second title',originalTitle:'Second title',date:null,poster:null,subject:{subjectId:8,title:'Second title',titleCn:null,titleEn:null,titleJa:null,titleKo:null,matchAliases:[],date:null,imageUrl:null,summary:null,subjectType:6}},
];
afterEach(cleanup);
describe('shared matching result operations',()=>{
 it('keeps one native selection action per candidate and sends the original DTO',()=>{
  const choose=vi.fn();render(<I18nProvider><MatchingResults results={results} loading={false} searched error={false} bindingId={null} provider="BANGUMI" onChoose={choose}/></I18nProvider>);
  expect(screen.getAllByRole('button',{name:'选择'})).toHaveLength(2);
  fireEvent.click(screen.getAllByRole('button',{name:'选择'})[1]);expect(choose).toHaveBeenCalledExactlyOnceWith(results[1]);
  expect(document.querySelector('.bangumi-result-copy strong')?.getAttribute('title')).toBe(results[0].title);
 });
 it('distinguishes the pending row, locks every action and retains current binding evidence',()=>{
  const choose=vi.fn();render(<I18nProvider><MatchingResults results={results} loading={false} searched error={false} bindingId={8} currentBindingId={7} provider="BANGUMI" onChoose={choose}/></I18nProvider>);
  const rows=document.querySelectorAll('.bangumi-result');expect(rows[0].getAttribute('aria-current')).toBe('true');expect(rows[1].classList.contains('is-binding')).toBe(true);expect(rows[0].classList.contains('is-disabled')).toBe(true);
  for(const button of screen.getAllByRole('button')){expect((button as HTMLButtonElement).disabled).toBe(true);fireEvent.click(button);}
  expect(choose).not.toHaveBeenCalled();expect(document.querySelector('.bangumi-results')?.getAttribute('aria-busy')).toBe('true');
 });
 it('uses the same action for a TMDb result without discarding its source snapshot',()=>{
  const choose=vi.fn(),movie={id:7,title:'Movie',originalTitle:'Movie',releaseDate:'2020-01-01',overview:'',posterPath:null};
  const result:MatchResult={provider:'TMDB',id:7,title:movie.title,originalTitle:movie.originalTitle,date:movie.releaseDate,poster:null,snapshot:'real-source-snapshot',movie};
  render(<I18nProvider><MatchingResults results={[result]} loading={false} searched error={false} bindingId={null} currentBindingId={7} provider="TMDB" onChoose={choose}/></I18nProvider>);
  fireEvent.click(screen.getByRole('button',{name:'选择'}));expect(choose).toHaveBeenCalledExactlyOnceWith(result);expect(screen.getByText(/TMDb #7/)).toBeTruthy();
 });
});
