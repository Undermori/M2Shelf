// @vitest-environment jsdom
import {act,cleanup,fireEvent,render,screen} from "@testing-library/react";
import {afterEach,beforeEach,expect,it,vi} from "vitest";
import {PosterCacheProgress} from "./PosterCacheProgress";
import {I18nProvider} from "../lib/i18n";

const mocks=vi.hoisted(()=>({status:vi.fn(),failures:vi.fn(),retry:vi.fn()}));
vi.mock("../lib/api",()=>({desktopAvailable:true,api:{posterCacheStatus:mocks.status,posterCacheFailures:mocks.failures,retryPosterCache:mocks.retry}}));
beforeEach(()=>{
  vi.useFakeTimers();
  mocks.status.mockReset().mockResolvedValue({phase:"COMPLETED",processed:5,total:5,failed:1,deferred:0,error:null});
  mocks.failures.mockReset().mockResolvedValue([{nodeId:1,name:"Example cover",reason:"SOURCE_READ",detail:"Access denied"}]);
  mocks.retry.mockReset().mockResolvedValue(undefined);
});
afterEach(()=>{cleanup();vi.useRealTimers();});
const show=(directory="X:/Cache")=>render(<I18nProvider><PosterCacheProgress cacheDirectory={directory} onCompleted={()=>{}}/></I18nProvider>);
const flush=()=>act(async()=>{await Promise.resolve();});

it("shows saved failures and retries without hiding their explanation",async()=>{
  show();await flush();
  fireEvent.click(screen.getByRole("button",{name:"查看原因"}));await flush();
  expect(screen.getByText("Access denied")).toBeTruthy();
  expect(screen.getByText("无法读取来源")).toBeTruthy();
  mocks.status.mockResolvedValue({phase:"QUEUED",processed:0,total:0,failed:0,deferred:0,error:null});
  fireEvent.click(screen.getByRole("button",{name:"重试未完成项"}));await flush();
  expect(mocks.retry).toHaveBeenCalledOnce();
  expect(screen.getByText("等待后台生成")).toBeTruthy();
});

it("discards late diagnostics when the cache changes and stops polling after leaving",async()=>{
  let resolve:(value:unknown)=>void=()=>{};
  mocks.failures.mockReturnValue(new Promise(done=>{resolve=done;}));
  const view=show();await flush();
  fireEvent.click(screen.getByRole("button",{name:"查看原因"}));
  view.rerender(<I18nProvider><PosterCacheProgress cacheDirectory="X:/NewCache" onCompleted={()=>{}}/></I18nProvider>);
  await act(async()=>{resolve([{nodeId:1,name:"Old source",reason:"SOURCE_READ",detail:"Old error"}]);});
  expect(screen.queryByText("Old error")).toBeNull();
  view.unmount();const calls=mocks.status.mock.calls.length;
  await act(async()=>{vi.advanceTimersByTime(3000);});
  expect(mocks.status).toHaveBeenCalledTimes(calls);
});

it("explains capacity deferrals separately and disables retry during a running pass",async()=>{
  mocks.status.mockResolvedValue({phase:"RUNNING",processed:5,total:10,failed:1,deferred:2,error:null});
  show();await flush();
  expect(screen.getByText(/2 项因缓存容量限制暂缓/)).toBeTruthy();
  expect((screen.getByRole("button",{name:"重试未完成项"}) as HTMLButtonElement).disabled).toBe(true);
});

it("does not restore a retry status from a previous cache location",async()=>{
  const view=show();await flush();
  let resolve:(value:unknown)=>void=()=>{};
  mocks.status.mockReturnValueOnce(new Promise(done=>{resolve=done;}));
  fireEvent.click(screen.getByRole("button",{name:"重试未完成项"}));await flush();
  mocks.status.mockResolvedValue({phase:"COMPLETED",processed:10,total:10,failed:0,deferred:0,error:null});
  view.rerender(<I18nProvider><PosterCacheProgress cacheDirectory="X:/NewCache" onCompleted={()=>{}}/></I18nProvider>);
  await flush();
  await act(async()=>{resolve({phase:"FAILED",processed:1,total:5,failed:1,deferred:0,error:"old"});});
  expect(screen.queryByRole("button",{name:"重试未完成项"})).toBeNull();
  expect(screen.getByText("已处理 10 / 10 个封面")).toBeTruthy();
});
