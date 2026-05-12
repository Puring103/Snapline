import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { EditorPane } from "../components/EditorPane";
import { MarkdownPreview } from "../components/MarkdownPreview";
import { importPromptText, SyncSettings } from "../components/SyncSettings";
import { api } from "../platform/api";
import { ChevronDownIcon, IconButton, ListIcon, LogoIcon, PinIcon, PlusIcon, PreviewModeIcon, SettingsIcon, SourceModeIcon, ThemeDarkIcon, ThemeLightIcon, ThemeSystemIcon } from "../components/app/AppIcons";
import { SearchIcon } from "./icons";
import { HighlightedText } from "../features/search/highlight";
import { conflictPromptText } from "../features/sync/session";
import { loadLastNoteId, loadTheme, saveLastNoteId, saveTheme } from "./storage";
import type { EditorMode, Note, ThemeMode } from "./types";
import type { LoginSyncResult, Note as StoredNote, NoteSummary, SavedAsset, SyncAccountState } from "../types";
import "./mobile.css";

function nowId() {
  return crypto.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function blankDraft(): Note {
  return {
    id: `draft-${nowId()}`,
    title: "Untitled",
    body: "",
    pinned: false,
    updatedAt: Date.now(),
  };
}

function draftFromStored(note: StoredNote): Note {
  return {
    id: note.id,
    title: note.title,
    body: note.content_md,
    pinned: note.pinned ?? false,
    updatedAt: Date.parse(note.updated_at),
    isConflictCopy: note.is_conflict_copy ?? false,
    sourceNoteId: note.source_note_id ?? null,
    ownerAccountId: note.owner_account_id ?? null,
  };
}

function noteFromSummary(summary: NoteSummary): Note {
  return {
    id: summary.id,
    title: summary.title,
    body: summary.preview_md ?? summary.preview ?? "",
    pinned: summary.pinned ?? false,
    updatedAt: Date.parse(summary.updated_at),
    isConflictCopy: summary.is_conflict_copy ?? false,
    sourceNoteId: summary.source_note_id ?? null,
    ownerAccountId: summary.owner_account_id ?? null,
  };
}

function hasMeaningfulContent(note: Note | null) {
  if (!note) return false;
  return note.title.trim() !== "" && note.title.trim() !== "Untitled" || note.body.trim() !== "";
}

function sortNotes(notes: Note[]) {
  return [...notes].sort((a, b) => Number(b.pinned) - Number(a.pinned) || b.updatedAt - a.updatedAt);
}

function formatTime(timestamp: number) {
  const delta = Math.max(0, Date.now() - timestamp);
  const minutes = Math.floor(delta / 60000);
  if (minutes < 1) return "刚刚";
  if (minutes < 60) return `${minutes} 分钟前`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} 小时前`;
  return new Intl.DateTimeFormat("zh-CN", { month: "short", day: "numeric" }).format(timestamp);
}

function useKeyboardOffset() {
  const [offset, setOffset] = useState(0);

  useEffect(() => {
    const viewport = window.visualViewport;

    function updateOffset() {
      if (!viewport) {
        setOffset(0);
        return;
      }

      const layoutHeight = Math.max(
        document.documentElement.clientHeight,
        window.innerHeight,
      );
      const visualBottom = viewport.height + viewport.offsetTop;
      const viewportHiddenHeight = Math.max(0, layoutHeight - visualBottom);
      setOffset(viewportHiddenHeight > 24 ? Math.round(viewportHiddenHeight) : 0);
    }

    function settleOffset() {
      updateOffset();
      window.setTimeout(updateOffset, 80);
      window.setTimeout(updateOffset, 220);
    }

    updateOffset();
    viewport?.addEventListener("resize", updateOffset);
    viewport?.addEventListener("scroll", updateOffset);
    window.addEventListener("scroll", updateOffset, true);
    window.addEventListener("resize", updateOffset);
    window.addEventListener("orientationchange", settleOffset);
    document.addEventListener("focusin", settleOffset);
    document.addEventListener("focusout", settleOffset);
    return () => {
      viewport?.removeEventListener("resize", updateOffset);
      viewport?.removeEventListener("scroll", updateOffset);
      window.removeEventListener("scroll", updateOffset, true);
      window.removeEventListener("resize", updateOffset);
      window.removeEventListener("orientationchange", settleOffset);
      document.removeEventListener("focusin", settleOffset);
      document.removeEventListener("focusout", settleOffset);
    };
  }, []);

  return offset;
}

export function AndroidApp() {
  const [notes, setNotes] = useState<Note[]>([]);
  const [theme, setTheme] = useState<ThemeMode>(loadTheme);
  const [resolvedTheme, setResolvedTheme] = useState<"dark" | "light">("dark");
  const [editorMode, setEditorMode] = useState<EditorMode>("preview");
  const [query, setQuery] = useState("");
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [settingsExpanded, setSettingsExpanded] = useState(false);
  const [syncAccount, setSyncAccount] = useState<SyncAccountState | null>(null);
  const [pendingImportCount, setPendingImportCount] = useState(0);
  const [promptedConflictIds, setPromptedConflictIds] = useState<string[]>([]);
  const keyboardOffset = useKeyboardOffset();
  const [isSaving, setIsSaving] = useState(false);
  const saveQueueRef = useRef(Promise.resolve());
  const syncQueueRef = useRef(Promise.resolve());
  const autoSyncTimerRef = useRef<number | null>(null);
  const materializedDraftRef = useRef<{ tempId: string; savedId: string } | null>(null);

  const [draft, setDraft] = useState<Note>(() => blankDraft());

  const visibleNotes = useMemo(() => {
    const needle = query.trim().toLowerCase();
    const sorted = sortNotes(notes);
    if (!needle) return sorted;
    return sorted.filter((note) => `${note.title}\n${note.body}`.toLowerCase().includes(needle));
  }, [notes, query]);
  const pendingConflict = useMemo(
    () => visibleNotes.find((note) => note.isConflictCopy && !promptedConflictIds.includes(note.id)) ?? null,
    [promptedConflictIds, visibleNotes],
  );
  const conflictToPrompt = pendingImportCount > 0 ? null : pendingConflict;

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");

    function applyTheme() {
      const nextTheme = theme === "system" ? (media.matches ? "dark" : "light") : theme;
      document.documentElement.dataset.mobileTheme = nextTheme;
      document.documentElement.dataset.mobileThemeMode = theme;
      setResolvedTheme(nextTheme);
    }

    applyTheme();
    saveTheme(theme);
    media.addEventListener("change", applyTheme);
    return () => media.removeEventListener("change", applyTheme);
  }, [theme]);

  useEffect(() => {
    void api.getSyncAccountState().then(setSyncAccount).catch(() => setSyncAccount(null));
  }, []);

  useEffect(() => {
    saveLastNoteId(hasMeaningfulContent(draft) ? draft.id : null);
  }, [draft]);

  const refreshNotes = useCallback(async (preferredId?: string | null) => {
    materializedDraftRef.current = null;
    const summaries = await api.searchNotes("");
    const nextNotes = sortNotes(summaries.map(noteFromSummary).filter(hasMeaningfulContent));
    setNotes(nextNotes);

    const targetId = preferredId ?? loadLastNoteId();
    const target = targetId ? nextNotes.find((note) => note.id === targetId) : null;
    const summary = target ?? nextNotes[0] ?? null;
    if (!summary) {
      setDraft(blankDraft());
      return;
    }

    const stored = await api.getNote(summary.id);
    setDraft(draftFromStored(stored));
  }, []);

  useEffect(() => {
    void refreshNotes().catch(() => undefined);
  }, [refreshNotes]);

  useEffect(() => {
    return () => {
      if (autoSyncTimerRef.current !== null) {
        window.clearTimeout(autoSyncTimerRef.current);
      }
    };
  }, []);

  async function persistDraft(next: Note) {
    setDraft(next);
    setNotes((current) => {
      const without = current.filter((note) => note.id !== next.id);
      if (!hasMeaningfulContent(next)) return sortNotes(without);
      return sortNotes([next, ...without]);
    });
    if (!hasMeaningfulContent(next)) return;
    setIsSaving(true);
    saveQueueRef.current = saveQueueRef.current.then(async () => {
      const materialized = materializedDraftRef.current;
      const saveId = next.id.startsWith("draft-")
        ? materialized?.tempId === next.id ? materialized.savedId : null
        : next.id;
      const result = await api.saveDraftSession({
        id: saveId,
        title: next.title,
        body_md: next.body,
        pinned: next.pinned,
      });
      if (result.note) {
        const saved = draftFromStored(result.note);
        if (next.id.startsWith("draft-")) {
          materializedDraftRef.current = { tempId: next.id, savedId: saved.id };
        }
        setDraft(saved);
        setNotes((current) => sortNotes([saved, ...current.filter((note) => note.id !== saved.id && note.id !== next.id)]));
        saveLastNoteId(saved.id);
        scheduleAutoSync();
      }
    }).finally(() => {
      setIsSaving(false);
    });
  }

  function scheduleAutoSync() {
    if (!syncAccount?.is_logged_in) return;

    if (autoSyncTimerRef.current !== null) {
      window.clearTimeout(autoSyncTimerRef.current);
    }

    autoSyncTimerRef.current = window.setTimeout(() => {
      autoSyncTimerRef.current = null;
      queueAutoSync();
    }, 900);
  }

  function queueAutoSync() {
    syncQueueRef.current = syncQueueRef.current
      .catch(() => undefined)
      .then(() => api.syncNow())
      .then(() => undefined)
      .catch(() => undefined);
  }

  function updateDraft(patch: Partial<Note>) {
    void persistDraft({ ...draft, ...patch, updatedAt: Date.now() });
  }

  function createNote() {
    const next = blankDraft();
    materializedDraftRef.current = null;
    setDraft(next);
    setEditorMode("preview");
    setSidebarOpen(false);
  }

  function openNote(note: Note) {
    materializedDraftRef.current = null;
    void api.getNote(note.id).then((stored) => setDraft(draftFromStored(stored))).catch(() => setDraft(note));
    setEditorMode("preview");
    setSidebarOpen(false);
  }

  function deleteDraft() {
    if (!draft.id.startsWith("draft-")) {
      void api.deleteNote(draft.id).then(() => refreshNotes(null)).catch(() => undefined);
    } else {
      setNotes((current) => current.filter((note) => note.id !== draft.id));
      const next = notes.filter((note) => note.id !== draft.id)[0] ?? blankDraft();
      setDraft(next);
    }
    setEditorMode("preview");
  }

  function togglePinned(note: Note) {
    const nextPinned = !note.pinned;
    if (note.id === draft.id) {
      updateDraft({ pinned: nextPinned });
      return;
    }
    setNotes((current) => sortNotes(current.map((item) => (
      item.id === note.id ? { ...item, pinned: nextPinned, updatedAt: Date.now() } : item
    ))));
    if (!note.id.startsWith("draft-")) void api.setNotePinned(note.id, nextPinned).catch(() => undefined);
  }

  function handleSyncSaved(result: LoginSyncResult) {
    setSyncAccount(result.account);
    setPendingImportCount(result.anonymous_note_count);
    if (result.account.is_logged_in) {
      void api.syncNow().catch(() => undefined).finally(() => {
        void refreshNotes().catch(() => undefined);
      });
      return;
    }
    void refreshNotes().catch(() => undefined);
  }

  async function handleImportAnonymousNotes() {
    try {
      const imported = await api.importAnonymousNotes();
      setNotes(sortNotes(imported.map(noteFromSummary).filter(hasMeaningfulContent)));
      setPendingImportCount(0);
      void api.syncNow().catch(() => undefined);
    } catch {
      setPendingImportCount(0);
    }
  }

  async function handleKeepServerVersion(conflict: Note) {
    try {
      const nextNotes = await api.deleteNote(conflict.id);
      setNotes(sortNotes(nextNotes.map(noteFromSummary).filter(hasMeaningfulContent)));
      if (draft.id === conflict.id) {
        void refreshNotes(conflict.sourceNoteId ?? null).catch(() => undefined);
      }
    } catch {
      markConflictPrompted(conflict.id);
    }
  }

  async function handleKeepLocalVersion(conflict: Note) {
    if (!conflict.sourceNoteId) return;

    try {
      const local = await api.getNote(conflict.id);
      const saved = await api.saveNote(conflict.sourceNoteId, local.title, local.content_md, local.pinned ?? false);
      const nextNotes = await api.deleteNote(conflict.id);
      const savedDraft = draftFromStored(saved);
      setNotes(sortNotes([
        savedDraft,
        ...nextNotes.map(noteFromSummary).filter((note) => note.id !== saved.id && hasMeaningfulContent(note)),
      ]));
      if (draft.id === conflict.id || draft.id === conflict.sourceNoteId) {
        setDraft(savedDraft);
      }
      void api.syncNow().catch(() => undefined);
    } catch {
      markConflictPrompted(conflict.id);
    }
  }

  function markConflictPrompted(conflictId: string) {
    setPromptedConflictIds((current) => current.includes(conflictId) ? current : [...current, conflictId]);
  }

  async function handleRequestImageSave(bytes: number[]): Promise<SavedAsset | null> {
    let noteId = draft.id;
    if (noteId.startsWith("draft-")) {
      const result = await api.saveDraftSession({
        id: null,
        title: draft.title,
        body_md: draft.body,
        pinned: draft.pinned,
      });
      if (!result.note) return null;
      const saved = draftFromStored(result.note);
      noteId = saved.id;
      setDraft(saved);
      setNotes((current) => sortNotes([saved, ...current.filter((note) => note.id !== draft.id && note.id !== saved.id)]));
    }
    return api.savePngAsset(noteId, bytes);
  }

  return (
    <div className="mobileRoot" data-mobile-theme={resolvedTheme} data-theme-mode={theme}>
    <div className="phoneApp">
      <main className="screen">
        <section className="editorView">
          <header className="editorTopBar">
            <IconButton label="打开列表" onClick={() => setSidebarOpen(true)}><ListIcon /></IconButton>
            <input
              className="titleInput"
              value={draft.title}
              onChange={(event) => updateDraft({ title: event.target.value })}
              onBlur={() => {
                if (!draft.title.trim()) updateDraft({ title: "Untitled" });
              }}
              placeholder="Untitled"
            />
            <IconButton label={editorMode === "preview" ? "源码" : "预览"} onClick={() => setEditorMode((mode) => mode === "preview" ? "source" : "preview")}>
              {editorMode === "preview" ? <SourceModeIcon /> : <PreviewModeIcon />}
            </IconButton>
            <IconButton label="新建笔记" onClick={createNote}><PlusIcon /></IconButton>
          </header>

          <section
            className="mobileEditorHost"
            style={{
              "--keyboard-offset": `${keyboardOffset}px`,
            } as CSSProperties}
          >
            <EditorPane
              bodyMarkdown={draft.body}
              mode={editorMode}
              onBodyChange={(body) => updateDraft({ body })}
              onModeToggle={() => setEditorMode((mode) => mode === "preview" ? "source" : "preview")}
              onRequestImageSave={handleRequestImageSave}
              readOnly={false}
              showModeToggle={false}
              toolbarVariant="full"
            />
          </section>
        </section>
      </main>

      {sidebarOpen ? (
        <div className="drawerBackdrop" onClick={() => setSidebarOpen(false)}>
          <aside className="notesDrawer" onClick={(event) => event.stopPropagation()}>
            <header className="drawerHeader">
              <LogoIcon />
              <div className="titleBlock">
                <h1>Snapline</h1>
                <p>{notes.length} 条笔记</p>
              </div>
              <IconButton label="关闭列表" onClick={() => setSidebarOpen(false)}><ListIcon /></IconButton>
            </header>

            <label className="searchBox">
              <SearchIcon />
              <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索笔记" />
            </label>

            <div className="noteFeed">
              {visibleNotes.map((note) => (
                <article
                  className={["noteRow", note.id === draft.id ? "selected" : "", note.pinned ? "pinned" : ""].filter(Boolean).join(" ")}
                  key={note.id}
                >
                  <button className="noteRowOpen" onClick={() => openNote(note)} type="button">
                    <div className="noteRowTitleBlock">
                      <div className="noteRowTitleRow">
                        <span className="noteRowTitle">
                          <HighlightedText query={query} text={note.title.trim() || "Untitled"} />
                        </span>
                        {note.isConflictCopy ? <span className="conflictBadge">冲突</span> : null}
                      </div>
                      <span className="noteRowTime">{formatTime(note.updatedAt)}</span>
                    </div>
                    <MarkdownPreview highlightQuery={query} markdown={note.body || "No preview"} />
                  </button>
                  <IconButton active={note.pinned} label={note.pinned ? "取消收藏" : "收藏"} onClick={() => togglePinned(note)}><PinIcon /></IconButton>
                </article>
              ))}
              {visibleNotes.length === 0 ? <div className="emptyState">没有匹配的笔记</div> : null}
            </div>

            <section className="drawerSettings">
              <button
                aria-expanded={settingsExpanded}
                className="drawerSettingsTrigger"
                onClick={() => setSettingsExpanded((open) => !open)}
                type="button"
              >
                <span><SettingsIcon />设置</span>
                <ChevronDownIcon className={settingsExpanded ? "drawerSettingsIcon open" : "drawerSettingsIcon"} />
              </button>
              {settingsExpanded ? (
                <div className="drawerSettingsPanel">
                  <div className="drawerSettingsSubpanel">
                    <h3>账号登录</h3>
                    <SyncSettings
                      initial={syncAccount}
                      onSaved={handleSyncSaved}
                    />
                  </div>
                  <div className="drawerSettingsSubpanel">
                  <h3>外观</h3>
                  <div className="segmented">
                    <button className={theme === "system" ? "selected" : ""} onClick={() => setTheme("system")} type="button"><ThemeSystemIcon />系统</button>
                    <button className={theme === "dark" ? "selected" : ""} onClick={() => setTheme("dark")} type="button"><ThemeDarkIcon />暗色</button>
                    <button className={theme === "light" ? "selected" : ""} onClick={() => setTheme("light")} type="button"><ThemeLightIcon />浅色</button>
                  </div>
                  </div>
                </div>
              ) : null}
            </section>
          </aside>
        </div>
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
              <button type="button" onClick={() => setPendingImportCount(0)}>不导入</button>
              <button type="button" onClick={() => void handleImportAnonymousNotes()}>导入</button>
            </div>
          </div>
        </div>
      ) : null}
    </div>
    </div>
  );
}
