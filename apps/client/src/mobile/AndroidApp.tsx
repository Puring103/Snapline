import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { EditorPane } from "../components/EditorPane";
import { SyncSettings } from "../components/SyncSettings";
import { api } from "../platform/api";
import { IconButton, ListIcon, LogoIcon, PinIcon, PlusIcon, PreviewModeIcon, SettingsIcon, SourceModeIcon, ThemeDarkIcon, ThemeLightIcon, ThemeSystemIcon } from "../components/app/AppIcons";
import { SearchIcon } from "./icons";
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
  };
}

function noteFromSummary(summary: NoteSummary): Note {
  return {
    id: summary.id,
    title: summary.title,
    body: summary.preview_md ?? summary.preview ?? "",
    pinned: summary.pinned ?? false,
    updatedAt: Date.parse(summary.updated_at),
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

function previewText(note: Note) {
  return note.body.replace(/[#*_`>-]/g, "").replace(/\s+/g, " ").trim() || "No preview";
}

function useKeyboardOffset() {
  const [offset, setOffset] = useState(0);

  useEffect(() => {
    const viewport = window.visualViewport;
    const initialHeight = Math.max(
      window.screen?.height || 0,
      window.outerHeight || 0,
      window.innerHeight,
      document.documentElement.clientHeight,
    );

    function updateOffset() {
      const activeTag = document.activeElement?.tagName;
      const editing = activeTag === "TEXTAREA" || activeTag === "INPUT" || Boolean(document.activeElement?.closest("[contenteditable='true']"));
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
      const resizedHiddenHeight = editing ? Math.max(0, initialHeight - window.innerHeight) : 0;
      const fallbackKeyboardHeight = editing ? Math.round(window.innerHeight * 0.38) : 0;
      const hiddenHeight = Math.max(viewportHiddenHeight, resizedHiddenHeight, fallbackKeyboardHeight);
      setOffset(hiddenHeight > 24 ? Math.round(hiddenHeight) : 0);
    }

    updateOffset();
    viewport?.addEventListener("resize", updateOffset);
    viewport?.addEventListener("scroll", updateOffset);
    window.addEventListener("resize", updateOffset);
    window.addEventListener("orientationchange", updateOffset);
    document.addEventListener("focusin", updateOffset);
    document.addEventListener("focusout", updateOffset);
    return () => {
      viewport?.removeEventListener("resize", updateOffset);
      viewport?.removeEventListener("scroll", updateOffset);
      window.removeEventListener("resize", updateOffset);
      window.removeEventListener("orientationchange", updateOffset);
      document.removeEventListener("focusin", updateOffset);
      document.removeEventListener("focusout", updateOffset);
    };
  }, []);

  return offset;
}

export function AndroidApp() {
  const [notes, setNotes] = useState<Note[]>([]);
  const [theme, setTheme] = useState<ThemeMode>(loadTheme);
  const [resolvedTheme, setResolvedTheme] = useState<"dark" | "light">("dark");
  const [editorMode, setEditorMode] = useState<EditorMode>("source");
  const [query, setQuery] = useState("");
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [settingsExpanded, setSettingsExpanded] = useState(false);
  const [syncAccount, setSyncAccount] = useState<SyncAccountState | null>(null);
  const keyboardOffset = useKeyboardOffset();
  const [isSaving, setIsSaving] = useState(false);
  const saveQueueRef = useRef(Promise.resolve());
  const materializedDraftRef = useRef<{ tempId: string; savedId: string } | null>(null);

  const [draft, setDraft] = useState<Note>(() => blankDraft());

  const visibleNotes = useMemo(() => {
    const needle = query.trim().toLowerCase();
    const sorted = sortNotes(notes);
    if (!needle) return sorted;
    return sorted.filter((note) => `${note.title}\n${note.body}`.toLowerCase().includes(needle));
  }, [notes, query]);

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
      }
    }).finally(() => {
      setIsSaving(false);
    });
  }

  function updateDraft(patch: Partial<Note>) {
    void persistDraft({ ...draft, ...patch, updatedAt: Date.now() });
  }

  function createNote() {
    const next = blankDraft();
    materializedDraftRef.current = null;
    setDraft(next);
    setEditorMode("source");
    setSidebarOpen(false);
  }

  function openNote(note: Note) {
    materializedDraftRef.current = null;
    void api.getNote(note.id).then((stored) => setDraft(draftFromStored(stored))).catch(() => setDraft(note));
    setEditorMode("source");
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
    setEditorMode("source");
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
    void refreshNotes().catch(() => undefined);
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
              showModeToggle={false}
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
                        <span className="noteRowTitle">{note.title.trim() || "Untitled"}</span>
                      </div>
                      <span className="noteRowTime">{formatTime(note.updatedAt)}</span>
                    </div>
                    <p className="noteRowPreview">{previewText(note)}</p>
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
                <span className="drawerSettingsChevron" aria-hidden="true">{settingsExpanded ? "收起" : "打开"}</span>
              </button>
              {settingsExpanded ? (
                <div className="drawerSettingsPanel">
                  <h3>外观</h3>
                  <div className="segmented">
                    <button className={theme === "system" ? "selected" : ""} onClick={() => setTheme("system")} type="button"><ThemeSystemIcon />系统</button>
                    <button className={theme === "dark" ? "selected" : ""} onClick={() => setTheme("dark")} type="button"><ThemeDarkIcon />暗色</button>
                    <button className={theme === "light" ? "selected" : ""} onClick={() => setTheme("light")} type="button"><ThemeLightIcon />浅色</button>
                  </div>
                  <h3>同步</h3>
                  <SyncSettings
                    initial={syncAccount}
                    onSaved={handleSyncSaved}
                    onSyncNow={async () => {
                      const report = await api.syncNow();
                      await refreshNotes(draft.id.startsWith("draft-") ? null : draft.id);
                      return report;
                    }}
                  />
                </div>
              ) : null}
            </section>
          </aside>
        </div>
      ) : null}
    </div>
    </div>
  );
}
