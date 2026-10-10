// @vitest-environment jsdom
import {afterEach,expect,it,vi} from 'vitest';
import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {TmdbDiagnostics} from './TmdbDiagnostics';
import {I18nProvider} from '../lib/i18n';
const calls=vi.hoisted(()=>({status:vi.fn(),records:vi.fn()}));
vi.mock('../lib/api',()=>({desktopAvailable:true,api:{tmdbStatus:calls.status,tmdbMatchDiagnostics:calls.records}}));
afterEach(()=>{cleanup();vi.resetAllMocks();});
it('loads diagnostics only when expanded, separates failures and supports refresh',async()=>{
 calls.status.mockResolvedValue(true);calls.records.mockResolvedValue(['credentials-unavailable','unauthorized','rate-limited','request-failed','no-results','year-uncertain'].map((outcome,i)=>({nodeId:i,query:'Movie '+i,year:2010,outcome,updatedAt:'2026-01-01'})));
 render(<I18nProvider><TmdbDiagnostics/></I18nProvider>);expect(calls.status).not.toHaveBeenCalled();
 fireEvent.click(document.querySelector('summary')!);
 await waitFor(()=>expect(calls.records).toHaveBeenCalledOnce());await screen.findByText('未找到结果');
 expect(screen.getByText('年份不确定，待人工确认')).toBeTruthy();expect(screen.getByText(/凭据无效/)).toBeTruthy();expect(screen.getByText(/过于频繁/)).toBeTruthy();expect(screen.getByText('未配置')).toBeTruthy();
 fireEvent.click(screen.getByRole('button',{name:'刷新记录'}));await waitFor(()=>expect(calls.records).toHaveBeenCalledTimes(2));
});
