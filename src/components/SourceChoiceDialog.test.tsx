// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { I18nProvider } from "../lib/i18n";
import type { MediaNode } from "../types/media";
import { SourceChoiceDialog } from "./SourceChoiceDialog";

afterEach(cleanup);

it("preserves focus through parent rerenders, traps Tab and uses the latest close handler", () => {
  const trigger = document.createElement("button");
  document.body.append(trigger);
  trigger.focus();
  const sources = [1, 2].map(id => ({ id, folderName: `Source ${id}`, absolutePath: `X:/Fixtures/${id}` } as MediaNode));
  const firstClose = vi.fn(), latestClose = vi.fn(), choose = vi.fn();
  const view = (onClose: () => void) => <I18nProvider><SourceChoiceDialog sources={sources} onClose={onClose} onChoose={choose} /></I18nProvider>;
  const result = render(view(firstClose));
  const first = screen.getByRole("button", { name: /Source 1/ });
  const second = screen.getByRole("button", { name: /Source 2/ });
  const close = screen.getByRole("button", { name: "关闭" });
  expect(document.activeElement).toBe(first);
  second.focus();
  result.rerender(view(latestClose));
  expect(document.activeElement).toBe(second);
  expect(screen.getByRole("dialog", { name: "选择资源来源" })).toBeTruthy();
  first.focus();
  fireEvent.keyDown(window, { key: "Tab", shiftKey: true });
  expect(document.activeElement).toBe(close);
  fireEvent.keyDown(window, { key: "Tab" });
  expect(document.activeElement).toBe(first);
  fireEvent.click(second);
  expect(choose).toHaveBeenCalledWith(sources[1]);
  fireEvent.keyDown(window, { key: "Escape" });
  expect(latestClose).toHaveBeenCalledOnce();
  expect(firstClose).not.toHaveBeenCalled();
  result.unmount();
  expect(document.activeElement).toBe(trigger);
  trigger.remove();
});
