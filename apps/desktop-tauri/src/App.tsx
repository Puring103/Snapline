import { emit, listen } from "@tauri-apps/api/event";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState, type MouseEvent, type ReactNode } from "react";
import { api } from "./api";
import { webviewAssetUrlFromFilesystemPath } from "./assetUrl";
import { hasTransientImageSource } from "./markdown";
import { DEFAULT_EDITOR_MODE, toggleEditorMode, type EditorMode } from "./editorMode";
import {
  createDraftSession,
  deleteConfirmationFor,
  hasMeaningfulDraftContent,
  isSessionDirty,
  sortNotes,
  upsertNote,
  type ActiveSession,
} from "./session";
import { startupLog } from "./startupLog";
import { SyncSettings } from "./SyncSettings";
import { syncStatusLabel } from "./syncStatus";
import type { Note, NoteSummary, SavedAsset, SyncAccountState } from "./types";
import { openListWindow, openNoteWindow, readAppRoute, shouldDeferInitialNoteLoad, shouldStartWindowDrag } from "./window";

const DEFAULT_SHORTCUT = "Ctrl+Shift+Space";
const FOCUS_EDITOR_EVENT = "snapline-focus-editor";
const THEME_STORAGE_KEY = "snapline.theme";
const LazyEditorPane = lazy(() => {
  startupLog("editor_chunk_requested");
  return import("./EditorPane").then((module) => {
    startupLog("editor_chunk_loaded");
    return { default: module.EditorPane };
  });
});
const LazyMarkdownPreview = lazy(() => {
  startupLog("preview_chunk_requested");
  return import("./MarkdownPreview").then((module) => {
    startupLog("preview_chunk_loaded");
    return { default: module.MarkdownPreview };
  });
});

type ThemeMode = "system" | "light" | "dark";

