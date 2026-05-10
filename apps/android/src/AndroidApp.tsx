import { useEffect, useMemo, useState } from "react";
import {
  BoldIcon,
  BlockquoteIcon,
  CodeBlockIcon,
  HeadingIcon,
  IconButton,
  InlineCodeIcon,
  ItalicIcon,
  ListIcon,
  LogoIcon,
  MenuIcon,
  OrderedListIcon,
  PinIcon,
  PlusIcon,
  PreviewIcon,
  SearchIcon,
  SettingsIcon,
  SourceIcon,
  SyncIcon,
  TaskListIcon,
  ThemeDarkIcon,
  ThemeLightIcon,
  ThemeSystemIcon,
} from "./icons";
import { loadLastNoteId, loadNotes, loadTheme, saveLastNoteId, saveNotes, saveTheme } from "./storage";
import type { EditorMode, Note, ThemeMode } from "./types";

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

function renderMarkdown(markdown: string) {
  return markdown
    .split(/\n{2,}/)
    .map((block) => block.trim())
    .filter(Boolean)
    .map((block, index) => {
      if (block.startsWith("# ")) return <h1 key={index}>{block.slice(2)}</h1>;
      if (block.startsWith("## ")) return <h2 key={index}>{block.slice(3)}</h2>;
      if (block.startsWith("- ")) {
        return (
          <ul key={index}>
            {block.split("\n").map((item, itemIndex) => (
              <li key={itemIndex}>{item.replace(/^- /, "")}</li>
            ))}
          </ul>
        );
      }
      return <p key={index}>{block}</p>;
    });
}

function useKeyboardOffset() {
  const [offset, setOffset] = useState(0);

  useEffect(() => {
    const viewport = window.visualViewport;
    if (!viewport) return;
    const activeViewport = viewport;

    function updateOffset() {
      const hiddenHeight = Math.max(0, window.innerHeight - activeViewport.height - activeViewport.offsetTop);
      setOffset(hiddenHeight);
    }

    updateOffset();
    activeViewport.addEventListener("resize", updateOffset);
    activeViewport.addEventListener("scroll", updateOffset);
    return () => {
      activeViewport.removeEventListener("resize", updateOffset);
      activeViewport.removeEventListener("scroll", updateOffset);
    };
  }, []);

  return offset;
}

