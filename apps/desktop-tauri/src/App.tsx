import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "./api";
import { EditorPane } from "./EditorPane";
import { assetUrlFromMarkdownPath } from "./markdown";
import { createDraftSession, isSessionDirty, matchesShortcut, sortNotes, upsertNote, type ActiveSession } from "./session";
import type { Note, NoteSummary, SavedAsset } from "./types";
import { openNoteWindow, readAppRoute } from "./window";
import { getCurrentWindow } from "@tauri-apps/api/window";

const DEFAULT_SHORTCUT = "Ctrl+Shift+Space";

export function App() {
  const route = useMemo(readAppRoute, []);
  useEffect(() => {
    const url = new URL(window.location.href);
    if (!url.searchParams.has("mode")) {
      url.searchParams.set("mode", "note");
      window.history.replaceState({}, "", `${url.pathname}${url.search}`);
    }
  }, []);

  return route.mode === "list" ? <NotesListWindow /> : <NoteEditorWindow noteId={route.noteId} />;
}

function NotesListWindow() {
  const [notes, setNotes] = useState<NoteSummary[]>([]);
  const [status, setStatus] = useState("Loading");
  const [error, setError] = useState<string | null>(null);
  const [shortcut, setShortcut] = useState(DEFAULT_SHORTCUT);

  useEffect(() => {
    void api
      .bootstrap()
      .then((state) => {
        setNotes(state.notes);
        setStatus("Ready");
      })
      .catch((err) => {
        setError(String(err));
        setStatus("Error");
      });

    void api
      .getOpenShortcut()
      .then((next) => {
        setShortcut(next ?? DEFAULT_SHORTCUT);
      })
      .catch(() => {
        setShortcut(DEFAULT_SHORTCUT);
      });
  }, []);

  const visibleNotes = useMemo(() => sortNotes(notes), [notes]);

  function persistShortcut(nextShortcut: string) {
    void api
      .setOpenShortcut(nextShortcut)
      .then(() => setShortcut(nextShortcut))
      .catch((err) => setError(String(err)));
  }

  function handleNewNote() {
    openNoteWindow();
  }

  function handleOpenNote(noteId: string) {
    openNoteWindow(noteId);
  }

  async function handleDeleteNote(id: string) {
    try {
      setError(null);
      const nextNotes = await api.deleteNote(id);
      setNotes(nextNotes);
    } catch (err) {
      setError(String(err));
      setStatus("Error");
    }
  }

  async function handleTogglePinned(noteId: string) {
    const current = notes.find((note) => note.id === noteId);
    if (!current) return;

    try {
      setError(null);
      const saved = await api.setNotePinned(noteId, !(current.pinned ?? false));
      setNotes((existing) => upsertNote(existing, saved));
    } catch (err) {
      setError(String(err));
      setStatus("Error");
    }
  }

  return (
    <main className="appShell listShell">
      <header className="listHeader">
        <div>
          <div className="listTitle">Notes</div>
          <div className="listSub">{status} · {visibleNotes.length} items</div>
        </div>
        <div className="listHeaderActions">
          <button className="iconButton" onClick={handleNewNote} title="New note" type="button">+</button>
          <button className="iconButton" onClick={() => void api.bootstrap().then((state) => setNotes(state.notes))} title="Refresh" type="button">↻</button>
        </div>
      </header>

      {error ? <div className="errorBanner">{error}</div> : null}

      <section className="listPanel" aria-label="Note list">
        {visibleNotes.length === 0 ? (
          <div className="emptyList">No saved notes yet.</div>
        ) : (
          visibleNotes.map((note) => {
            const pinned = note.pinned ?? false;
            return (
              <article className={pinned ? "noteRow pinned" : "noteRow"} key={note.id}>
                <button className="noteRowMain" onClick={() => handleOpenNote(note.id)} type="button">
                  <div className="noteRowTop">
                    <span className="noteRowTitle">{note.title}</span>
                    {pinned ? <span className="noteRowFlag">Pin</span> : null}
                  </div>
                  <div className="noteRowPreview">{note.preview || "No preview"}</div>
                  <div className="noteRowTime">{new Date(note.updated_at).toLocaleString()}</div>
                </button>
                <div className="noteRowActions">
                  <button className={pinned ? "iconButton iconButtonActive" : "iconButton"} onClick={() => void handleTogglePinned(note.id)} title={pinned ? "Unpin" : "Pin"} type="button">↥</button>
                  <button className="iconButton danger" onClick={() => void handleDeleteNote(note.id)} title="Delete" type="button">×</button>
                </div>
              </article>
            );
          })
        )}
      </section>

      <footer className="listFooter">
        <label className="shortcutField">
          <span>Open shortcut</span>
          <div className="shortcutRow">
            <input
              className="shortcutInput"
              value={shortcut}
              onChange={(event) => setShortcut(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  persistShortcut(shortcut);
                }
              }}
              placeholder={DEFAULT_SHORTCUT}
            />
            <button className="drawerAction" onClick={() => persistShortcut(shortcut)} type="button">
              Save
            </button>
          </div>
        </label>
      </footer>
    </main>
  );
}

