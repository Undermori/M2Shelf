// @vitest-environment jsdom
import {afterEach,beforeEach,describe,expect,it,vi} from 'vitest';
import {act,cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {I18nProvider} from '../lib/i18n';
import {TmdbCredentialSettings} from './TmdbCredentialSettings';
import {PlayerSettings} from './PlayerSettings';
import {StrictMode} from 'react';
const mocks=vi.hoisted(()=>({status:vi.fn(),select:vi.fn(),configure:vi.fn(),verify:vi.fn(),remove:vi.fn(),open:vi.fn(),test:vi.fn(),choose:vi.fn(),flush:vi.fn(),change:vi.fn(),settings:{mpvPath:'X:/Player/mpv.exe',playerMode:'CUSTOM'} as {mpvPath:string|null;playerMode:'CUSTOM'|'SYSTEM'}}));
vi.mock('../lib/api',()=>({desktopAvailable:true,choosePlayerExecutable:mocks.choose,api:{tmdbCredentialStatus:mocks.status,tmdbSelectSource:mocks.select,tmdbConfigure:mocks.configure,tmdbVerify:mocks.verify,tmdbDeletePersonal:mocks.remove,openExternalUrl:mocks.open,testMpv:mocks.test}}));
vi.mock('../lib/settingsStore',()=>({useAppSettings:()=>({settings:mocks.settings,saving:false,flush:mocks.flush,change:mocks.change})}));
let status:{source:'DEFAULT'|'PERSONAL';defaultAvailable:boolean;personalConfigured:boolean;check:string};
beforeEach(()=>{vi.resetAllMocks();status={source:'DEFAULT',defaultAvailable:true,personalConfigured:false,check:'UNCHECKED'};mocks.settings={mpvPath:'X:/Player/mpv.exe',playerMode:'CUSTOM'};mocks.status.mockImplementation(async()=>({...status}));mocks.select.mockImplementation(async(source)=>{status.source=source;status.check='UNCHECKED';});mocks.verify.mockImplementation(async()=>{status.check='VERIFIED';});mocks.configure.mockImplementation(async()=>{status.personalConfigured=true;status.source='PERSONAL';status.check='UNCHECKED';});mocks.remove.mockImplementation(async()=>{status.personalConfigured=false;status.source='DEFAULT';status.check='UNCHECKED';});mocks.flush.mockResolvedValue(undefined);mocks.test.mockResolvedValue({ok:true,version:'mpv v0.41.0',message:''});});
afterEach(cleanup);
const mount=(element:React.ReactNode)=>render(<I18nProvider>{element}</I18nProvider>);
describe('TMDb settings feedback',()=>{
 it('ignores an obsolete initial read under StrictMode and preserves a cancelled credential dialog',async()=>{
  let finish!:(v:unknown)=>void;mocks.status.mockReturnValueOnce(new Promise(r=>{finish=r;}));status={...status,source:'PERSONAL',personalConfigured:true,check:'VERIFIED'};
  render(<StrictMode><I18nProvider><TmdbCredentialSettings/></I18nProvider></StrictMode>);await screen.findByText('连接验证成功');await act(async()=>finish({source:'DEFAULT',defaultAvailable:true,personalConfigured:false,check:'UNCHECKED'}));expect((screen.getAllByRole('radio')[1] as HTMLInputElement).checked).toBe(true);
  mocks.configure.mockResolvedValue(false);fireEvent.click(screen.getByRole('button',{name:'修改个人令牌'}));await waitFor(()=>expect(mocks.configure).toHaveBeenCalled());await screen.findByText('连接验证成功');expect((screen.getAllByRole('radio')[1] as HTMLInputElement).checked).toBe(true);
 });
 it('keeps personal recommendation, switches sources and verifies only on demand',async()=>{
  mount(<TmdbCredentialSettings/>);await waitFor(()=>expect((screen.getAllByRole('radio')[0] as HTMLInputElement).checked).toBe(true));
  expect(screen.getByText('使用默认 TMDb 服务')).toBeTruthy();expect(screen.getByText('使用个人 API Read Access Token（推荐）')).toBeTruthy();expect(mocks.verify).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button',{name:'配置 TMDB'}));await screen.findByText('已保存，尚未验证');
  fireEvent.click(screen.getByRole('button',{name:'检查连接'}));await screen.findByText('连接验证成功');expect(screen.getAllByText('连接验证成功')).toHaveLength(1);
  fireEvent.click(screen.getAllByRole('radio')[0]);await waitFor(()=>expect(mocks.select).toHaveBeenCalledWith('DEFAULT'));
  fireEvent.click(screen.getByRole('button',{name:'删除个人配置'}));await screen.findByRole('button',{name:'配置 TMDB'});expect(mocks.remove).toHaveBeenCalledOnce();
 });
 it('keeps authentication/network failures near the selected option and allows another check',async()=>{
  status={...status,source:'PERSONAL',personalConfigured:true,check:'VERIFIED'};mount(<TmdbCredentialSettings/>);await screen.findByText('连接验证成功');
  mocks.verify.mockImplementation(async()=>{status.check='NETWORK_ERROR';});fireEvent.click(screen.getByRole('button',{name:'检查连接'}));await screen.findByText('连接失败，配置已保留');expect(screen.queryByText('连接验证成功')).toBeNull();
  mocks.verify.mockImplementation(async()=>{status.check='AUTH_ERROR';});fireEvent.click(screen.getByRole('button',{name:'检查连接'}));await screen.findByText('凭证无效或无权限，请切换服务或修改个人令牌');
  mocks.verify.mockImplementation(async()=>{status.check='VERIFIED';});fireEvent.click(screen.getByRole('button',{name:'检查连接'}));await screen.findByText('连接验证成功');
 });
 it('retries a failed initial status read and keeps required credits accessible',async()=>{
  mocks.status.mockRejectedValueOnce(new Error('private diagnostic'));mount(<TmdbCredentialSettings/>);await screen.findByRole('alert');expect(screen.queryByText('private diagnostic')).toBeNull();fireEvent.click(screen.getByRole('button',{name:'重试'}));await screen.findByText('无需配置，即可检索电影');
  expect(screen.getByText('服务说明与署名')).toBeTruthy();expect(screen.getByAltText('TMDB')).toBeTruthy();expect(screen.getByText(/not endorsed or certified/)).toBeTruthy();
 });
});
describe('explicit player testing',()=>{
 it('offers only custom/system, preserves the path and never probes on entry',async()=>{
  mount(<PlayerSettings/>);expect(screen.getAllByRole('radio')).toHaveLength(2);expect(screen.queryByText('自动检测 mpv')).toBeNull();expect((screen.getByRole('textbox') as HTMLInputElement).value).toBe('X:/Player/mpv.exe');expect(mocks.test).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('radio',{name:'系统默认播放器'}));expect(mocks.change).toHaveBeenCalledOnce();expect(mocks.change.mock.calls[0][0](mocks.settings)).toEqual({...mocks.settings,playerMode:'SYSTEM'});
  fireEvent.click(screen.getByRole('button',{name:'测试播放器'}));await screen.findByText('播放器可用：mpv v0.41.0');expect(mocks.flush).toHaveBeenCalledOnce();expect(mocks.test).toHaveBeenCalledOnce();
 });
 it('does not mistake config errors for versions and reveals raw output only inside an explicit log',async()=>{
  mocks.test.mockResolvedValue({ok:false,version:null,message:'PLAYER_CONFIG_ERROR\nError parsing option screenshot-flags (option not found)'});mount(<PlayerSettings/>);expect(screen.queryByText(/screenshot-flags/)).toBeNull();fireEvent.click(screen.getByRole('button',{name:'测试播放器'}));await screen.findByRole('alert');expect(screen.getByRole('alert').textContent).not.toContain('screenshot-flags');const log=screen.getByText('查看测试日志').closest('details')!;expect(log.open).toBe(false);fireEvent.click(screen.getByText('查看测试日志'));expect(screen.getByText(/Error parsing option/)).toBeTruthy();
 });
 it('rejects a late test result after the selected mode changes',async()=>{
  let finish!:(v:unknown)=>void;mocks.test.mockReturnValue(new Promise(r=>{finish=r;}));const view=mount(<PlayerSettings/>);fireEvent.click(screen.getByRole('button',{name:'测试播放器'}));await waitFor(()=>expect(mocks.test).toHaveBeenCalled());mocks.settings={...mocks.settings,playerMode:'SYSTEM'};view.rerender(<I18nProvider><PlayerSettings/></I18nProvider>);await act(async()=>finish({ok:false,message:'OLD_CONFIGURATION',version:null}));expect(screen.queryByRole('alert')).toBeNull();
  mocks.test.mockResolvedValue({ok:true,message:'SYSTEM',version:'SYSTEM'});fireEvent.click(screen.getByRole('button',{name:'测试播放器'}));await waitFor(()=>expect(screen.getByRole('status').textContent).toBe('系统默认播放器'));
 });
});
