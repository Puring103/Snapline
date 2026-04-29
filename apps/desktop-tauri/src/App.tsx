import { emit, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { EditorContent, useEditor } from "@tiptap/react";
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { api } from "./api";
import { createMarkdownExtensions } from "./editorExtensions";
import { EditorPane } from "./EditorPane";
import { assetUrlFromMarkdownPath, rewriteMarkdownImageSources } from "./markdown";
import {
  createDraftSession,
  deleteConfirmationFor,
  isSessionDirty,
  sortNotes,
  upsertNote,
  type ActiveSession,
} from "./session";
import type { Note, NoteSummary, SavedAsset } from "./types";
import { openListWindow, openNoteWindow, readAppRoute } from "./window";

const DEFAULT_SHORTCUT = "Ctrl+Shift+Space";
const THEME_STORAGE_KEY = "snapline.theme";

type ThemeMode = "system" | "light" | "dark";

export function App() {
  const route = useMemo(readAppRoute, []);
  useThemeSync();

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
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [confirmingDeleteId, setConfirmingDeleteId] = useState<string | null>(null);
  const [shortcut, setShortcut] = useState(DEFAULT_SHORTCUT);
  const [themeMode, setThemeMode] = useThemeMode();

  const refreshNotes = useCallback(async (quiet = false) => {
    try {
      setError(null);
      if (!quiet) {
        setStatus("Loading");
      }
      const state = await api.bootstrap();
      setNotes(state.notes);
      setConfirmingDeleteId((current) =>
        current && state.notes.some((note) => note.id === current) ? current : null,
      );
      setStatus("Ready");
    } catch (err) {
      setError(String(err));
      setStatus("Error");
    }
  }, []);

  useEffect(() => {
    void refreshNotes();
    void api
      .getOpenShortcut()
      .then((next) => {
        setShortcut(next ?? DEFAULT_SHORTCUT);
      })
      .catch(() => {
        setShortcut(DEFAULT_SHORTCUT);
      });
  }, [refreshNotes]);

  useEffect(() => {
    let unlistenSaved: (() => void) | null = null;
    let unlistenDeleted: (() => void) | null = null;
    const refreshQuietly = () => void refreshNotes(true);
    const refreshWhenVisible = () => {
      if (document.visibilityState === "visible") {
        refreshQuietly();
      }
    };

    void listen("note-saved", refreshQuietly).then((unlisten) => {
      unlistenSaved = unlisten;
    });
    void listen("note-deleted", refreshQuietly).then((unlisten) => {
      unlistenDeleted = unlisten;
    });

    window.addEventListener("focus", refreshQuietly);
    document.addEventListener("visibilitychange", refreshWhenVisible);
    const intervalId = window.setInterval(refreshWhenVisible, 2000);

    return () => {
      window.clearInterval(intervalId);
      window.removeEventListener("focus", refreshQuietly);
      document.removeEventListener("visibilitychange", refreshWhenVisible);
      unlistenSaved?.();
      unlistenDeleted?.();
    };
  }, [refreshNotes]);

  const visibleNotes = useMemo(() => sortNotes(notes), [notes]);

  function persistShortcut(nextShortcut: string) {
    void api
      .setOpenShortcut(nextShortcut)
      .then(() => setShortcut(nextShortcut))
      .catch((err) => setError(String(err)));
  }

  async function handleNewNote() {
    try {
      setError(null);
      await openNoteWindow();
    } catch (err) {
      setError(String(err));
      setStatus("Error");
    }
  }

  async function handleSelectNote(noteId: string) {
    try {
      setError(null);
      await openNoteWindow(noteId);
    } catch (err) {
      setError(String(err));
      setStatus("Error");
    }
  }

  async function handleDeleteNote(id: string) {
    try {
      setError(null);
      const nextNotes = await api.deleteNote(id);
      setNotes(nextNotes);
      setConfirmingDeleteId(null);
      await emit("note-deleted", { id });
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
        <div className="brandBlock">
          <LogoIcon />
          <div>
            <div className="listTitle">Snapline</div>
            <div className="listSub">{status} / {visibleNotes.length} items</div>
          </div>
        </div>
        <div className="listHeaderActions">
          <IconButton label="New note" onClick={() => void handleNewNote()}><PlusIcon /></IconButton>
          <IconButton label="Refresh" onClick={() => void refreshNotes()}><RefreshIcon /></IconButton>
          <IconButton label="Settings" onClick={() => setSettingsOpen(true)}><SettingsIcon /></IconButton>
        </div>
      </header>

      {error ? <div className="errorBanner">{error}</div> : null}

      <section className="listPanel" aria-label="Note list">
        {visibleNotes.length === 0 ? (
          <div className="emptyList">No saved notes yet.</div>
        ) : (
          visibleNotes.map((note) => {
            const pinned = note.pinned ?? false;
            const confirmingDelete = confirmingDeleteId === note.id;

            return (
              <article
                className={pinned ? "noteRow pinned" : "noteRow"}
                key={note.id}
                onClick={() => void handleSelectNote(note.id)}
                onKeyDown={(event) => {
                  if (event.key === "Enter" || event.key === " ") {
                    event.preventDefault();
                    void handleSelectNote(note.id);
                  }
                }}
                role="button"
                tabIndex={0}
              >
                <div className="noteRowHeader">
                  <div className="noteRowTitleBlock">
                    <span className="noteRowTitle">{note.title}</span>
                    <span className="noteRowTime">{new Date(note.updated_at).toLocaleString()}</span>
                  </div>
                  <div className="noteRowActions">
                    {confirmingDelete ? (
                      <>
                        <IconButton danger label="Confirm delete" onClick={() => void handleDeleteNote(note.id)}>
                          <CheckIcon />
                        </IconButton>
                        <IconButton label="Cancel delete" onClick={() => setConfirmingDeleteId(null)}>
                          <CloseIcon />
                        </IconButton>
                      </>
                    ) : (
                      <>
                        <IconButton active={pinned} label={pinned ? "Unpin" : "Pin"} onClick={() => void handleTogglePinned(note.id)}>
                          <StarIcon />
                        </IconButton>
                        <IconButton danger label="Delete" onClick={() => setConfirmingDeleteId((current) => deleteConfirmationFor(current, note.id))}>
                          <TrashIcon />
                        </IconButton>
                      </>
                    )}
                  </div>
                </div>
                <MarkdownPreview markdown={note.preview || "No preview"} />
              </article>
            );
          })
        )}
      </section>

      {settingsOpen ? (
        <SettingsPanel
          onClose={() => setSettingsOpen(false)}
          shortcut={shortcut}
          onShortcutChange={setShortcut}
          onShortcutSave={persistShortcut}
          themeMode={themeMode}
          onThemeModeChange={setThemeMode}
        />
      ) : null}
    </main>
  );
}

function MarkdownPreview({ markdown }: { markdown: string }) {
  const editor = useEditor({
    extensions: createMarkdownExtensions(),
    content: rewriteMarkdownImageSources(markdown, assetUrlFromMarkdownPath),
    contentType: "markdown",
    editable: false,
  });

  useEffect(() => {
    if (!editor) return;
    editor.commands.setContent(rewriteMarkdownImageSources(markdown, assetUrlFromMarkdownPath));
  }, [editor, markdown]);

  if (!editor) {
    return <div className="noteRowPreview">Loading preview...</div>;
  }

  return <EditorContent editor={editor} className="noteRowPreview" />;
}

function NoteEditorWindow({ noteId }: { noteId: string | null }) {
  const [session, setSession] = useState<ActiveSession>(() => createDraftSession());
  const [pinned, setPinned] = useState(false);
  const [status, setStatus] = useState("Loading");
  const [error, setError] = useState<string | null>(null);
  const [deleted, setDeleted] = useState(false);

  const sessionRef = useRef(session);
  const pinnedRef = useRef(pinned);
  const deletedRef = useRef(deleted);
  const saveTimerRef = useRef<number | null>(null);
  const titleInputRef = useRef<HTMLInputElement | null>(null);
  sessionRef.current = session;
  pinnedRef.current = pinned;
  deletedRef.current = deleted;

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
    const currentId = session.id;
    if (!currentId) return;

    let unlisten: (() => void) | null = null;
    void listen<{ id: string }>("note-deleted", (event) => {
      if (event.payload.id !== currentId) {
        return;
      }

      clearSaveTimer();
      setDeleted(true);
      setStatus("Deleted");
      setError("This note was deleted from the list. This window is now read-only.");
    }).then((nextUnlisten) => {
      unlisten = nextUnlisten;
    });

    return () => {
      unlisten?.();
    };
  }, [session.id]);

  useEffect(() => {
    clearSaveTimer();

    if (deleted) {
      setStatus("Deleted");
      return;
    }

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
  }, [deleted, session.bodyMd, session.kind, session.persistedBodyMd, session.persistedTitle, session.title]);

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
    setDeleted(false);
    setError(null);
    setStatus("Draft");
    queueMicrotask(() => {
      titleInputRef.current?.focus();
      titleInputRef.current?.select();
    });
  }

  async function persistCurrentSession() {
    const snapshot = sessionRef.current;
    if (deletedRef.current || !isSessionDirty(snapshot)) {
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
      await emit("note-saved", { id: saved.id });
      return saved;
    } catch (err) {
      setError(String(err));
      setStatus("Error");
      return null;
    }
  }

  function handleTitleChange(nextTitle: string) {
    if (deletedRef.current) return;
    setError(null);
    setSession((current) => ({ ...current, title: nextTitle }));
  }

  function handleBodyChange(nextBodyMd: string) {
    if (deletedRef.current) return;
    setError(null);
    setSession((current) => ({ ...current, bodyMd: nextBodyMd }));
  }

  async function handleRequestImageSave(bytes: number[]): Promise<SavedAsset | null> {
    if (deletedRef.current) return null;

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
    if (!targetId || deletedRef.current) return;

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
      await emit("note-saved", { id: saved.id });
      await getCurrentWindow().setAlwaysOnTop(nextPinned);
      setStatus("Saved");
    } catch (err) {
      setError(String(err));
      setStatus("Error");
    }
  }

  function handleCreateNoteWindow() {
    void openNoteWindow();
  }

  function handleOpenListWindow() {
    void openListWindow();
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
          <IconButton label="Notes" onClick={handleOpenListWindow}><ListIcon /></IconButton>
          <input
            aria-label="Note title"
            className="titleInput"
            disabled={deleted}
            ref={titleInputRef}
            onChange={(event) => handleTitleChange(event.target.value)}
            placeholder="Untitled"
            value={session.title}
          />
          <IconButton active={pinned} disabled={deleted} label={pinned ? "Unpin note" : "Pin note"} onClick={() => void handleTogglePinned()}>
            <PinIcon />
          </IconButton>
          <IconButton label="New window" onClick={handleCreateNoteWindow}><PlusIcon /></IconButton>
        </header>

        <section className="noteSurface">
          <div className="noteMeta">
            <span>{status}</span>
            <span>{session.kind === "draft" ? "Draft" : "Saved note"}</span>
          </div>
          {error ? (
            <div className="errorBanner">
              <span>{error}</span>
              {deleted ? (
                <button className="linkButton" onClick={() => void getCurrentWindow().close()} type="button">
                  Close
                </button>
              ) : null}
            </div>
          ) : null}
          <EditorPane
            bodyMarkdown={session.bodyMd}
            onBodyChange={handleBodyChange}
            onRequestImageSave={handleRequestImageSave}
            readOnly={deleted}
          />
        </section>
      </section>
    </main>
  );
}

