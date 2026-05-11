import { emit, listen } from "@tauri-apps/api/event";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { lazy, Suspense, useEffect, useMemo, useRef, useState, type MouseEvent } from "react";
import { api } from "../../platform/api";
import { startupLog } from "../../platform/startupLog";
import {
  openListWindow,
  openNoteWindow,
  revealCurrentWindowWhenReady,
  shouldDeferInitialNoteLoad,
  shouldStartWindowDrag,
} from "../../platform/window";
import { webviewAssetUrlFromFilesystemPath } from "../../features/assets/assetUrl";
import { DEFAULT_EDITOR_MODE, toggleEditorMode, type EditorMode } from "../../features/editor/editorMode";
import {
  createDraftSession,
  hasMeaningfulDraftContent,
  isSessionDirty,
  type ActiveSession,
} from "../../features/sync/session";
import type { LoginSyncResult, Note, SavedAsset, SyncAccountState } from "../../types";
import { SettingsPanel } from "./SettingsPanel";
import {
  CloseIcon,
  ConflictIcon,
  IconButton,
  ListIcon,
  MoreIcon,
  PinIcon,
  PlusIcon,
  SettingsIcon,
} from "./AppIcons";
import { useThemeMode } from "../../hooks/theme";

const DEFAULT_SHORTCUT = "Ctrl+Shift+Space";
const FOCUS_EDITOR_EVENT = "snapline-focus-editor";
const LazyEditorPane = lazy(() => import("../EditorPane").then((module) => ({ default: module.EditorPane })));

