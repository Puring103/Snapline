import { Suspense, lazy, useEffect, useMemo } from "react";
import { startupLog } from "./platform/startupLog";
import { readAppRoute } from "./platform/window";
import { useThemeSync } from "./hooks/theme";
import { NotesListWindow } from "./components/app/NotesListWindow";
import { NoteEditorWindow } from "./components/app/NoteEditorWindow";

const AndroidApp = lazy(() => import("./mobile/AndroidApp").then((module) => ({ default: module.AndroidApp })));

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
    preloadEditorChunks();
  }, []);

  useEffect(() => {
    const url = new URL(window.location.href);
    if (!url.searchParams.has("mode")) {
      url.searchParams.set("mode", route.mode);
      window.history.replaceState({}, "", `${url.pathname}${url.search}`);
    }
  }, [route.mode]);

  if (route.mode === "android") {
    return (
      <Suspense fallback={null}>
        <AndroidApp />
      </Suspense>
    );
  }

  return route.mode === "list" ? <NotesListWindow /> : <NoteEditorWindow newDraft={route.newDraft} noteId={route.noteId} />;
}

function preloadEditorChunks() {
  const preload = () => {
    void Promise.all([
      import("./components/EditorPane"),
      import("./components/MarkdownPreview"),
    ]).catch(() => undefined);
  };

  if ("requestIdleCallback" in window) {
    window.requestIdleCallback(preload, { timeout: 1200 });
    return;
  }

  globalThis.setTimeout(preload, 0);
}
