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
import { conflictPromptText, deleteConfirmationFor, sortNotes, upsertNote } from "../../features/sync/session";
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
  const [promptedConflictIds, setPromptedConflictIds] = useState<string[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [windowReadyToReveal, setWindowReadyToReveal] = useState(false);
  const searchQueryRef = useRef("");
  const searchMountedRef = useRef(false);
  const quietRefreshTimerRef = useRef<number | null>(null);

  const refreshNotes = useCallback(async (quiet = false) => {
    try {
      const startedAt = performance.now();
      setError(null);
      if (!quiet) {
        setStatus("Loading");
      }
      const currentSearchQuery = searchQueryRef.current;
      const trimmedSearchQuery = currentSearchQuery.trim();
      const payload = trimmedSearchQuery ? null : await api.listNotesPayload();
      const nextNotes = trimmedSearchQuery
        ? await api.searchNotes(trimmedSearchQuery)
        : payload?.notes ?? [];
      if (payload) {
        setDataDir(payload.data_dir);
      } else if (dataDir === null) {
        setDataDir(await api.getDataDir());
      }
      startupLog("list_notes_loaded", {
        quiet,
        search: trimmedSearchQuery.length > 0,
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
  }, [dataDir]);

  const scheduleQuietRefresh = useCallback(() => {
    if (quietRefreshTimerRef.current !== null) {
      window.clearTimeout(quietRefreshTimerRef.current);
    }

    quietRefreshTimerRef.current = window.setTimeout(() => {
      quietRefreshTimerRef.current = null;
      void refreshNotes(true);
    }, 400);
  }, [refreshNotes]);

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
      if (event.payload.pulled > 0 || event.payload.conflicts > 0 || event.payload.has_conflicts) {
        scheduleQuietRefresh();
      }
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
  }, [scheduleQuietRefresh]);

  useEffect(() => {
    let unlistenSaved: (() => void) | null = null;
    let unlistenDeleted: (() => void) | null = null;
    const refreshWhenVisible = () => {
      if (document.visibilityState === "visible") {
        scheduleQuietRefresh();
      }
    };

    void listen("note-saved", scheduleQuietRefresh).then((unlisten) => {
      unlistenSaved = unlisten;
    });
    void listen("note-deleted", scheduleQuietRefresh).then((unlisten) => {
      unlistenDeleted = unlisten;
    });

    window.addEventListener("focus", scheduleQuietRefresh);
    document.addEventListener("visibilitychange", refreshWhenVisible);

    return () => {
      if (quietRefreshTimerRef.current !== null) {
        window.clearTimeout(quietRefreshTimerRef.current);
        quietRefreshTimerRef.current = null;
      }
      window.removeEventListener("focus", scheduleQuietRefresh);
      document.removeEventListener("visibilitychange", refreshWhenVisible);
      unlistenSaved?.();
      unlistenDeleted?.();
    };
  }, [scheduleQuietRefresh]);

  const visibleNotes = useMemo(() => sortNotes(notes), [notes]);
  const pendingConflict = useMemo(
    () => visibleNotes.find((note) => note.is_conflict_copy && !promptedConflictIds.includes(note.id)) ?? null,
    [promptedConflictIds, visibleNotes],
  );
  const conflictToPrompt = pendingImportCount > 0 ? null : pendingConflict;

  useEffect(() => {
    searchQueryRef.current = searchQuery;
    if (!searchMountedRef.current) {
      searchMountedRef.current = true;
      return;
    }
    scheduleQuietRefresh();
  }, [scheduleQuietRefresh, searchQuery]);

  useEffect(() => {
    if (!windowReadyToReveal) return;

    void revealCurrentWindowWhenReady();
  }, [windowReadyToReveal]);

  async function persistShortcut(nextShortcut: string): Promise<boolean> {
    try {
      await api.setOpenShortcut(nextShortcut);
      setShortcut(nextShortcut);
      return true;
    } catch {
      return false;
    }
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
      if (report.pulled > 0 || report.conflicts > 0 || report.has_conflicts) {
        await refreshNotes(true);
      }
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
    scheduleQuietRefresh();
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

  async function handleKeepServerVersion(conflict: NoteSummary) {
    try {
      setError(null);
      const nextNotes = await api.deleteNote(conflict.id);
      setNotes(nextNotes);
    } catch (err) {
      setError(String(err));
      setStatus("Error");
    }
  }

  async function handleKeepLocalVersion(conflict: NoteSummary) {
    if (!conflict.source_note_id) return;

    try {
      setError(null);
      const local = await api.getNote(conflict.id);
      const saved = await api.saveNote(conflict.source_note_id, local.title, local.content_md, local.pinned ?? false);
      const afterDelete = await api.deleteNote(conflict.id);
      setNotes(upsertNote(afterDelete, {
        id: saved.id,
        title: saved.title,
        preview: saved.content_md,
        preview_md: saved.content_md,
        pinned: saved.pinned,
        updated_at: saved.updated_at,
        is_conflict_copy: saved.is_conflict_copy,
        source_note_id: saved.source_note_id,
        owner_account_id: saved.owner_account_id,
      }));
      void api.syncNow().catch(() => undefined);
    } catch (err) {
      setError(String(err));
      setStatus("Error");
    }
  }

  function markConflictPrompted(conflictId: string) {
    setPromptedConflictIds((current) => current.includes(conflictId) ? current : [...current, conflictId]);
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
          onShortcutSave={persistShortcut}
          autostartEnabled={autostartEnabled}
          onAutostartChange={persistAutostart}
          themeMode={themeMode}
          onThemeModeChange={setThemeMode}
          syncAccount={syncAccount}
          onSyncSaved={handleSyncSaved}
        />
      ) : null}

      {conflictToPrompt ? (
        <div className="connectionDialogBackdrop">
          <div className="connectionDialog" role="dialog" aria-modal="true">
            <div className="connectionDialogTitle">发现冲突版本</div>
            <div className="connectionDialogSub">
              {conflictPromptText(conflictToPrompt.title)}
            </div>
            <div className="connectionDialogActions conflictDialogActions">
              <button type="button" onClick={() => markConflictPrompted(conflictToPrompt.id)}>稍后</button>
              <button type="button" onClick={() => void handleKeepServerVersion(conflictToPrompt)}>保存服务器版</button>
              <button type="button" onClick={() => void handleKeepLocalVersion(conflictToPrompt)}>保存本地版</button>
            </div>
          </div>
        </div>
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
