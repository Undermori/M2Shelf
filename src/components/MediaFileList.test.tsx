// @vitest-environment jsdom
import {cleanup,fireEvent,render,screen} from "@testing-library/react";
import {afterEach,expect,it,vi} from "vitest";
import {MediaFileList} from "./MediaFileList";
import {I18nProvider} from "../lib/i18n";
import type {MediaFile} from "../types/media";
afterEach(cleanup);
const file=(id:number,path:string):MediaFile=>({id,nodeId:id,fileName:"01.mkv",absolutePath:path,extension:".mkv",fileSize:100,modifiedAt:"2026-01-01",lastSeenAt:"2026-01-01",durationMs:null,width:null,height:null,codec:null});
it("sorts by relative directory before source label and keeps same-name actions distinct",()=>{
 const files=[file(10,"X:/Fixture/Release A/CD2/01.mkv"),file(20,"X:/Fixture/Release Z/CD1/01.mkv")];
 const onPlay=vi.fn(),onReveal=vi.fn();
 render(<I18nProvider><MediaFileList files={files} relativePaths={{10:"Release A / CD2",20:"Release Z / CD1"}} directorySortKeys={{10:"CD2",20:"CD1"}} onPlay={onPlay} onReveal={onReveal}/></I18nProvider>);
 const rows=document.querySelectorAll(".media-file-row");
 expect(rows[0].textContent).toContain("Release Z / CD1");
 fireEvent.doubleClick(rows[0]);expect(onPlay).toHaveBeenCalledWith(files[1]);
 fireEvent.click(screen.getByRole("button",{name:/在资源管理器中显示 Release A.*01.mkv/}).closest(".media-file-row")!.querySelectorAll("button")[1]);
 expect(onReveal).toHaveBeenCalledWith(files[0]);
 expect(rows[0].getAttribute("title")).toContain(files[1].absolutePath);
});