function SettingsPanel({
  onClose,
  shortcut,
  onShortcutChange,
  onShortcutSave,
  themeMode,
  onThemeModeChange,
}: {
  onClose: () => void;
  shortcut: string;
  onShortcutChange: (value: string) => void;
  onShortcutSave: (value: string) => void;
  themeMode: ThemeMode;
  onThemeModeChange: (value: ThemeMode) => void;
}) {
  return (
    <div className="settingsBackdrop" onClick={onClose}>
      <section className="settingsPanel" onClick={(event) => event.stopPropagation()}>
        <header className="settingsHeader">
          <div>
            <div className="settingsTitle">Settings</div>
            <div className="settingsSub">Shortcut and appearance</div>
          </div>
          <IconButton label="Close settings" onClick={onClose}><CloseIcon /></IconButton>
        </header>

        <label className="settingsField">
          <span>Open shortcut</span>
          <div className="shortcutRow">
            <input
              className="shortcutInput"
              value={shortcut}
              onChange={(event) => onShortcutChange(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  onShortcutSave(shortcut);
                }
              }}
              placeholder={DEFAULT_SHORTCUT}
            />
            <button className="drawerAction" onClick={() => onShortcutSave(shortcut)} type="button">
              Save
            </button>
          </div>
        </label>

        <div className="settingsField">
          <span>Theme</span>
          <div className="segmentedControl">
            {(["system", "light", "dark"] as const).map((mode) => (
              <button
                className={themeMode === mode ? "segment active" : "segment"}
                key={mode}
                onClick={() => onThemeModeChange(mode)}
                type="button"
              >
                {mode}
              </button>
            ))}
          </div>
        </div>
      </section>
    </div>
  );
}