function NoteEditorWindow({ noteId }: { noteId: string | null }) {
  const [session, setSession] = useState<ActiveSession>(() => createDraftSession());
  const [pinned, setPinned] = useState(false);
  const [status, setStatus] = useState("Loading");
  const [error, setError] = useState<string | null>(null);

  const sessionRef = useRef(session);
  const pinnedRef = useRef(pinned);
  const saveTimerRef = useRef<number | null>(null);
  const titleInputRef = useRef<HTMLInputElement | null>(null);
  sessionRef.current = session;
  pinnedRef.current = pinned;

  useEffect(() => {
    let cancelled = false;

    async function loadNote() {
      try {
        setError(null);

        const next = noteId ? await api.getNote(noteId) : await api.createNote();
        if (cancelled) {
          return;
        }

        beginSessionFromNote(next);
        setStatus("Draft");
      } catch (err) {
        if (!cancelled) {
          setError(String(err));
          setStatus("Error");
        }
      }
    }

    void loadNote();

    return () => {
      cancelled = true;
      clearSaveTimer();
    };
  }, [noteId]);

  useEffect(() => {
    clearSaveTimer();

    if (!isSessionDirty(session)) {
      setStatus(session.kind === "draft" ? "Draft" : "Saved");
      return;
    }

    setStatus("Saving");
    saveTimerRef.current = window.setTimeout(() => {
      saveTimerRef.current = null;
      void persistCurrentSession();
    }, 450);

    return () => clearSaveTimer();
  }, [session.bodyMd, session.kind, session.persistedBodyMd, session.persistedTitle, session.title]);

  function beginSessionFromNote(note: Note) {
    clearSaveTimer();
    setSession({
      kind: noteId ? "existing" : "draft",
      id: note.id,
      title: note.title,
      bodyMd: note.content_md,
      persistedTitle: note.title,
      persistedBodyMd: note.content_md,
    });
    setPinned(note.pinned ?? false);
    setError(null);
    setStatus("Draft");
    queueMicrotask(() => {
      titleInputRef.current?.focus();
      titleInputRef.current?.select();
    });
  }

  async function persistCurrentSession() {
    const snapshot = sessionRef.current;
    if (!isSessionDirty(snapshot)) {
      return null;
    }

    try {
      setError(null);
      setStatus("Saving");

      const noteIdToSave = snapshot.id ?? (await api.createNote()).id;
      const saved = await api.saveNote(noteIdToSave, snapshot.title, snapshot.bodyMd, pinnedRef.current);

      setSession({
        kind: "existing",
        id: saved.id,
        title: saved.title,
        bodyMd: saved.content_md,
        persistedTitle: saved.title,
        persistedBodyMd: saved.content_md,
      });
      setPinned(saved.pinned ?? false);
      setStatus("Saved");
      return saved;
    } catch (err) {
      setError(String(err));
      setStatus("Error");
      return null;
    }
  }

  function handleTitleChange(nextTitle: string) {
    setError(null);
    setSession((current) => ({ ...current, title: nextTitle }));
  }

  function handleBodyChange(nextBodyMd: string) {
    setError(null);
    setSession((current) => ({ ...current, bodyMd: nextBodyMd }));
  }

  async function handleRequestImageSave(bytes: number[]): Promise<SavedAsset | null> {
    const snapshot = sessionRef.current;
    let noteIdToUse = snapshot.id;

    if (!noteIdToUse) {
      const draft = await api.createNote();
      noteIdToUse = draft.id;
      setSession((current) => ({
        ...current,
        kind: "draft",
        id: draft.id,
      }));
    }

    if (!noteIdToUse) {
      return null;
    }

    try {
      const asset = await api.savePngAsset(noteIdToUse, bytes);
      return {
        markdown_path: asset.markdown_path,
        asset_url: assetUrlFromMarkdownPath(asset.markdown_path),
      };
    } catch (err) {
      setError(String(err));
      setStatus("Error");
      return null;
    }
  }

  async function handleTogglePinned() {
    const targetId = sessionRef.current.id;
    if (!targetId) return;

    try {
      const nextPinned = !pinnedRef.current;
      const saved = await api.setNotePinned(targetId, nextPinned);
      setPinned(saved.pinned ?? nextPinned);
      setSession((current) => ({
        ...current,
        title: saved.title,
        bodyMd: saved.content_md,
        persistedTitle: saved.title,
        persistedBodyMd: saved.content_md,
      }));
      await getCurrentWindow().setAlwaysOnTop(nextPinned);
      setStatus("Saved");
    } catch (err) {
      setError(String(err));
      setStatus("Error");
    }
  }

  function handleCreateNoteWindow() {
    openNoteWindow();
  }

  function clearSaveTimer() {
    if (saveTimerRef.current !== null) {
      window.clearTimeout(saveTimerRef.current);
      saveTimerRef.current = null;
    }
  }

  return (
    <main className="appShell editorShellRoot">
      <section className="shellFrame editorShellFrame">
        <header className="chromeBar">
          <input
            aria-label="Note title"
            className="titleInput"
            ref={titleInputRef}
            onChange={(event) => handleTitleChange(event.target.value)}
            placeholder="Untitled"
            value={session.title}
          />
          <button className={pinned ? "iconButton iconButtonActive" : "iconButton"} onClick={() => void handleTogglePinned()} title={pinned ? "Unpin note" : "Pin note"} type="button">↥</button>
          <button className="iconButton" onClick={handleCreateNoteWindow} title="New window" type="button">+</button>
        </header>

        <section className="noteSurface">
          <div className="noteMeta">
            <span>{status}</span>
            <span>{session.kind === "draft" ? "Draft" : "Saved note"}</span>
          </div>
          {error ? <div className="errorBanner">{error}</div> : null}
          <EditorPane
            bodyMarkdown={session.bodyMd}
            onBodyChange={handleBodyChange}
            onRequestImageSave={handleRequestImageSave}
          />
        </section>
      </section>
    </main>
  );
}
