import { LogicalPosition } from "@tauri-apps/api/dpi";
import { emit } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { api } from "./api";

export const CLOSE_OTHER_NOTES_EVENT = "snapline-close-other-note-windows";
export const CLOSE_NOTE_WINDOWS_EVENT = "snapline-close-note-windows";
export const FOCUS_EDITOR_EVENT = "snapline-focus-editor";
export const PREPARE_NOTE_WINDOW_EVENT = "snapline-prepare-note-window";

export type AppWindowMode = "list" | "note" | "android";

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
const CLAIMED_SPARE_WINDOW_LABEL_PREFIX = "snapline.claimedSpareWindow.";
const DRAFT_WINDOW_LABEL = "main";
const LIST_WINDOW_LABEL = "list";
const SPARE_NOTE_WINDOW_PREFIX = "note-spare-";

export interface AppRoute {
  mode: AppWindowMode;
  noteId: string | null;
  newDraft: boolean;
  spare: boolean;
}

export function readAppRoute(): AppRoute {
  const search = new URLSearchParams(window.location.search);
  const rawMode = search.get("mode");
  const mode: AppWindowMode = rawMode === "list" || rawMode === "android"
    ? rawMode
    : isAndroidRuntime()
      ? "android"
      : "note";
  return {
    mode,
    noteId: search.get("noteId"),
    newDraft: search.get("newDraft") === "1",
    spare: search.get("spare") === "1",
  };
}

export function isAndroidRuntime(): boolean {
  const userAgent = navigator.userAgent.toLowerCase();
  return userAgent.includes("android") && userAgent.includes("wv");
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
  if (!isAndroidRuntime()) {
    const spareLabel = await consumeSpareNoteWindow(noteId ?? null, position);
    if (spareLabel) {
      if (noteId) {
        await emit(CLOSE_NOTE_WINDOWS_EVENT, { id: noteId, exceptLabel: spareLabel });
        await closeKnownNoteWindowsForNote(noteId, spareLabel);
        rememberNoteWindow(noteId, spareLabel);
      }
      void ensureSpareNoteWindow();
      return spareLabel;
    }
  }

  if (noteId) {
    const label = await api.openNoteWindow(noteId, position);
    await emit(CLOSE_NOTE_WINDOWS_EVENT, { id: noteId, exceptLabel: label });
    await closeKnownNoteWindowsForNote(noteId, label);
    rememberNoteWindow(noteId, label);
    if (!isAndroidRuntime()) {
      void ensureSpareNoteWindow();
    }
    return label;
  }

  const label = await api.openNoteWindow(null, position);
  if (!isAndroidRuntime()) {
    void ensureSpareNoteWindow();
  }
  return label;
}

export async function ensureSpareNoteWindow() {
  if (isAndroidRuntime()) return null;

  const existing = await findAvailableSpareNoteWindow();
  if (existing) return existing.label;

  const label = `${SPARE_NOTE_WINDOW_PREFIX}${createWindowLabelSuffix()}`;
  const spare = openAppWindow("note", null, undefined, {
    label,
    params: { spare: "1" },
  });
  return spare.label;
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

export async function revealCurrentWindowWhenReady() {
  const currentWindow = getCurrentWindow();
  await nextPaint();
  await currentWindow.show();
  await currentWindow.unminimize();
  await currentWindow.setFocus();
}

function nextPaint() {
  return new Promise<void>((resolve) => {
    requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
  });
}

async function closeKnownNoteWindowsForNote(noteId: string, exceptLabel: string) {
  const stableLabel = buildWindowLabel("note", noteId);
  const rememberedLabel = readRememberedNoteWindowLabel(noteId);
  const candidateLabels = rememberedLabel && rememberedLabel !== stableLabel
    ? [rememberedLabel, stableLabel]
    : [stableLabel];

  await Promise.all(candidateLabels.map(async (label) => {
    if (label === exceptLabel) return;
    const existing = await WebviewWindow.getByLabel(label);
    await existing?.close().catch(() => undefined);
  }));
}

async function consumeSpareNoteWindow(noteId: string | null, position?: PointerWindowPosition) {
  const spare = await findAvailableSpareNoteWindow();
  if (!spare) return null;

  claimSpareWindow(spare.label);
  await emit(PREPARE_NOTE_WINDOW_EVENT, { label: spare.label, noteId });
  if (position) {
    await spare.setPosition?.(new LogicalPosition(position.x, position.y));
  }
  await revealExistingWindow(spare);
  return spare.label;
}

async function findAvailableSpareNoteWindow() {
  try {
    const all = await WebviewWindow.getAll();
    return all.find((window) => window.label.startsWith(SPARE_NOTE_WINDOW_PREFIX) && !isSpareWindowClaimed(window.label)) ?? null;
  } catch {
    return null;
  }
}

function openAppWindow(
  mode: AppWindowMode,
  noteId: string | null = null,
  position?: PointerWindowPosition,
  override?: { label?: string; params?: Record<string, string> },
) {
  const label = override?.label ?? buildWindowLabel(mode, noteId);
  const params = new URLSearchParams({ mode });
  if (noteId) params.set("noteId", noteId);
  if (mode === "note" && noteId === null && override?.params?.spare !== "1") params.set("newDraft", "1");
  Object.entries(override?.params ?? {}).forEach(([key, value]) => params.set(key, value));
  const options = windowOptionsForMode(mode);

  return new WebviewWindow(label, {
    url: `/?${params.toString()}`,
    title: mode === "list" ? "Snapline" : "Snapline Note",
    ...(position ? { position: new LogicalPosition(position.x, position.y) } : { center: true }),
    ...options,
    visible: false,
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

function claimSpareWindow(label: string) {
  localStorage.setItem(`${CLAIMED_SPARE_WINDOW_LABEL_PREFIX}${label}`, "1");
}

function isSpareWindowClaimed(label: string) {
  return localStorage.getItem(`${CLAIMED_SPARE_WINDOW_LABEL_PREFIX}${label}`) === "1";
}
