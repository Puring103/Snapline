export type NoteId = string;

export interface Note {
  id: NoteId;
  title: string;
  content_md: string;
  created_at: string;
  updated_at: string;
  deleted_at: string | null;
}

export interface NoteSummary {
  id: NoteId;
  title: string;
  updated_at: string;
}

export interface BootstrapState {
  notes: NoteSummary[];
  current: Note;
}

export interface AssetRef {
  markdown_path: string;
}
