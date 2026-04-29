import { convertFileSrc } from "@tauri-apps/api/core";
import { appDataDir, join } from "@tauri-apps/api/path";
import { useEffect, useRef, useState } from "react";
import { api } from "./api";
import { EditorPane } from "./EditorPane";
import type { Note, NoteSummary } from "./types";

export function App() {
  const [notes, setNotes] = useState<NoteSummary[]>([]);
  const [current, setCurrent] = useState<Note | null>(null);
  const [appDir, setAppDir] = useState<string | null>(null);
  const [status, setStatus] = useState("Loading");
  const [error, setError] = useState<string | null>(null);
  const currentIdRef = useRef<string | null>(null);
  currentIdRef.current = current?.id ?? null;

  useEffect(() => {
    void appDataDir().then(setAppDir).catch((err) => setError(String(err)));
    const started = performance.now();
    void api
      .bootstrap()
      .then((state) => {
        setNotes(state.notes);
        setCurrent(state.current);
        setStatus("Saved");
        console.info("snapline.frontend_bootstrap_ms", Math.round(performance.now() - started));
      })
      .catch((err) => {
        setError(String(err));
        setStatus("Error");
      });
  }, []);

  async function createNote() {
    try {
      const note = await api.createNote();
      setCurrent(note);
      setNotes((existing) =>
        sortNotes([{ id: note.id, title: note.title, updated_at: note.updated_at }, ...existing]),
      );
    } catch (createError) {
      setError(String(createError));
      setStatus("Error");
    }
  }

  async function deleteCurrent() {
    if (!current) return;
    try {
      const nextNotes = await api.deleteNote(current.id);
      setNotes(nextNotes);
      if (nextNotes.length > 0) {
        setCurrent(await api.getNote(nextNotes[0].id));
        return;
      }
      const next = await api.createNote();
      setNotes([{ id: next.id, title: next.title, updated_at: next.updated_at }]);
      setCurrent(next);
    } catch (deleteError) {
      setError(String(deleteError));
      setStatus("Error");
    }
  }

  async function selectNote(id: string) {
    try {
      const note = await api.getNote(id);
      setCurrent(note);
    } catch (selectError) {
      setError(String(selectError));
      setStatus("Error");
    }
  }

  function onSaved(note: Note) {
    setNotes((existing) =>
      sortNotes([
        { id: note.id, title: note.title, updated_at: note.updated_at },
        ...existing.filter((item) => item.id !== note.id),
      ]),
    );
    if (currentIdRef.current === note.id) {
      setCurrent(note);
      setStatus("Saved");
    }
  }

  async function resolveAsset(markdownPath: string) {
    if (!appDir) return markdownPath;
    return convertFileSrc(await join(appDir, markdownPath));
  }

  return (
    <main className="appShell">
      <aside className="sidebar">
        <header className="sidebarHeader">
          <div>
            <div className="brand">Snapline</div>
            <div className="brandSub">Local notes</div>
          </div>
          <button className="iconButton" onClick={createNote} title="New note">
            +
          </button>
        </header>
        <nav className="noteList" aria-label="Notes">
          {notes.map((note) => (
            <button
              className={note.id === current?.id ? "noteItem active" : "noteItem"}
              key={note.id}
              onClick={() => void selectNote(note.id)}
              title={note.title}
            >
              <span className="noteTitle">{note.title}</span>
              <span className="noteTime">{new Date(note.updated_at).toLocaleString()}</span>
            </button>
          ))}
        </nav>
      </aside>

      <section className="workspace">
        <header className="workspaceHeader">
          <div className="status">{status}</div>
          <button className="ghostButton" disabled={!current} onClick={() => void deleteCurrent()}>
            Delete
          </button>
        </header>

        {error ? <div className="errorBanner">{error}</div> : null}

        {current ? (
          <EditorPane
            key={current.id}
            note={current}
            onAssetResolved={resolveAsset}
            onSaved={onSaved}
            setStatus={setStatus}
          />
        ) : (
          <div className="emptyState">Create or select a note</div>
        )}
      </section>
    </main>
  );
}

function sortNotes(notes: NoteSummary[]) {
  return [...notes].sort((a, b) => b.updated_at.localeCompare(a.updated_at));
}
