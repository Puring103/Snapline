import { LogicalPosition } from "@tauri-apps/api/dpi";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { api } from "./api";

export const CLOSE_OTHER_NOTES_EVENT = "snapline-close-other-note-windows";
export const FOCUS_EDITOR_EVENT = "snapline-focus-editor";

export type AppWindowMode = "list" | "note";

const NOTE_WINDOW_OPTIONS = {
  width: 340,
  height: 440,
  minWidth: 300,
  minHeight: 260,
  resizable: true,
  decorations: false,
} as const;

const LIST_WINDOW_OPTIONS = {
  width: 360,
  height: 520,
  minWidth: 320,
  minHeight: 300,
  resizable: true,
  decorations: false,
} as const;

const NOTE_WINDOW_LABEL_PREFIX = "snapline.noteWindow.";
const DRAFT_WINDOW_LABEL = "main";
const LIST_WINDOW_LABEL = "list";

export interface AppRoute {
  mode: AppWindowMode;
  noteId: string | null;
  newDraft: boolean;
}

export function readAppRoute(): AppRoute {
  const search = new URLSearchParams(window.location.search);
  const mode = search.get("mode") === "list" ? "list" : "note";
  return {
    mode,
    noteId: search.get("noteId"),
    newDraft: search.get("newDraft") === "1",
  };
}

export function shouldDeferInitialNoteLoad({
  launchedInBackground,
  windowLabel,
  noteId,
}: {
  launchedInBackground: boolean;
  windowLabel: string;
  noteId: string | null;
}): boolean {
  return launchedInBackground && windowLabel === "main" && noteId === null;
}

export function openListWindow() {
  return WebviewWindow.getByLabel(LIST_WINDOW_LABEL).then(async (existing) => {
    if (existing) {
      await revealExistingWindow(existing);
      return existing;
    }

    return openAppWindow("list");
  });
}

export interface PointerWindowPosition {
  x: number;
  y: number;
}

export async function openNoteWindow(noteId?: string | null, position?: PointerWindowPosition) {
  return api.openNoteWindow(noteId ?? null, position);
}

async function closeOtherNoteWindows(keepLabel: string) {
  try {
    const all = await WebviewWindow.getAll();
    await Promise.all(
      all
        .filter((w) => w.label !== keepLabel && w.label !== "main" && w.label !== "list" && w.label.startsWith("note-"))
        .map((w) => w.close().catch(() => undefined)),
    );
  } catch {
    // 忽略枚举失败，新窗口仍然能创建
  }
}

export function rememberNoteWindow(noteId: string, label: string) {
  localStorage.setItem(`${NOTE_WINDOW_LABEL_PREFIX}${noteId}`, label);
}

export function forgetNoteWindow(noteId: string) {
  localStorage.removeItem(`${NOTE_WINDOW_LABEL_PREFIX}${noteId}`);
}

export function windowOptionsForMode(mode: AppWindowMode) {
  return mode === "list" ? LIST_WINDOW_OPTIONS : NOTE_WINDOW_OPTIONS;
}

export function shouldStartWindowDrag(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) {
    return false;
  }

  return target.closest("button, input, textarea, select, .chromeMenu") === null;
}

export interface RevealableWindow {
  show: () => Promise<void>;
  unminimize: () => Promise<void>;
  setFocus: () => Promise<void>;
  setPosition?: (position: LogicalPosition) => Promise<void>;
}

export async function revealExistingWindow(window: RevealableWindow, position?: PointerWindowPosition) {
  if (position && window.setPosition) {
    await window.setPosition(new LogicalPosition(position.x, position.y));
  }
  await window.show();
  await window.unminimize();
  await window.setFocus();
}

async function revealExistingNoteWindow(noteId: string) {
  const stableLabel = buildWindowLabel("note", noteId);
  const rememberedLabel = readRememberedNoteWindowLabel(noteId);
  const candidateLabels = rememberedLabel && rememberedLabel !== stableLabel
    ? [rememberedLabel, stableLabel]
    : [stableLabel];

  for (const label of candidateLabels) {
    const existing = await WebviewWindow.getByLabel(label);
    if (existing) {
      await revealExistingWindow(existing);
      rememberNoteWindow(noteId, label);
      return existing;
    }
  }

  forgetNoteWindow(noteId);
  return null;
}

function openAppWindow(mode: AppWindowMode, noteId: string | null = null, position?: PointerWindowPosition) {
  const label = buildWindowLabel(mode, noteId);
  const params = new URLSearchParams({ mode });
  if (noteId) params.set("noteId", noteId);
  if (mode === "note" && noteId === null) params.set("newDraft", "1");
  const options = windowOptionsForMode(mode);

  return new WebviewWindow(label, {
    url: `/?${params.toString()}`,
    title: mode === "list" ? "Snapline" : "Snapline Note",
    ...(position ? { position: new LogicalPosition(position.x, position.y) } : { center: true }),
    ...options,
  });
}

function buildWindowLabel(mode: AppWindowMode, noteId: string | null) {
  if (mode === "list") {
    return LIST_WINDOW_LABEL;
  }

  if (noteId) {
    return `note-${noteId}`;
  }

  const suffix = createWindowLabelSuffix();
  return `${mode}-${suffix}`;
}

function createWindowLabelSuffix() {
  if (typeof crypto.randomUUID === "function") {
    return crypto.randomUUID().replace(/-/g, "");
  }

  return `${Date.now().toString(36)}${Math.random().toString(36).slice(2)}`;
}

function readRememberedNoteWindowLabel(noteId: string): string | null {
  return localStorage.getItem(`${NOTE_WINDOW_LABEL_PREFIX}${noteId}`);
}
