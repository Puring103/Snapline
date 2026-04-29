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

function openAppWindow(mode: AppWindowMode, noteId: string | null = null) {
  const label = buildWindowLabel(mode, noteId);
  const params = new URLSearchParams({ mode });
  if (noteId) params.set("noteId", noteId);

  return new WebviewWindow(label, {
    url: `/?${params.toString()}`,
    title: mode === "list" ? "Snapline" : "Snapline Note",
    width: mode === "list" ? 360 : 420,
    height: mode === "list" ? 520 : 560,
    minWidth: mode === "list" ? 320 : 360,
    minHeight: 300,
    resizable: true,
  });
}

function buildWindowLabel(mode: AppWindowMode, noteId: string | null) {
  if (mode === "list") {
    return "list";
  }

  const suffix = crypto.randomUUID().replace(/-/g, "");
  return noteId ? `note-${noteId}-${suffix}` : `${mode}-${suffix}`;
}
