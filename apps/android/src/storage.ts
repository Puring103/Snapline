import type { Note, ThemeMode } from "./types";

const NOTES_KEY = "snapline.android.notes";
const THEME_KEY = "snapline.android.theme";
const LAST_NOTE_KEY = "snapline.android.lastNoteId";

const seedNotes: Note[] = [
  {
    id: "welcome",
    title: "Snapline Android",
    body: "移动端版本使用单列信息架构：打开就是笔记流，进入后是全屏编辑器，常用编辑动作固定在底部工具栏。\n\n- 保留置顶、列表、编辑、删除、预览\n- 默认暗色，支持浅色切换\n- 后续接入 Tauri Android 后复用桌面端核心数据命令",
    pinned: true,
    updatedAt: Date.now(),
  },
  {
    id: "sync-plan",
    title: "同步兼容计划",
    body: "账号登录、手动同步、冲突提示和导入匿名笔记会放入设置抽屉，避免占用编辑首屏。",
    pinned: false,
    updatedAt: Date.now() - 1000 * 60 * 34,
  },
];

export function loadNotes(): Note[] {
  const raw = localStorage.getItem(NOTES_KEY);
  if (!raw) return seedNotes;
  try {
    const parsed = JSON.parse(raw) as Note[];
    return Array.isArray(parsed) ? parsed : seedNotes;
  } catch {
    return seedNotes;
  }
}

export function saveNotes(notes: Note[]) {
  localStorage.setItem(NOTES_KEY, JSON.stringify(notes));
}

export function loadLastNoteId(): string | null {
  return localStorage.getItem(LAST_NOTE_KEY);
}

export function saveLastNoteId(id: string | null) {
  if (id) {
    localStorage.setItem(LAST_NOTE_KEY, id);
  } else {
    localStorage.removeItem(LAST_NOTE_KEY);
  }
}

export function loadTheme(): ThemeMode {
  return localStorage.getItem(THEME_KEY) === "light" ? "light" : "dark";
}

export function saveTheme(theme: ThemeMode) {
  localStorage.setItem(THEME_KEY, theme);
}
