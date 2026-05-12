export interface Note {
  id: string;
  title: string;
  body: string;
  pinned: boolean;
  updatedAt: number;
  isConflictCopy?: boolean;
  sourceNoteId?: string | null;
  ownerAccountId?: string | null;
}

export type EditorMode = "source" | "preview";
export type ThemeMode = "system" | "dark" | "light";
