export interface Note {
  id: string;
  title: string;
  body: string;
  pinned: boolean;
  updatedAt: number;
}

export type EditorMode = "write" | "preview";
export type ThemeMode = "dark" | "light";
