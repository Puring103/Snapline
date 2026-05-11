import { emit, listen } from "@tauri-apps/api/event";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState, type MouseEvent } from "react";
import { SettingsPanel } from "./SettingsPanel";
import {
  CheckIcon,
  CloseIcon,
  ConflictIcon,
  IconButton,
  LogoIcon,
  PlusIcon,
  SettingsIcon,
  StarIcon,
  TrashIcon,
} from "./AppIcons";
import { api } from "../../platform/api";
import { startupLog } from "../../platform/startupLog";
import { openNoteWindow, revealCurrentWindowWhenReady, shouldStartWindowDrag } from "../../platform/window";
import { importPromptText } from "../SyncSettings";
import { useThemeMode } from "../../hooks/theme";
import { deleteConfirmationFor, sortNotes, upsertNote } from "../../features/sync/session";
import { HighlightedText } from "../../features/search/highlight";
import type { LoginSyncResult, NoteSummary, SyncAccountState, SyncReport, SyncStatusState } from "../../types";

const DEFAULT_SHORTCUT = "Ctrl+Shift+Space";
const LazyMarkdownPreview = lazy(() => import("../MarkdownPreview").then((module) => ({ default: module.MarkdownPreview })));

export function NotesListWindow() {
  const [notes, setNotes] = useState<NoteSummary[]>([]);
  const [status, setStatus] = useState("Loading");
  const [error, setError] = useState<string | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [syncAccount, setSyncAccount] = useState<SyncAccountState | null>(null);
  const [syncStatus, setSyncStatus] = useState<SyncStatusState>({ label: "Sync", detail: null });
  const [dataDir, setDataDir] = useState<string | null>(null);
  const [confirmingDeleteId, setConfirmingDeleteId] = useState<string | null>(null);
  const [shortcut, setShortcut] = useState(DEFAULT_SHORTCUT);
  const [autostartEnabled, setAutostartEnabled] = useState(false);
  const [themeMode, setThemeMode] = useThemeMode();
  const [pendingImportCount, setPendingImportCount] = useState(0);
  const [searchQuery, setSearchQuery] = useState("");
  const [windowReadyToReveal, setWindowReadyToReveal] = useState(false);
  const searchQueryRef = useRef("");
  const searchMountedRef = useRef(false);

  const refreshNotes = useCallback(async (quiet = false) => {
    try {
      const startedAt = performance.now();
      setError(null);
      if (!quiet) {
        setStatus("Loading");
      }
      const state = await api.bootstrap();
      setDataDir(state.data_dir);
      const currentSearchQuery = searchQueryRef.current;
      const nextNotes = currentSearchQuery.trim()
        ? await api.searchNotes(currentSearchQuery)
        : state.notes;
      startupLog("list_bootstrap_done", {
        quiet,
        notes: nextNotes.length,
        duration_ms: Math.round(performance.now() - startedAt),
      });
      setNotes(nextNotes);
      setConfirmingDeleteId((current) =>
        current && nextNotes.some((note) => note.id === current) ? current : null,
      );
      setStatus("Ready");
      if (!quiet) {
        setWindowReadyToReveal(true);
      }
    } catch (err) {
      setError(String(err));
      setStatus("Error");
      if (!quiet) {
        setWindowReadyToReveal(true);
      }
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
    void api.getSyncAccountState().then((next) => {
      setSyncAccount(next);
      setSyncStatus({ label: next.is_logged_in ? "Synced" : "Sync", detail: null });
    }).catch(() => setSyncAccount(null));
  }, [refreshNotes]);

  useEffect(() => {
    let unlistenStatus: (() => void) | null = null;
    let unlistenError: (() => void) | null = null;

    void listen<SyncReport>("sync-status", (event) => {
      setSyncStatus({
        label: event.payload.has_conflicts ? "Conflict" : "Synced",
        detail: event.payload.detail,
      });
      void refreshNotes(true);
    }).then((unlisten) => {
      unlistenStatus = unlisten;
    });
    void listen<string>("sync-error", (event) => {
      const label = event.payload.toLowerCase().includes("offline") ? "Offline" : "Error";
      setSyncStatus({ label, detail: event.payload });
    }).then((unlisten) => {
      unlistenError = unlisten;
    });

    return () => {
      unlistenStatus?.();
      unlistenError?.();
    };
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

  useEffect(() => {
    searchQueryRef.current = searchQuery;
    if (!searchMountedRef.current) {
      searchMountedRef.current = true;
      return;
    }
    void refreshNotes(true);
  }, [refreshNotes, searchQuery]);

  useEffect(() => {
    if (!windowReadyToReveal) return;

    void revealCurrentWindowWhenReady();
  }, [windowReadyToReveal]);

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
      await api.setNotePinned(noteId, !(current.pinned ?? false));
      const summary = await api.getNoteSummary(noteId);
      setNotes((existing) => upsertNote(existing, summary));
    } catch (err) {
      setError(String(err));
      setStatus("Error");
    }
  }

  async function handleSyncNow(): Promise<SyncReport> {
    setSyncStatus({ label: "Syncing", detail: null });
    try {
      const report = await api.syncNow();
      setSyncStatus({ label: report.has_conflicts ? "Conflict" : "Synced", detail: report.detail });
      await refreshNotes(true);
      return report;
    } catch (err) {
      const message = String(err);
      const label = message.toLowerCase().includes("offline") ? "Offline" : "Error";
      setSyncStatus({ label, detail: message });
      throw err;
    }
  }

  function handleSyncSaved(result: LoginSyncResult) {
    setSyncAccount(result.account);
    setSyncStatus({ label: result.account.is_logged_in ? "Synced" : "Sync", detail: null });
    setPendingImportCount(result.anonymous_note_count);
    void refreshNotes(true);
  }

  async function handleImportAnonymousNotes() {
    try {
      setError(null);
      const imported = await api.importAnonymousNotes();
      setNotes(imported);
      setPendingImportCount(0);
      void api.syncNow().catch(() => undefined);
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
            <div className="listSub">{status} / {visibleNotes.length} items / {syncStatus.label}</div>
          </div>
        </div>
        <div className="listHeaderActions">
          <IconButton label="New note" onClick={(event) => void handleNewNote(event)}><PlusIcon /></IconButton>
          <IconButton label="Settings" onClick={() => setSettingsOpen(true)}><SettingsIcon /></IconButton>
          <IconButton label="Close window" onClick={() => void getCurrentWindow().close()}><CloseIcon /></IconButton>
        </div>
      </header>

      {error ? <div className="errorBanner">{error}</div> : null}

      <div className="searchBar">
        <input
          aria-label="Search notes"
          className="searchInput"
          placeholder="Search notes"
          type="search"
          value={searchQuery}
          onChange={(event) => setSearchQuery(event.target.value)}
        />
      </div>

      <section className="listPanel" aria-label="Note list">
        {visibleNotes.length === 0 ? (
          <div className="emptyList">{searchQuery.trim() ? "No matching notes." : "No saved notes yet."}</div>
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
                    <div className="noteRowTitleRow">
                      <span className="noteRowTitle">
                        <HighlightedText query={searchQuery} text={note.title} />
                      </span>
                      {note.is_conflict_copy ? (
                        <span className="conflictBadge" title="Conflict copy — your local changes conflicted with a remote update">
                          <ConflictIcon />
                          Conflict
                        </span>
                      ) : null}
                    </div>
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
                <Suspense fallback={<div className="noteRowPreview">{note.preview || "No preview"}</div>}>
                  <LazyMarkdownPreview
                    dataDir={dataDir}
                    highlightQuery={searchQuery}
                    markdown={note.preview_md || note.preview || "No preview"}
                  />
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
          onSyncSaved={handleSyncSaved}
        />
      ) : null}

      {pendingImportCount > 0 ? (
        <div className="connectionDialogBackdrop">
          <div className="connectionDialog" role="dialog" aria-modal="true">
            <div className="connectionDialogTitle">{importPromptText(pendingImportCount)}</div>
            <div className="connectionDialogActions">
              <button type="button" onClick={() => setPendingImportCount(0)}>Do not import</button>
              <button type="button" onClick={() => void handleImportAnonymousNotes()}>Import</button>
            </div>
          </div>
        </div>
      ) : null}
    </main>
  );
}

function pointerPositionFromEvent(event?: MouseEvent<HTMLElement>) {
  return event ? { x: event.screenX, y: event.screenY } : undefined;
}
