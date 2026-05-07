import { describe, expect, it } from "vitest";
import { forgetNoteWindow, rememberNoteWindow, readAppRoute, revealExistingWindow, shouldDeferInitialNoteLoad, shouldStartWindowDrag, windowOptionsForMode } from "./window";

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
      width: 340,
      height: 440,
      minWidth: 300,
      minHeight: 260,
      resizable: true,
      decorations: false,
    });
  });

  it("allows window dragging from header surfaces but not controls", () => {
    document.body.innerHTML = `
      <header>
        <div id="surface"><span id="title">Snapline</span></div>
        <button id="button">Close</button>
        <input id="input" />
        <div class="chromeMenu"><button id="menu-button">Notes</button></div>
      </header>
    `;

    expect(shouldStartWindowDrag(document.getElementById("surface"))).toBe(true);
    expect(shouldStartWindowDrag(document.getElementById("title"))).toBe(true);
    expect(shouldStartWindowDrag(document.getElementById("button"))).toBe(false);
    expect(shouldStartWindowDrag(document.getElementById("input"))).toBe(false);
    expect(shouldStartWindowDrag(document.getElementById("menu-button"))).toBe(false);
  });

  it("reveals an existing list window before focusing it", async () => {
    const calls: string[] = [];
    const existing = {
      show: async () => {
        calls.push("show");
      },
      unminimize: async () => {
        calls.push("unminimize");
      },
      setFocus: async () => {
        calls.push("setFocus");
      },
    };

    await revealExistingWindow(existing);

    expect(calls).toEqual(["show", "unminimize", "setFocus"]);
  });

  it("remembers and clears the window label for a saved note", () => {
    rememberNoteWindow("note-id", "note-draft-window");

    expect(localStorage.getItem("snapline.noteWindow.note-id")).toBe("note-draft-window");

    forgetNoteWindow("note-id");

    expect(localStorage.getItem("snapline.noteWindow.note-id")).toBeNull();
  });
});