function useThemeMode(): [ThemeMode, (mode: ThemeMode) => void] {
  const [mode, setMode] = useState<ThemeMode>(() => {
    const stored = localStorage.getItem(THEME_STORAGE_KEY);
    return stored === "light" || stored === "dark" || stored === "system" ? stored : "system";
  });

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      document.documentElement.dataset.theme = mode === "system" ? (media.matches ? "dark" : "light") : mode;
      document.documentElement.dataset.themeMode = mode;
    };

    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [mode]);

  const updateMode = (nextMode: ThemeMode) => {
    localStorage.setItem(THEME_STORAGE_KEY, nextMode);
    setMode(nextMode);
  };

  return [mode, updateMode];
}

function useThemeSync() {
  const [mode, setMode] = useState<ThemeMode>(() => readStoredThemeMode());

  useEffect(() => {
    const onStorage = (event: StorageEvent) => {
      if (event.key === THEME_STORAGE_KEY) {
        setMode(readStoredThemeMode());
      }
    };

    window.addEventListener("storage", onStorage);
    return () => window.removeEventListener("storage", onStorage);
  }, []);

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      document.documentElement.dataset.theme = mode === "system" ? (media.matches ? "dark" : "light") : mode;
      document.documentElement.dataset.themeMode = mode;
    };

    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [mode]);
}

