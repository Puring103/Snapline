import { WebviewWindow } from "@tauri-apps/api/webviewWindow";

export type AppWindowMode = "list" | "note";

const NOTE_WINDOW_OPTIONS = {
  width: 380,
  height: 500,
  minWidth: 320,
  minHeight: 300,
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
  return WebviewWindow.getByLabel("list").then((existing) => {
    if (existing) {
      void existing.show();
      void existing.unminimize();
      void existing.setFocus();
      return existing;
    }

    return openAppWindow("list");
  });
}

export function openNoteWindow(noteId?: string | null) {
  return openAppWindow("note", noteId ?? null);
}

export function windowOptionsForMode(mode: AppWindowMode) {
  return mode === "list" ? LIST_WINDOW_OPTIONS : NOTE_WINDOW_OPTIONS;
}

function openAppWindow(mode: AppWindowMode, noteId: string | null = null) {
  const label = buildWindowLabel(mode, noteId);
  const params = new URLSearchParams({ mode });
  if (noteId) params.set("noteId", noteId);
  const options = windowOptionsForMode(mode);

  return new WebviewWindow(label, {
    url: `/?${params.toString()}`,
    title: mode === "list" ? "Snapline" : "Snapline Note",
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
