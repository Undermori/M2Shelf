// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { I18nProvider } from "../lib/i18n";
import { BatchTagDialog } from "./BatchTagDialog";
import { FavoriteAssignmentDialog } from "./FavoriteAssignmentDialog";

vi.mock("../lib/api", () => ({ api: {
  listUserTags: vi.fn().mockResolvedValue([]),
  listFavoriteFolders: vi.fn().mockResolvedValue([]),
} }));
afterEach(cleanup);

it.each([
  ["batch tags", BatchTagDialog],
  ["favorite assignment", FavoriteAssignmentDialog],
] as const)("only the visible %s dialog handles Escape", async (_name, Dialog) => {
  const close = vi.fn();
  const view = (nodeIds: number[]) => <I18nProvider><Dialog nodeIds={nodeIds} onClose={close} onApplied={vi.fn()} /></I18nProvider>;
  const result = render(view([]));
  fireEvent.keyDown(window, { key: "Escape" });
  expect(close).not.toHaveBeenCalled();

  result.rerender(view([1]));
  fireEvent.keyDown(await screen.findByRole("textbox"), { key: "Escape" });
  expect(close).toHaveBeenCalledOnce();

  result.rerender(view([]));
  fireEvent.keyDown(window, { key: "Escape" });
  expect(close).toHaveBeenCalledOnce();
});
