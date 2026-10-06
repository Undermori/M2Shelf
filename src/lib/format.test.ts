import { describe, expect, it } from "vitest";
import { compareFileModifiedTimes, compareMediaNodes, formatRelativeTime, nodeMatchesQuery } from "./format";
import type { MediaNode } from "../types/media";

describe("source file update ordering", () => {
  it("compares instants across timezone offsets and puts unavailable dates last in both directions", () => {
    const dates = ["2026-01-01T12:00:00+08:00", "2026-01-01T05:00:00Z", null, "invalid"];
    expect([...dates].sort((a,b) => compareFileModifiedTimes(a,b,"modified-desc"))).toEqual([dates[1], dates[0], null, "invalid"]);
    expect([...dates].sort((a,b) => compareFileModifiedTimes(a,b,"modified-asc"))).toEqual([dates[0], dates[1], null, "invalid"]);
  });
});

it("formats elapsed time in each locale while tolerating future and invalid timestamps", () => {
  const now = Date.parse("2026-09-12T12:00:00Z");
  expect(formatRelativeTime("2026-09-09T12:00:00Z", now, "zh-CN")).toBe("3天前");
  expect(formatRelativeTime("2026-09-12T11:55:00Z", now, "en-US")).toBe("5 minutes ago");
  expect(formatRelativeTime("2026-09-12T10:00:00Z", now, "ja-JP")).toBe("2 時間前");
  expect(formatRelativeTime("2026-09-09T12:00:00Z", now, "ko-KR")).toBe("3일 전");
  expect(formatRelativeTime("2026-09-15T12:00:00Z", now, "en-US")).toBe("in 3 days");
  expect(formatRelativeTime("2026-09-12T11:59:59Z", now, "zh-CN")).toBe("现在");
  expect(formatRelativeTime("invalid", now)).toBe("invalid");
});

it("orders by actual watched timestamps and keeps unwatched works last in either direction", () => {
  const nodes = [
    { id: 1, displayName: "A", lastWatchedAt: null },
    { id: 2, displayName: "B", lastWatchedAt: "2026-01-01T12:00:00+08:00" },
    { id: 3, displayName: "C", lastWatchedAt: "2026-01-01T05:00:00Z" },
  ] as MediaNode[];
  expect([...nodes].sort((a,b) => compareMediaNodes(a,b,"watched-desc","zh-CN")).map(n => n.id)).toEqual([3,2,1]);
  expect([...nodes].sort((a,b) => compareMediaNodes(a,b,"watched-asc","zh-CN")).map(n => n.id)).toEqual([2,3,1]);
});

it("matches official aliases and multilingual titles independently of the displayed language", () => {
  const node = { displayName: "Folder", folderName: "Folder", userTags: [], binding: { providerTitle: "フリップフラッパーズ", providerTitleCn: "轻拍翻转小魔女", providerAliases: ["Flip Flappers", "フリフラ"] } } as unknown as MediaNode;
  for (const query of ["轻拍", "flip", "ＦＬＩＰ", "フリフラ"]) expect(nodeMatchesQuery(node, query, "en-US")).toBe(true);
  expect(nodeMatchesQuery(node, "missing", "zh-CN")).toBe(false);
});