export function AndroidApp() {
  const [notes, setNotes] = useState<Note[]>(() => sortNotes(loadNotes().filter(hasMeaningfulContent)));
  const [theme, setTheme] = useState<ThemeMode>(loadTheme);
  const [resolvedTheme, setResolvedTheme] = useState<"dark" | "light">("dark");
  const [editorMode, setEditorMode] = useState<EditorMode>("source");
  const [query, setQuery] = useState("");
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const keyboardOffset = useKeyboardOffset();

  const [draft, setDraft] = useState<Note>(() => {
    const saved = sortNotes(loadNotes().filter(hasMeaningfulContent));
    const lastId = loadLastNoteId();
    return saved.find((note) => note.id === lastId) ?? saved[0] ?? blankDraft();
  });

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
      document.documentElement.dataset.theme = nextTheme;
      document.documentElement.dataset.themeMode = theme;
      setResolvedTheme(nextTheme);
    }

    applyTheme();
    saveTheme(theme);
    media.addEventListener("change", applyTheme);
    return () => media.removeEventListener("change", applyTheme);
  }, [theme]);

  useEffect(() => {
    saveNotes(sortNotes(notes.filter(hasMeaningfulContent)));
  }, [notes]);

  useEffect(() => {
    saveLastNoteId(hasMeaningfulContent(draft) ? draft.id : null);
  }, [draft]);

  function persistDraft(next: Note) {
    setDraft(next);
    setNotes((current) => {
      const without = current.filter((note) => note.id !== next.id);
      if (!hasMeaningfulContent(next)) return sortNotes(without);
      return sortNotes([next, ...without]);
    });
  }

  function updateDraft(patch: Partial<Note>) {
    persistDraft({ ...draft, ...patch, updatedAt: Date.now() });
  }

  function createNote() {
    const next = blankDraft();
    setDraft(next);
    setEditorMode("source");
    setSidebarOpen(false);
  }

  function openNote(note: Note) {
    setDraft(note);
    setEditorMode("source");
    setSidebarOpen(false);
  }

  function deleteDraft() {
    setNotes((current) => current.filter((note) => note.id !== draft.id));
    const next = notes.filter((note) => note.id !== draft.id)[0] ?? blankDraft();
    setDraft(next);
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
  }

  function insertMarkup(kind: "bold" | "italic" | "code" | "h1" | "h2" | "bullet" | "ordered" | "task" | "quote" | "codeBlock") {
    const suffix = draft.body && !draft.body.endsWith("\n") ? "\n" : "";
    const addition = {
      bold: "**粗体**",
      italic: "*斜体*",
      code: "`代码`",
      h1: "# ",
      h2: "## ",
      bullet: "- ",
      ordered: "1. ",
      task: "- [ ] ",
      quote: "> ",
      codeBlock: "```\n\n```",
    }[kind];
    updateDraft({ body: `${draft.body}${suffix}${addition}` });
  }

  return (
    <div className="phoneApp" data-theme-mode={theme} data-mobile-theme={resolvedTheme}>
      <main className="screen">
        <section className="editorView">
          <header className="editorTopBar">
            <IconButton label="打开列表" onClick={() => setSidebarOpen(true)}><MenuIcon /></IconButton>
            <input
              className="titleInput"
              value={draft.title}
              onChange={(event) => updateDraft({ title: event.target.value })}
              onBlur={() => {
                if (!draft.title.trim()) updateDraft({ title: "Untitled" });
              }}
              placeholder="Untitled"
            />
            <IconButton label="设置" onClick={() => setSettingsOpen(true)}><SettingsIcon /></IconButton>
          </header>

          {editorMode === "source" ? (
            <textarea
              className="bodyEditor"
              value={draft.body}
              onChange={(event) => updateDraft({ body: event.target.value })}
              placeholder="写点什么..."
            />
          ) : (
            <article className="previewPane">
              {draft.body.trim() ? renderMarkdown(draft.body) : <p className="muted">空白笔记</p>}
            </article>
          )}

          <nav
            className="bottomToolbar"
            style={{ transform: `translateY(-${keyboardOffset}px)` }}
            aria-label="编辑工具栏"
          >
            <IconButton label="粗体" onClick={() => insertMarkup("bold")}><BoldIcon /></IconButton>
            <IconButton label="斜体" onClick={() => insertMarkup("italic")}><ItalicIcon /></IconButton>
            <IconButton label="行内代码" onClick={() => insertMarkup("code")}><InlineCodeIcon /></IconButton>
            <div className="toolbarDivider" />
            <IconButton label="一级标题" onClick={() => insertMarkup("h1")}><HeadingIcon /></IconButton>
            <IconButton label="二级标题" onClick={() => insertMarkup("h2")}><HeadingIcon /></IconButton>
            <div className="toolbarDivider" />
            <IconButton label="无序列表" onClick={() => insertMarkup("bullet")}><ListIcon /></IconButton>
            <IconButton label="有序列表" onClick={() => insertMarkup("ordered")}><OrderedListIcon /></IconButton>
            <IconButton label="任务列表" onClick={() => insertMarkup("task")}><TaskListIcon /></IconButton>
            <div className="toolbarDivider" />
            <IconButton label="引用" onClick={() => insertMarkup("quote")}><BlockquoteIcon /></IconButton>
            <IconButton label="代码块" onClick={() => insertMarkup("codeBlock")}><CodeBlockIcon /></IconButton>
            <div className="toolbarSpacer" />
            <div className="toolbarDivider" />
            <IconButton label={editorMode === "preview" ? "源码" : "预览"} onClick={() => setEditorMode((mode) => mode === "preview" ? "source" : "preview")}>
              {editorMode === "preview" ? <SourceIcon /> : <PreviewIcon />}
            </IconButton>
          </nav>
          <button className="newNoteFab" onClick={createNote} type="button" aria-label="新建笔记" title="新建笔记">
            <PlusIcon />
          </button>
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
          </aside>
        </div>
      ) : null}

      {settingsOpen ? (
        <div className="sheetBackdrop" onClick={() => setSettingsOpen(false)}>
          <aside className="settingsSheet" onClick={(event) => event.stopPropagation()}>
            <div className="sheetHandle" />
            <header>
              <h2>设置</h2>
              <button onClick={() => setSettingsOpen(false)} type="button">完成</button>
            </header>
            <section className="settingsGroup">
              <h3>外观</h3>
              <div className="segmented">
                <button className={theme === "system" ? "selected" : ""} onClick={() => setTheme("system")} type="button"><ThemeSystemIcon />系统</button>
                <button className={theme === "dark" ? "selected" : ""} onClick={() => setTheme("dark")} type="button"><ThemeDarkIcon />暗色</button>
                <button className={theme === "light" ? "selected" : ""} onClick={() => setTheme("light")} type="button"><ThemeLightIcon />浅色</button>
              </div>
            </section>
            <section className="settingsGroup">
              <h3>同步</h3>
              <p>账号登录、手动同步、冲突处理和匿名笔记导入会在接入 Android 原生层后复用桌面端同步核心。</p>
              <button className="primaryAction" type="button"><SyncIcon />连接同步服务器</button>
            </section>
          </aside>
        </div>
      ) : null}
    </div>
  );
}
