import { describe, expect, it } from "vitest";
import { shouldDeferInitialNoteLoad, readAppRoute, windowOptionsForMode } from "./window";

describe("window routing", () => {
  it("defaults to the note window route", () => {
    window.history.pushState({}, "", "/");

    expect(readAppRoute()).toEqual({ mode: "note", noteId: null });
  });

  it("only defers note creation for the background main window", () => {
    expect(shouldDeferInitialNoteLoad({ launchedInBackground: true, windowLabel: "main", noteId: null })).toBe(true);
    expect(shouldDeferInitialNoteLoad({ launchedInBackground: true, windowLabel: "note-abc", noteId: null })).toBe(false);
    expect(shouldDeferInitialNoteLoad({ launchedInBackground: false, windowLabel: "main", noteId: null })).toBe(false);
    expect(shouldDeferInitialNoteLoad({ launchedInBackground: true, windowLabel: "main", noteId: "note-id" })).toBe(false);
  });

  it("uses compact resizable window sizes for list and note routes", () => {
    expect(windowOptionsForMode("list")).toMatchObject({
      width: 360,
      height: 520,
      minWidth: 320,
      minHeight: 300,
      resizable: true,
      decorations: false,
    });

    expect(windowOptionsForMode("note")).toMatchObject({
      width: 380,
      height: 500,
      minWidth: 320,
      minHeight: 300,
      resizable: true,
      decorations: false,
    });
  });
});
