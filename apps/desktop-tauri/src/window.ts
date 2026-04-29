import { WebviewWindow } from "@tauri-apps/api/webviewWindow";

export type AppWindowMode = "list" | "note";

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

export function openListWindow() {
  return openAppWindow("list");
}

export function openNoteWindow(noteId?: string | null) {
  return openAppWindow("note", noteId ?? null);
}

function openAppWindow(mode: AppWindowMode, noteId: string | null = null) {
  const label = buildWindowLabel(mode, noteId);
  const url = new URL(window.location.href);
  url.searchParams.set("mode", mode);

  if (noteId) {
    url.searchParams.set("noteId", noteId);
  } else {
    url.searchParams.delete("noteId");
  }

  return new WebviewWindow(label, {
    url: url.toString(),
    title: mode === "list" ? "Snapline" : "Snapline Note",
    width: mode === "list" ? 360 : 420,
    height: mode === "list" ? 520 : 560,
    minWidth: mode === "list" ? 320 : 360,
    minHeight: 300,
    resizable: true,
  });
}

function buildWindowLabel(mode: AppWindowMode, noteId: string | null) {
  const suffix = crypto.randomUUID().replace(/-/g, "");
  return noteId ? `note-${noteId}-${suffix}` : `${mode}-${suffix}`;
}
