import type { Note, NoteSummary } from "./types";
import { previewFromMarkdown, previewMarkdownFromMarkdown } from "./markdown";

export interface ActiveSession {
  kind: "draft" | "existing";
  id: string | null;
  title: string;
  bodyMd: string;
  persistedTitle: string;
  persistedBodyMd: string;
}

export function createDraftSession(): ActiveSession {
  return {
    kind: "draft",
    id: null,
    title: "Untitled",
    bodyMd: "",
    persistedTitle: "Untitled",
    persistedBodyMd: "",
  };
}

export function isSessionDirty(session: ActiveSession): boolean {
  return normalizeForComparison(session.title) !== normalizeForComparison(session.persistedTitle)
    || normalizeForComparison(session.bodyMd) !== normalizeForComparison(session.persistedBodyMd);
}

export function hasMeaningfulDraftContent(session: ActiveSession): boolean {
  return session.title.trim() !== "" && normalizeForComparison(session.title) !== "Untitled"
    || session.bodyMd.trim() !== "";
}

export function sortNotes(notes: NoteSummary[]): NoteSummary[] {
  return [...notes].sort((a, b) => {
    const aPinned = a.pinned ?? false;
    const bPinned = b.pinned ?? false;
    if (aPinned !== bPinned) {
      return aPinned ? -1 : 1;
    }

    const updated = b.updated_at.localeCompare(a.updated_at);
    if (updated !== 0) {
      return updated;
    }

    return a.title.localeCompare(b.title);
  });
}

export function upsertNote(notes: NoteSummary[], note: Note): NoteSummary[] {
  return sortNotes([
    {
      id: note.id,
      title: note.title,
      preview: previewFromMarkdown((note as Partial<Note>).content_md ?? ""),
      preview_md: previewMarkdownFromMarkdown((note as Partial<Note>).content_md ?? ""),
      updated_at: note.updated_at,
      pinned: note.pinned,
    },
    ...notes.filter((existing) => existing.id !== note.id),
  ]);
}

export function deleteConfirmationFor(currentNoteId: string | null, targetNoteId: string): string | null {
  return currentNoteId === targetNoteId ? null : targetNoteId;
}

export function matchesShortcut(event: KeyboardEvent, shortcut: string): boolean {
  const parts = shortcut.split("+").map((part) => part.trim()).filter(Boolean);
  if (parts.length === 0) return false;

  const state = { ctrl: false, alt: false, shift: false, meta: false, key: "" };
  for (const part of parts) {
    const normalized = part.toLowerCase();
    if (normalized === "ctrl" || normalized === "control") state.ctrl = true;
    else if (normalized === "alt" || normalized === "option") state.alt = true;
    else if (normalized === "shift") state.shift = true;
    else if (normalized === "meta" || normalized === "cmd" || normalized === "command") state.meta = true;
    else state.key = normalizeShortcutKey(part);
  }

  if (!state.key) return false;

  return (
    event.ctrlKey === state.ctrl &&
    event.altKey === state.alt &&
    event.shiftKey === state.shift &&
    event.metaKey === state.meta &&
    normalizeShortcutKey(event.key) === state.key
  );
}

function normalizeForComparison(value: string): string {
  return value.replace(/\r\n/g, "\n").trimEnd();
}

function normalizeShortcutKey(value: string): string {
  if (value === " " || value.toLowerCase() === "space") {
    return "SPACE";
  }

  const normalized = value.trim();
  if (!normalized) return "";

  return normalized.length === 1 ? normalized.toUpperCase() : normalized.toUpperCase();
}
