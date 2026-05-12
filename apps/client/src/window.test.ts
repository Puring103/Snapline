import { beforeEach, describe, expect, it, vi } from "vitest";

const webviewState = vi.hoisted(() => ({
  all: [] as Array<{
    close?: () => Promise<void>;
    label: string;
    setFocus?: () => Promise<void>;
    setPosition?: () => Promise<void>;
    show?: () => Promise<void>;
    unminimize?: () => Promise<void>;
  }>,
  byLabel: new Map<string, unknown>(),
  constructed: [] as Array<{ label: string; options: { url?: string; visible?: boolean } }>,
}));

const invokeState = vi.hoisted(() => ({
  calls: [] as Array<{ command: string; args: unknown }>,
}));

const eventState = vi.hoisted(() => ({
  emits: [] as Array<{ event: string; payload: unknown }>,
}));

vi.mock("@tauri-apps/api/webviewWindow", () => {
  class MockWebviewWindow {
    label: string;
    options: { url?: string };

    constructor(label: string, options: { url?: string }) {
      this.label = label;
      this.options = options;
      webviewState.constructed.push({ label, options });
    }

    static getByLabel(label: string) {
      return Promise.resolve(webviewState.byLabel.get(label) ?? null);
    }

    static getAll() {
      return Promise.resolve(webviewState.all);
    }
  }

  return { WebviewWindow: MockWebviewWindow };
});

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args: unknown) => {
    invokeState.calls.push({ command, args });
    return Promise.resolve("note-saved-note-id-replacement");
  },
}));

vi.mock("@tauri-apps/api/event", () => ({
  emit: (event: string, payload?: unknown) => {
    eventState.emits.push({ event, payload });
    return Promise.resolve();
  },
}));

import {
  ensureSpareNoteWindow,
  forgetNoteWindow,
  openNoteWindow,
  rememberNoteWindow,
  readAppRoute,
  revealExistingWindow,
  shouldDeferInitialNoteLoad,
  shouldStartWindowDrag,
  windowOptionsForMode,
} from "./platform/window";

describe("window routing", () => {
  beforeEach(() => {
    webviewState.all = [];
    webviewState.byLabel.clear();
    webviewState.constructed = [];
    invokeState.calls = [];
    eventState.emits = [];
    localStorage.clear();
  });

  it("defaults to the note window route", () => {
    window.history.pushState({}, "", "/");

    expect(readAppRoute()).toEqual({ mode: "note", noteId: null, newDraft: false, spare: false });
  });

  it("reads explicit new draft note routes separately from startup restore routes", () => {
    window.history.pushState({}, "", "/?mode=note&newDraft=1");

    expect(readAppRoute()).toEqual({ mode: "note", noteId: null, newDraft: true, spare: false });
  });

  it("keeps saved note routes from being treated as new drafts", () => {
    window.history.pushState({}, "", "/?mode=note&noteId=note-id");

    expect(readAppRoute()).toEqual({ mode: "note", noteId: "note-id", newDraft: false, spare: false });
  });

  it("reads spare note routes separately from explicit drafts", () => {
    window.history.pushState({}, "", "/?mode=note&spare=1");

    expect(readAppRoute()).toEqual({ mode: "note", noteId: null, newDraft: false, spare: true });
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
      setPosition: async () => {
        calls.push("setPosition");
      },
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

    await revealExistingWindow(existing, { x: 100, y: 200 });

    expect(calls).toEqual(["setPosition", "show", "unminimize", "setFocus"]);
  });

  it("asks the backend to create an explicit new draft note window", async () => {
    await openNoteWindow(null, { x: 100, y: 200 });

    expect(invokeState.calls).toEqual([
      { command: "open_note_window", args: { noteId: null, position: { x: 100, y: 200 } } },
    ]);
  });

  it("asks the backend to open saved note windows by note id when no local window is known", async () => {
    await openNoteWindow("saved-note-id");

    expect(invokeState.calls).toEqual([
      { command: "open_note_window", args: { noteId: "saved-note-id", position: undefined } },
    ]);
    expect(eventState.emits).toEqual([
      {
        event: "snapline-close-note-windows",
        payload: { id: "saved-note-id", exceptLabel: "note-saved-note-id-replacement" },
      },
    ]);
    expect(localStorage.getItem("snapline.noteWindow.saved-note-id")).toBe("note-saved-note-id-replacement");
  });

  it("creates a hidden spare note window without arming a new draft", async () => {
    const label = await ensureSpareNoteWindow();

    expect(label).toMatch(/^note-spare-/);
    expect(webviewState.constructed).toHaveLength(1);
    expect(webviewState.constructed[0]).toMatchObject({
      options: {
        url: "/?mode=note&spare=1",
        visible: false,
      },
    });
    expect(webviewState.constructed[0].label).toMatch(/^note-spare-/);
  });

  it("uses a spare note window before falling back to backend creation", async () => {
    const calls: string[] = [];
    const spare = {
      label: "note-spare-existing",
      setPosition: async () => {
        calls.push("setPosition");
      },
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
    webviewState.all = [spare];

    const label = await openNoteWindow("saved-note-id", { x: 100, y: 200 });

    expect(label).toBe("note-spare-existing");
    expect(invokeState.calls).toEqual([]);
    expect(eventState.emits).toEqual([
      { event: "snapline-prepare-note-window", payload: { label: "note-spare-existing", noteId: "saved-note-id" } },
      { event: "snapline-close-note-windows", payload: { id: "saved-note-id", exceptLabel: "note-spare-existing" } },
    ]);
    expect(calls).toEqual(["setPosition", "show", "unminimize", "setFocus"]);
    expect(localStorage.getItem("snapline.noteWindow.saved-note-id")).toBe("note-spare-existing");
  });

  it("opens a replacement saved note window before closing remembered old windows", async () => {
    const calls: string[] = [];
    const existing = {
      label: "note-draft-window",
      close: async () => {
        calls.push("closeExisting");
      },
    };
    const stableExisting = {
      label: "note-saved-note-id",
      close: async () => {
        calls.push("closeStable");
      },
    };
    rememberNoteWindow("saved-note-id", existing.label);
    webviewState.byLabel.set(existing.label, existing);
    webviewState.byLabel.set(stableExisting.label, stableExisting);

    await openNoteWindow("saved-note-id", { x: 100, y: 200 });

    expect(invokeState.calls).toEqual([
      { command: "open_note_window", args: { noteId: "saved-note-id", position: { x: 100, y: 200 } } },
    ]);
    expect(eventState.emits).toEqual([
      {
        event: "snapline-close-note-windows",
        payload: { id: "saved-note-id", exceptLabel: "note-saved-note-id-replacement" },
      },
    ]);
    expect(calls).toEqual(["closeExisting", "closeStable"]);
    expect(localStorage.getItem("snapline.noteWindow.saved-note-id")).toBe("note-saved-note-id-replacement");
  });

  it("remembers and clears the window label for a saved note", () => {
    rememberNoteWindow("note-id", "note-draft-window");

    expect(localStorage.getItem("snapline.noteWindow.note-id")).toBe("note-draft-window");

    forgetNoteWindow("note-id");

    expect(localStorage.getItem("snapline.noteWindow.note-id")).toBeNull();
  });
});
