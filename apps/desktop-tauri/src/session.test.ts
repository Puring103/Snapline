import { describe, expect, it } from "vitest";
import { createDraftSession, isSessionDirty, matchesShortcut, sortNotes, upsertNote } from "./session";

describe("session helpers", () => {
  it("treats a blank draft as clean", () => {
    const session = createDraftSession();

    expect(isSessionDirty(session)).toBe(false);
  });

  it("detects draft edits", () => {
    const session = createDraftSession();

    expect(isSessionDirty({ ...session, title: "Changed" })).toBe(true);
    expect(isSessionDirty({ ...session, bodyMd: "Body" })).toBe(true);
  });

  it("sorts pinned notes first and newest first within each group", () => {
    const notes = sortNotes([
      { id: "c", title: "C", preview: "c", updated_at: "2024-01-03T00:00:00Z", pinned: false },
      { id: "b", title: "B", preview: "b", updated_at: "2024-01-02T00:00:00Z", pinned: true },
      { id: "a", title: "A", preview: "a", updated_at: "2024-01-01T00:00:00Z", pinned: true },
    ]);

    expect(notes.map((note) => note.id)).toEqual(["b", "a", "c"]);
  });

  it("upserts a note without duplicating it", () => {
    const notes = upsertNote(
      [
        { id: "a", title: "Old", preview: "old", updated_at: "2024-01-01T00:00:00Z", pinned: false },
        { id: "b", title: "Pinned", preview: "pinned", updated_at: "2024-01-02T00:00:00Z", pinned: true },
      ],
      { id: "a", title: "New", content_md: "# New\nBody", updated_at: "2024-01-03T00:00:00Z", pinned: true } as never,
    );

    expect(notes.map((note) => note.id)).toEqual(["a", "b"]);
    expect(notes[0].title).toBe("New");
    expect(notes[0].pinned).toBe(true);
    expect(notes[0].preview).toBe("Body");
  });

  it("matches configurable shortcuts", () => {
    expect(
      matchesShortcut(
        {
          altKey: false,
          ctrlKey: true,
          metaKey: false,
          shiftKey: true,
          key: " ",
          preventDefault: () => undefined,
        } as KeyboardEvent,
        "Ctrl+Shift+Space",
      ),
    ).toBe(true);

    expect(
      matchesShortcut(
        {
          altKey: false,
          ctrlKey: true,
          metaKey: false,
          shiftKey: false,
          key: "Space",
          preventDefault: () => undefined,
        } as KeyboardEvent,
        "Ctrl+Shift+Space",
      ),
    ).toBe(false);
  });
});
