import { describe, expect, it } from "vitest";
import { shouldDeferInitialNoteLoad, readAppRoute } from "./window";

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
});
