export interface Note {
  id: string;
  title: string;
  body: string;
  pinned: boolean;
  updatedAt: number;
}

export type EditorMode = "source" | "preview";
export type ThemeMode = "system" | "dark" | "light";