export function NoteEditorWindow({ newDraft, noteId }: { newDraft: boolean; noteId: string | null }) {
  const windowLabel = useMemo(() => getCurrentWindow().label, []);
  const [session, setSession] = useState<ActiveSession>(() => createDraftSession());
  const [notePinned, setNotePinned] = useState(false);
  const [windowAlwaysOnTop, setWindowAlwaysOnTop] = useState(false);
  const [status, setStatus] = useState("Loading");
  const [error, setError] = useState<string | null>(null);
  const [deleted, setDeleted] = useState(false);
  const [focusRequestId, setFocusRequestId] = useState(0);
  const [reloadRequestId, setReloadRequestId] = useState(0);
  const [noteLoadArmed, setNoteLoadArmed] = useState(noteId !== null || newDraft);
  const [editorMode, setEditorMode] = useState<EditorMode>(DEFAULT_EDITOR_MODE);
  const [chromeMenuOpen, setChromeMenuOpen] = useState(false);
  const [dataDir, setDataDir] = useState<string | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [syncAccount, setSyncAccount] = useState<SyncAccountState | null>(null);
  const [shortcut, setShortcut] = useState(DEFAULT_SHORTCUT);
  const [autostartEnabled, setAutostartEnabled] = useState(false);
  const [themeMode, setThemeMode] = useThemeMode();
  const [isConflictCopy, setIsConflictCopy] = useState(false);
  const [sourceNoteId, setSourceNoteId] = useState<string | null>(null);
  const [windowReadyToReveal, setWindowReadyToReveal] = useState(false);

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

    if (noteId !== null || newDraft) {
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
          setWindowReadyToReveal(false);
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
  }, [newDraft, noteId, windowLabel]);

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
          await beginSessionFromNote(next);
        } else if (newDraft) {
          beginDraftSession();
        } else {
          await beginSessionFromNote(bootstrapState.current);
        }
        setStatus("Draft");
        setWindowReadyToReveal(true);
      } catch (err) {
        if (!cancelled) {
          setError(String(err));
          setStatus("Error");
          setWindowReadyToReveal(true);
        }
      }
    }

    void loadNote();

    return () => {
      cancelled = true;
      clearSaveTimer();
    };
  }, [newDraft, noteId, noteLoadArmed, reloadRequestId]);

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
          setWindowReadyToReveal(false);
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
      setWindowReadyToReveal(false);
      setNoteLoadArmed(true);
      setReloadRequestId((current) => current + 1);
      setFocusRequestId((current) => current + 1);
    }).then((nextUnlisten) => {
      unlisten = nextUnlisten;
    });

    return () => {
      unlisten?.();
    };
  }, [noteId]);

  useEffect(() => {
    if (!windowReadyToReveal) return;

    void revealCurrentWindowWhenReady();
  }, [windowReadyToReveal]);

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

    if (session.bodyMd.includes("blob:") || session.bodyMd.includes("data:image/")) {
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
  }, [deleted, session]);

  async function beginSessionFromNote(note: Note) {
    clearSaveTimer();
    const { title, body_md } = await api.splitStoredNoteMarkdown(note.title, note.content_md);
    setSession({
      kind: noteId ? "existing" : "draft",
      id: note.id,
      title,
      bodyMd: body_md,
      persistedTitle: title,
      persistedBodyMd: body_md,
    });
    setNotePinned(note.pinned ?? false);
    setDeleted(false);
    setError(null);
    setStatus("Draft");
    setIsConflictCopy(note.is_conflict_copy ?? false);
    setSourceNoteId(note.source_note_id ?? null);
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

      const result = await api.saveDraftSession({
        id: snapshot.id,
        title: snapshot.title,
        body_md: snapshot.bodyMd,
        pinned: notePinnedRef.current,
      });

      if (result.skipped || !result.note) {
        setStatus("Draft");
        return null;
      }

      const saved = result.note;
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

  async function handleCreateNoteWindow(event?: MouseEvent<HTMLElement>) {
    try {
      setError(null);
      await openNoteWindow(null, pointerPositionFromEvent(event));
    } catch (err) {
      setError(String(err));
      setStatus("Error");
    }
  }

  function handleOpenListWindow() {
    void openListWindow();
  }

  function handleOpenSettings() {
    setSettingsOpen(true);
  }

  function handleSyncSaved(result: LoginSyncResult) {
    setSyncAccount(result.account);
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
                  <button onClick={(event) => handleChromeMenuAction(() => void handleCreateNoteWindow(event))} role="menuitem" type="button">
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
          {isConflictCopy ? (
            <div className="conflictBanner">
              <ConflictIcon />
              <span>This is a conflict copy — your local changes conflicted with a remote update.</span>
              {sourceNoteId ? (
                <button
                  className="linkButton"
                  onClick={() => void openNoteWindow(sourceNoteId)}
                  type="button"
                >
                  View original
                </button>
              ) : null}
            </div>
          ) : null}
          <Suspense fallback={<EditorPaneFallback bodyMarkdown={session.bodyMd} readOnly={deleted} onBodyChange={handleBodyChange} />}>
            <LazyEditorPane
              bodyMarkdown={session.bodyMd}
              dataDir={dataDir}
              focusRequestId={focusRequestId}
              mode={editorMode}
              onBodyChange={handleBodyChange}
              onModeToggle={() => setEditorMode((mode) => toggleEditorMode(mode))}
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
            onSyncSaved={handleSyncSaved}
          />
        ) : null}
      </section>
    </main>
  );
}

function pointerPositionFromEvent(event?: MouseEvent<HTMLElement>) {
  return event ? { x: event.screenX, y: event.screenY } : undefined;
}

function EditorPaneFallback({
  bodyMarkdown,
  onBodyChange,
  readOnly,
}: {
  bodyMarkdown: string;
  onBodyChange: (bodyMarkdown: string) => void;
  readOnly: boolean;
}) {
  return (
    <div className="editorShell">
      <textarea
        aria-label="Note body source"
        className="editorSurface editorLoadingSurface editorSourceSurface"
        onChange={(event) => onBodyChange(event.target.value)}
        placeholder="Write before the thought fades..."
        readOnly={readOnly}
        spellCheck={false}
        value={bodyMarkdown}
      />
    </div>
  );
}
