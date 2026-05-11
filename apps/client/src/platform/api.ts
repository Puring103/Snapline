import { invoke } from "@tauri-apps/api/core";
import type {
  AssetRef,
  BootstrapState,
  DraftParts,
  HydratedMarkdown,
  LoginSyncResult,
  MarkdownImageMapping,
  Note,
  NoteSummary,
  SaveDraftResult,
  SyncAccountState,
  SyncReport,
} from "../types";

export const api = {
  launchedInBackground: () => invoke<boolean>("launched_in_background"),
  bootstrap: () => invoke<BootstrapState>("bootstrap"),
  deriveTitleFromMarkdown: (markdown: string) => invoke<string>("derive_title_from_markdown", { markdown }),
  composeDraftMarkdown: (title: string, bodyMd: string) =>
    invoke<string>("compose_draft_markdown", { title, bodyMd }),
  splitDraftMarkdown: (markdown: string) => invoke<DraftParts>("split_draft_markdown", { markdown }),
  splitStoredNoteMarkdown: (storedTitle: string, markdown: string) =>
    invoke<DraftParts>("split_stored_note_markdown", { storedTitle, markdown }),
  prepareDraftForSave: (title: string, bodyMd: string) =>
    invoke<DraftParts>("prepare_draft_for_save", { title, bodyMd }),
  normalizeMarkdown: (markdown: string) => invoke<string>("normalize_markdown", { markdown }),
  hydrateMarkdownAssets: (markdown: string) =>
    invoke<HydratedMarkdown>("hydrate_markdown_assets", { markdown }),
  restoreMarkdownAssetSources: (markdown: string, mappings: MarkdownImageMapping[]) =>
    invoke<string>("restore_markdown_asset_sources", { markdown, mappings }),
  createNote: () => invoke<Note>("create_note"),
  getNote: (id: string) => invoke<Note>("get_note", { id }),
  getNoteSummary: (id: string) => invoke<NoteSummary>("get_note_summary", { id }),
  searchNotes: (query: string) => invoke<NoteSummary[]>("search_notes", { query }),
  saveNote: (id: string, title: string, contentMd: string, pinned: boolean) =>
    invoke<Note>("save_note", { id, title, contentMd, pinned }),
  saveDraftSession: (request: {
    id: string | null;
    title: string;
    body_md: string;
    pinned: boolean;
  }) => invoke<SaveDraftResult>("save_draft_session", { request }),
  setNotePinned: (id: string, pinned: boolean) => invoke<Note>("set_note_pinned", { id, pinned }),
  deleteNote: (id: string) => invoke<NoteSummary[]>("delete_note", { id }),
  savePngAsset: (noteId: string, bytes: number[]) =>
    invoke<AssetRef>("save_png_asset", { noteId, bytes }),
  readAssetBytes: (markdownPath: string) =>
    invoke<number[]>("read_asset_bytes", { markdownPath }),
  readLocalImageFile: (path: string) =>
    invoke<number[]>("read_local_image_file", { path }),
  readClipboardImagePng: () =>
    invoke<number[] | null>("read_clipboard_image_png"),
  assetUrlFromMarkdownPath: (markdownPath: string) =>
    invoke<string>("asset_url_from_markdown_path", { markdownPath }),
  markdownPathFromAssetUrl: (assetUrl: string) =>
    invoke<string>("markdown_path_from_asset_url", { assetUrl }),
  getOpenShortcut: () => invoke<string>("get_open_shortcut"),
  setOpenShortcut: (shortcut: string) => invoke<string>("set_open_shortcut", { shortcut }),
  getSyncAccountState: () => invoke<SyncAccountState>("get_sync_account_state"),
  logoutSync: () => invoke<LoginSyncResult>("logout_sync"),
  registerSync: (serverBaseUrl: string, email: string, password: string) =>
    invoke<LoginSyncResult>("register_sync", { serverBaseUrl, email, password }),
  loginSync: (serverBaseUrl: string, email: string, password: string) =>
    invoke<LoginSyncResult>("login_sync", { serverBaseUrl, email, password }),
  anonymousNoteCount: () => invoke<number>("anonymous_note_count"),
  importAnonymousNotes: () => invoke<NoteSummary[]>("import_anonymous_notes"),
  syncNow: () => invoke<SyncReport>("sync_now"),
  exportNoteAsMarkdown: (id: string) => invoke<string>("export_note_as_markdown", { id }),
  openExternalUrl: (url: string) => invoke<string>("open_external_url", { url }),
  openNoteWindow: (noteId: string | null, position?: { x: number; y: number }) =>
    invoke<string>("open_note_window", { noteId, position }),
};
