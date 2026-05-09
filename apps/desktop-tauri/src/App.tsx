import { useEffect, useMemo } from "react";
import { startupLog } from "./platform/startupLog";
import { readAppRoute } from "./platform/window";
import { useThemeSync } from "./hooks/theme";
import { NotesListWindow } from "./components/app/NotesListWindow";
import { NoteEditorWindow } from "./components/app/NoteEditorWindow";

export function App() {
  const route = useMemo(readAppRoute, []);
  useThemeSync();

  useEffect(() => {
    startupLog("route_mounted", {
      mode: route.mode,
      has_note_id: route.noteId !== null,
      new_draft: route.newDraft,
    });
  }, [route.mode, route.noteId, route.newDraft]);

  useEffect(() => {
    const url = new URL(window.location.href);
    if (!url.searchParams.has("mode")) {
      url.searchParams.set("mode", "note");
      window.history.replaceState({}, "", `${url.pathname}${url.search}`);
    }
  }, []);

  return route.mode === "list" ? <NotesListWindow /> : <NoteEditorWindow newDraft={route.newDraft} noteId={route.noteId} />;
}