function readStoredThemeMode(): ThemeMode {
  const stored = localStorage.getItem(THEME_STORAGE_KEY);
  return stored === "light" || stored === "dark" || stored === "system" ? stored : "system";
}

function IconButton({
  active = false,
  danger = false,
  disabled = false,
  label,
  onClick,
  children,
}: {
  active?: boolean;
  danger?: boolean;
  disabled?: boolean;
  label: string;
  onClick: () => void;
  children: ReactNode;
}) {
  const className = [
    "iconButton",
    active ? "iconButtonActive" : "",
    danger ? "danger" : "",
  ].filter(Boolean).join(" ");

  return (
    <button aria-label={label} className={className} disabled={disabled} onClick={(event) => {
      event.stopPropagation();
      onClick();
    }} title={label} type="button">
      {children}
    </button>
  );
}

function LogoIcon() {
  return (
    <svg className="logoMark" viewBox="0 0 32 32" aria-hidden="true">
      <rect x="5" y="4" width="18" height="22" rx="4" />
      <path d="M11 11h9M11 16h7M11 21h5" />
      <path d="M20 6l7 7" />
      <path d="M23 5l4 4-12 12-5 1 1-5 12-12z" />
    </svg>
  );
}

function PlusIcon() {
  return <svg className="flatIcon" viewBox="0 0 24 24" aria-hidden="true"><path d="M12 5v14M5 12h14" /></svg>;
}

function RefreshIcon() {
  return <svg className="flatIcon" viewBox="0 0 24 24" aria-hidden="true"><path d="M19 8a7 7 0 0 0-12-2l-2 2M5 4v4h4M5 16a7 7 0 0 0 12 2l2-2M19 20v-4h-4" /></svg>;
}

function SettingsIcon() {
  return <svg className="flatIcon" viewBox="0 0 24 24" aria-hidden="true"><path d="M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8z" /><path d="M4 12h2M18 12h2M12 4v2M12 18v2M6.3 6.3l1.4 1.4M16.3 16.3l1.4 1.4M17.7 6.3l-1.4 1.4M7.7 16.3l-1.4 1.4" /></svg>;
}

function ListIcon() {
  return <svg className="flatIcon" viewBox="0 0 24 24" aria-hidden="true"><path d="M8 7h11M8 12h11M8 17h11" /><path d="M4.5 7h.1M4.5 12h.1M4.5 17h.1" /></svg>;
}

function PinIcon() {
  return <svg className="flatIcon" viewBox="0 0 24 24" aria-hidden="true"><path d="M14 4l6 6-3 1-4 4v5l-2 2-2-7-7-2 2-2h5l4-4 1-3z" /></svg>;
}

function StarIcon() {
  return <svg className="flatIcon" viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3.8l2.5 5.1 5.6.8-4.1 4 1 5.6-5-2.6-5 2.6 1-5.6-4.1-4 5.6-.8L12 3.8z" /></svg>;
}

function CheckIcon() {
  return <svg className="flatIcon" viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12.5l4.2 4.2L19 6.8" /></svg>;
}

function TrashIcon() {
  return <svg className="flatIcon" viewBox="0 0 24 24" aria-hidden="true"><path d="M5 7h14M9 7V5h6v2M8 10v8M12 10v8M16 10v8" /><path d="M7 7l1 14h8l1-14" /></svg>;
}

function CloseIcon() {
  return <svg className="flatIcon" viewBox="0 0 24 24" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18" /></svg>;
}
