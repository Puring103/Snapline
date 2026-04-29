export type NoteId = string;

export interface Note {
  id: NoteId;
  title: string;
  content_md: string;
  pinned?: boolean;
  created_at: string;
  updated_at: string;
  deleted_at: string | null;
  server_version?: number;
  last_modified_by_device?: string | null;
  is_conflict_copy?: boolean;
  source_note_id?: string | null;
}

export interface NoteSummary {
  id: NoteId;
  title: string;
  preview: string;
  preview_md?: string;
  pinned?: boolean;
  updated_at: string;
  is_conflict_copy?: boolean;
  source_note_id?: string | null;
}

export interface BootstrapState {
  notes: NoteSummary[];
  current: Note;
  data_dir: string;
}

export interface AssetRef {
  markdown_path: string;
  filesystem_path: string;
}

export interface ShortcutState {
  open_shortcut: string | null;
}

export interface SavedAsset {
  markdown_path: string;
  asset_url: string;
}

export interface DraftParts {
  title: string;
  body_md: string;
}

export interface SyncAccountState {
  account_id: string | null;
  device_id: string;
  server_base_url: string | null;
  is_logged_in: boolean;
}
