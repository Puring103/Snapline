import { LogicalPosition } from "@tauri-apps/api/dpi";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";

export type AppWindowMode = "list" | "note";

const NOTE_WINDOW_OPTIONS = {
  width: 340,
  height: 440,
  minWidth: 300,
  minHeight: 260,
  resizable: true,
  decorations: false,
} as const;

const LIST_WINDOW_OPTIONS = {
  width: 360,
  height: 520,
  minWidth: 320,
  minHeight: 300,
  resizable: true,
  decorations: false,
} as const;

export interface AppRoute {
  mode: AppWindowMode;
  noteId: string | null;
}

export function readAppRoute(): AppRoute {
  const search = new URLSearchParams(window.location.search);
  const mode = search.get("mode") === "list" ? "list" : "note";
  return {
    mode,
    noteId: search.get("noteId"),
  };
}

export function shouldDeferInitialNoteLoad({
  launchedInBackground,
  windowLabel,
  noteId,
}: {
  launchedInBackground: boolean;
  windowLabel: string;
  noteId: string | null;
}): boolean {
  return launchedInBackground && windowLabel === "main" && noteId === null;
}

export function openListWindow() {
  return WebviewWindow.getByLabel("list").then(async (existing) => {
    if (existing) {
      await revealExistingWindow(existing);
      return existing;
    }

    return openAppWindow("list");
  });
}

export interface PointerWindowPosition {
  x: number;
  y: number;
}

export function openNoteWindow(noteId?: string | null, position?: PointerWindowPosition) {
  return openAppWindow("note", noteId ?? null, position);
}

export function windowOptionsForMode(mode: AppWindowMode) {
  return mode === "list" ? LIST_WINDOW_OPTIONS : NOTE_WINDOW_OPTIONS;
}

export function shouldStartWindowDrag(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) {
    return false;
  }

  return target.closest("button, input, textarea, select, .chromeMenu") === null;
}

export interface RevealableWindow {
  show: () => Promise<void>;
  unminimize: () => Promise<void>;
  setFocus: () => Promise<void>;
}

export async function revealExistingWindow(window: RevealableWindow) {
  await window.show();
  await window.unminimize();
  await window.setFocus();
}

function openAppWindow(mode: AppWindowMode, noteId: string | null = null, position?: PointerWindowPosition) {
  const label = buildWindowLabel(mode, noteId);
  const params = new URLSearchParams({ mode });
  if (noteId) params.set("noteId", noteId);
  const options = windowOptionsForMode(mode);

  return new WebviewWindow(label, {
    url: `/?${params.toString()}`,
    title: mode === "list" ? "Snapline" : "Snapline Note",
    ...(position ? { position: new LogicalPosition(position.x + 12, position.y + 12) } : {}),
    ...options,
  });
}

function buildWindowLabel(mode: AppWindowMode, noteId: string | null) {
  if (mode === "list") {
    return "list";
  }

  const suffix = crypto.randomUUID().replace(/-/g, "");
  return noteId ? `note-${noteId}-${suffix}` : `${mode}-${suffix}`;
}