export function App() {
  const route = useMemo(readAppRoute, []);
  useThemeSync();

  useEffect(() => {
    startupLog("route_mounted", {
      mode: route.mode,
      has_note_id: route.noteId !== null,
    });
  }, [route.mode, route.noteId]);

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
  const [syncAccount, setSyncAccount] = useState<SyncAccountState | null>(null);
  const [dataDir, setDataDir] = useState<string | null>(null);
  const [confirmingDeleteId, setConfirmingDeleteId] = useState<string | null>(null);
  const [shortcut, setShortcut] = useState(DEFAULT_SHORTCUT);
  const [autostartEnabled, setAutostartEnabled] = useState(false);
  const [themeMode, setThemeMode] = useThemeMode();
  const syncStatus = syncStatusLabel(syncAccount);

  const refreshNotes = useCallback(async (quiet = false) => {
    try {
      const startedAt = performance.now();
      setError(null);
      if (!quiet) {
        setStatus("Loading");
      }
      const state = await api.bootstrap();
      setDataDir(state.data_dir);
      startupLog("list_bootstrap_done", {
        quiet,
        notes: state.notes.length,
        duration_ms: Math.round(performance.now() - startedAt),
      });
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
    void isEnabled()
      .then(setAutostartEnabled)
      .catch(() => setAutostartEnabled(false));
    void api.getSyncAccountState().then(setSyncAccount).catch(() => setSyncAccount(null));
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

  function persistAutostart(nextEnabled: boolean) {
    setAutostartEnabled(nextEnabled);
    void (nextEnabled ? enable() : disable()).catch((err) => {
      setAutostartEnabled(!nextEnabled);
      setError(String(err));
    });
  }

  async function handleNewNote(event?: MouseEvent<HTMLElement>) {
    try {
      setError(null);
      await openNoteWindow(null, pointerPositionFromEvent(event));
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

  function handleHeaderDrag(event: MouseEvent<HTMLElement>) {
    if (event.button !== 0 || !shouldStartWindowDrag(event.target)) return;
    void getCurrentWindow().startDragging();
  }

  return (
    <main className="appShell listShell">
      <header className="listHeader" onMouseDown={handleHeaderDrag}>
        <div className="brandBlock">
          <LogoIcon />
          <div>
            <div className="listTitle">Snapline</div>
            <div className="listSub">{status} / {visibleNotes.length} items / {syncStatus}</div>
          </div>
        </div>
        <div className="listHeaderActions">
          <IconButton label="New note" onClick={(event) => void handleNewNote(event)}><PlusIcon /></IconButton>
          <IconButton label="Settings" onClick={() => setSettingsOpen(true)}><SettingsIcon /></IconButton>
          <IconButton label="Close window" onClick={() => void getCurrentWindow().close()}><CloseIcon /></IconButton>
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
                          <PinIcon />
                        </IconButton>
                        <IconButton danger label="Delete" onClick={() => setConfirmingDeleteId((current) => deleteConfirmationFor(current, note.id))}>
                          <TrashIcon />
                        </IconButton>
                      </>
                    )}
                  </div>
                </div>
                <Suspense fallback={<div className="noteRowPreview">{note.preview || "No preview"}</div>}>
                  <LazyMarkdownPreview dataDir={dataDir} markdown={note.preview_md || note.preview || "No preview"} />
                </Suspense>
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
          autostartEnabled={autostartEnabled}
          onAutostartChange={persistAutostart}
          themeMode={themeMode}
          onThemeModeChange={setThemeMode}
          syncAccount={syncAccount}
          onSyncSaved={setSyncAccount}
        />
      ) : null}
    </main>
  );
}

function NoteEditorWindow({ noteId }: { noteId: string | null }) {
  const windowLabel = useMemo(() => getCurrentWindow().label, []);
  const [session, setSession] = useState<ActiveSession>(() => createDraftSession());
  const [notePinned, setNotePinned] = useState(false);
  const [windowAlwaysOnTop, setWindowAlwaysOnTop] = useState(false);
  const [status, setStatus] = useState("Loading");
  const [error, setError] = useState<string | null>(null);
  const [deleted, setDeleted] = useState(false);
  const [focusRequestId, setFocusRequestId] = useState(0);
  const [noteLoadArmed, setNoteLoadArmed] = useState(noteId !== null);
  const [editorMode, setEditorMode] = useState<EditorMode>(DEFAULT_EDITOR_MODE);
  const [chromeMenuOpen, setChromeMenuOpen] = useState(false);
  const [dataDir, setDataDir] = useState<string | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [syncAccount, setSyncAccount] = useState<SyncAccountState | null>(null);
  const [shortcut, setShortcut] = useState(DEFAULT_SHORTCUT);
  const [autostartEnabled, setAutostartEnabled] = useState(false);
  const [themeMode, setThemeMode] = useThemeMode();

  const sessionRef = useRef(session);
  const notePinnedRef = useRef(notePinned);
  const deletedRef = useRef(deleted);
  const saveTimerRef = useRef<number | null>(null);
  const titleInputRef = useRef<HTMLInputElement | null>(null);
  sessionRef.current = session;
  notePinnedRef.current = notePinned;
  deletedRef.current = deleted;

  useEffect(() => {
    let cancelled = false;

    if (noteId !== null) {
      setNoteLoadArmed(true);
      return () => {
        cancelled = true;
      };
    }

    void api
      .launchedInBackground()
      .then((isBackgroundLaunch) => {
        if (cancelled) return;
        if (
          shouldDeferInitialNoteLoad({
            launchedInBackground: isBackgroundLaunch,
            windowLabel,
            noteId,
          })
        ) {
          setStatus("Ready");
          return;
        }
        setNoteLoadArmed(true);
      })
      .catch(() => {
        if (!cancelled) {
          setNoteLoadArmed(true);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [noteId, windowLabel]);

  useEffect(() => {
    void api
      .getOpenShortcut()
      .then((next) => {
        setShortcut(next ?? DEFAULT_SHORTCUT);
      })
      .catch(() => {
        setShortcut(DEFAULT_SHORTCUT);
      });
    void isEnabled()
      .then(setAutostartEnabled)
      .catch(() => setAutostartEnabled(false));
    void api.getSyncAccountState().then(setSyncAccount).catch(() => setSyncAccount(null));
  }, []);

  useEffect(() => {
    if (!noteLoadArmed) return;

    let cancelled = false;

    async function loadNote() {
      try {
        const startedAt = performance.now();
        setError(null);

        const bootstrapState = await api.bootstrap();
        const next = noteId ? await api.getNote(noteId) : null;
        startupLog("note_data_loaded", {
          existing_note: noteId !== null,
          duration_ms: Math.round(performance.now() - startedAt),
        });
        if (cancelled) {
          return;
        }

        setDataDir(bootstrapState.data_dir);
        if (next) {
          beginSessionFromNote(next);
        } else {
          beginDraftSession();
        }
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
  }, [noteId, noteLoadArmed]);

  useEffect(() => {
    const currentId = session.id;
    if (!currentId) return;

    let unlisten: (() => void) | null = null;
    void listen<{ id: string }>("note-deleted", (event) => {
      if (event.payload.id !== currentId) {
        return;
      }

      clearSaveTimer();
      void getCurrentWindow().close().finally(() => {
        if (windowLabel === "main") {
          setSession(createDraftSession());
          setNotePinned(false);
          setDeleted(false);
          setNoteLoadArmed(false);
          setStatus("Ready");
          setError(null);
        }
      });
    }).then((nextUnlisten) => {
      unlisten = nextUnlisten;
    });

    return () => {
      unlisten?.();
    };
  }, [session.id, windowLabel]);

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    void listen(FOCUS_EDITOR_EVENT, () => {
      if (noteId === null) {
        beginDraftSession();
      }
      setNoteLoadArmed(true);
      setFocusRequestId((current) => current + 1);
    }).then((nextUnlisten) => {
      unlisten = nextUnlisten;
    });

    return () => {
      unlisten?.();
    };
  }, []);

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

    if (hasTransientImageSource(session.bodyMd)) {
      setStatus("Uploading image");
      return;
    }

    if (!hasMeaningfulDraftContent(session)) {
      setStatus("Draft");
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
    setNotePinned(note.pinned ?? false);
    setDeleted(false);
    setError(null);
    setStatus("Draft");
    queueMicrotask(() => {
      titleInputRef.current?.focus();
      titleInputRef.current?.select();
    });
  }

  function beginDraftSession() {
    clearSaveTimer();
    setSession(createDraftSession());
    setNotePinned(false);
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
    if (deletedRef.current || !isSessionDirty(snapshot) || !hasMeaningfulDraftContent(snapshot)) {
      return null;
    }

    try {
      setError(null);
      setStatus("Saving");

      const noteIdToSave = snapshot.id ?? (await api.createNote()).id;
      const saved = await api.saveNote(noteIdToSave, snapshot.title, snapshot.bodyMd, notePinnedRef.current);

      setSession({
        kind: "existing",
        id: saved.id,
        title: saved.title,
        bodyMd: saved.content_md,
        persistedTitle: saved.title,
        persistedBodyMd: saved.content_md,
      });
      setNotePinned(saved.pinned ?? false);
      setStatus("Saved");
      await emit("note-saved", { id: saved.id });
      void api.syncNow().catch(() => undefined);
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
        filesystem_path: asset.filesystem_path,
        asset_url: webviewAssetUrlFromFilesystemPath(asset.filesystem_path),
      };
    } catch (err) {
      setError(String(err));
      setStatus("Error");
      return null;
    }
  }

  async function handleToggleWindowAlwaysOnTop() {
    try {
      const nextAlwaysOnTop = !windowAlwaysOnTop;
      await getCurrentWindow().setAlwaysOnTop(nextAlwaysOnTop);
      setWindowAlwaysOnTop(nextAlwaysOnTop);
    } catch (err) {
      setError(String(err));
      setStatus("Error");
    }
  }

  function handleCreateNoteWindow(event?: MouseEvent<HTMLElement>) {
    void openNoteWindow(null, pointerPositionFromEvent(event));
  }

  function handleOpenListWindow() {
    void openListWindow();
  }

  function handleOpenSettings() {
    setSettingsOpen(true);
  }

  function persistShortcut(nextShortcut: string) {
    void api
      .setOpenShortcut(nextShortcut)
      .then(() => setShortcut(nextShortcut))
      .catch((err) => setError(String(err)));
  }

  function persistAutostart(nextEnabled: boolean) {
    setAutostartEnabled(nextEnabled);
    void (nextEnabled ? enable() : disable()).catch((err) => {
      setAutostartEnabled(!nextEnabled);
      setError(String(err));
    });
  }

  function handleChromeDrag(event: MouseEvent<HTMLElement>) {
    if (event.button !== 0 || !shouldStartWindowDrag(event.target)) return;
    void getCurrentWindow().startDragging();
  }

  function handleChromeMenuAction(action: () => void) {
    setChromeMenuOpen(false);
    action();
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
        <header className="chromeBar" onMouseDown={handleChromeDrag}>
          <IconButton
            active={windowAlwaysOnTop}
            label={windowAlwaysOnTop ? "Disable window always on top" : "Keep window on top"}
            onClick={() => void handleToggleWindowAlwaysOnTop()}
          >
            <PinIcon />
          </IconButton>
          <input
            aria-label="Note title"
            className="titleInput"
            disabled={deleted}
            ref={titleInputRef}
            onChange={(event) => handleTitleChange(event.target.value)}
            placeholder="Untitled"
            style={{ width: `${Math.max(session.title.length || 8, 8)}ch` }}
            value={session.title}
          />
          <div className="chromeActions">
            <div className="chromeMenuWrap">
              <IconButton label="More actions" onClick={() => setChromeMenuOpen((open) => !open)}><MoreIcon /></IconButton>
              {chromeMenuOpen ? (
                <div className="chromeMenu" role="menu">
                  <button onClick={() => handleChromeMenuAction(handleOpenListWindow)} role="menuitem" type="button">
                    <ListIcon />
                    <span>Notes</span>
                  </button>
                  <button onClick={(event) => handleChromeMenuAction(() => handleCreateNoteWindow(event))} role="menuitem" type="button">
                    <PlusIcon />
                    <span>New</span>
                  </button>
                  <button onClick={() => handleChromeMenuAction(handleOpenSettings)} role="menuitem" type="button">
                    <SettingsIcon />
                    <span>Settings</span>
                  </button>
                </div>
              ) : null}
            </div>
            <IconButton label="Close window" onClick={() => void getCurrentWindow().close()}><CloseIcon /></IconButton>
          </div>
        </header>

        <section className="noteSurface">
          <IconButton
            label={editorMode === "preview" ? "Switch to source mode" : "Switch to preview mode"}
            onClick={() => setEditorMode((mode) => toggleEditorMode(mode))}
            variant="floating"
          >
            {editorMode === "preview" ? <SourceModeIcon /> : <PreviewModeIcon />}
          </IconButton>
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
          <Suspense fallback={<EditorLoadingState bodyMarkdown={session.bodyMd} />}>
            <LazyEditorPane
              bodyMarkdown={session.bodyMd}
              dataDir={dataDir}
              focusRequestId={focusRequestId}
              mode={editorMode}
              onBodyChange={handleBodyChange}
              onRequestImageSave={handleRequestImageSave}
              readOnly={deleted}
            />
          </Suspense>
        </section>
        {settingsOpen ? (
          <SettingsPanel
            onClose={() => setSettingsOpen(false)}
            shortcut={shortcut}
            onShortcutChange={setShortcut}
            onShortcutSave={persistShortcut}
            autostartEnabled={autostartEnabled}
            onAutostartChange={persistAutostart}
            themeMode={themeMode}
            onThemeModeChange={setThemeMode}
            syncAccount={syncAccount}
            onSyncSaved={setSyncAccount}
          />
        ) : null}
      </section>
    </main>
  );
}

function EditorLoadingState({ bodyMarkdown }: { bodyMarkdown: string }) {
  return (
    <div className="editorShell">
      <textarea
        aria-label="Note body loading"
        className="editorSurface editorLoadingSurface"
        readOnly
        value={bodyMarkdown}
      />
      {hasTransientImageSource(bodyMarkdown) ? (
        <div className="editorHint">Uploading image...</div>
      ) : null}
    </div>
  );
}

function SettingsPanel({
  onClose,
  shortcut,
  onShortcutChange,
  onShortcutSave,
  autostartEnabled,
  onAutostartChange,
  themeMode,
  onThemeModeChange,
  syncAccount,
  onSyncSaved,
}: {
  onClose: () => void;
  shortcut: string;
  onShortcutChange: (value: string) => void;
  onShortcutSave: (value: string) => void;
  autostartEnabled: boolean;
  onAutostartChange: (value: boolean) => void;
  themeMode: ThemeMode;
  onThemeModeChange: (value: ThemeMode) => void;
  syncAccount: SyncAccountState | null;
  onSyncSaved: (state: SyncAccountState) => void;
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

        <div className="settingsGroup">
          <div className="settingsGroupTitle">General</div>
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

          <label className="settingsToggle">
            <span>Start at login</span>
            <input
              checked={autostartEnabled}
              onChange={(event) => onAutostartChange(event.target.checked)}
              type="checkbox"
            />
          </label>
        </div>

        <div className="settingsGroup">
          <div className="settingsGroupTitle">Appearance</div>
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
        </div>

        <div className="settingsGroup">
          <div className="settingsGroupTitle">Sync</div>
          <div className="settingsField">
            <span>Status</span>
            <div className="settingsSyncStatus">{syncStatusLabel(syncAccount)}</div>
            <SyncSettings initial={syncAccount} onSaved={onSyncSaved} />
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

function pointerPositionFromEvent(event?: MouseEvent<HTMLElement>) {
  return event ? { x: event.screenX, y: event.screenY } : undefined;
}

function IconButton({
  active = false,
  danger = false,
  disabled = false,
  label,
  onClick,
  variant = "default",
  children,
}: {
  active?: boolean;
  danger?: boolean;
  disabled?: boolean;
  label: string;
  onClick: (event: MouseEvent<HTMLButtonElement>) => void;
  variant?: "default" | "floating";
  children: ReactNode;
}) {
  const className = [
    "iconButton",
    active ? "iconButtonActive" : "",
    danger ? "danger" : "",
    variant === "floating" ? "floatingIconButton" : "",
  ].filter(Boolean).join(" ");

  return (
    <button aria-label={label} className={className} disabled={disabled} onClick={(event) => {
      event.stopPropagation();
      onClick(event);
    }} title={label} type="button">
      {children}
    </button>
  );
}

function LogoIcon() {
  return (
    <svg className="logoMark" viewBox="0 0 32 32" aria-hidden="true">
      <path d="M9 5.5h11l5 5v16H9z" />
      <path d="M20 5.5v5h5" />
      <path d="M13 15h8M11 20h10M13 24h6" />
      <path d="M6 16h3M5 21h4" />
    </svg>
  );
}

function PlusIcon() {
  return <svg className="flatIcon" viewBox="0 0 24 24" aria-hidden="true"><path d="M12 5v14M5 12h14" /></svg>;
}

function SettingsIcon() {
  return (
    <svg className="gearIcon" viewBox="0 0 24 24" aria-hidden="true">
      <path d="M10.6 3.5h2.8l.45 2.15c.45.16.88.34 1.28.58l1.86-1.18 1.98 1.98-1.18 1.86c.24.4.42.83.58 1.28l2.15.45v2.8l-2.15.45c-.16.45-.34.88-.58 1.28l1.18 1.86-1.98 1.98-1.86-1.18c-.4.24-.83.42-1.28.58l-.45 2.15h-2.8l-.45-2.15a6.7 6.7 0 0 1-1.28-.58l-1.86 1.18-1.98-1.98 1.18-1.86a6.7 6.7 0 0 1-.58-1.28l-2.15-.45v-2.8l2.15-.45c.16-.45.34-.88.58-1.28L5.05 7.03l1.98-1.98 1.86 1.18c.4-.24.83-.42 1.28-.58l.43-2.15z" />
      <path d="M9.3 12a2.7 2.7 0 1 0 5.4 0 2.7 2.7 0 0 0-5.4 0z" />
    </svg>
  );
}

function ListIcon() {
  return <svg className="flatIcon" viewBox="0 0 24 24" aria-hidden="true"><path d="M7 6.5h10" /><path d="M7 12h10" /><path d="M7 17.5h10" /><path d="M4 6.5h.1M4 12h.1M4 17.5h.1" /></svg>;
}

function PinIcon() {
  return <svg className="pinIcon" viewBox="0 0 24 24" aria-hidden="true"><path d="M9.2 4.5h5.6" /><path d="M10 4.5v5.1L6.9 14h10.2L14 9.6V4.5" /><path d="M12 14v6" /></svg>;
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

function MoreIcon() {
  return <svg className="flatIcon" viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12h.1M12 12h.1M19 12h.1" /></svg>;
}

function SourceModeIcon() {
  return <svg className="flatIcon" viewBox="0 0 24 24" aria-hidden="true"><path d="M9 7l-4 5 4 5" /><path d="M15 7l4 5-4 5" /><path d="M13 5l-2 14" /></svg>;
}

function PreviewModeIcon() {
  return <svg className="flatIcon" viewBox="0 0 24 24" aria-hidden="true"><path d="M3.5 12s3.5-6.5 8.5-6.5 8.5 6.5 8.5 6.5-3.5 6.5-8.5 6.5S3.5 12 3.5 12Z" /><path d="M12 9.2a2.8 2.8 0 1 0 0 5.6 2.8 2.8 0 0 0 0-5.6Z" /></svg>;
}
