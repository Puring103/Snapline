import { invoke } from "@tauri-apps/api/core";
import type { AssetRef, BootstrapState, Note, NoteSummary, SyncAccountState } from "./types";

export const api = {
  launchedInBackground: () => invoke<boolean>("launched_in_background"),
  bootstrap: () => invoke<BootstrapState>("bootstrap"),
  createNote: () => invoke<Note>("create_note"),
  getNote: (id: string) => invoke<Note>("get_note", { id }),
  saveNote: (id: string, title: string, contentMd: string, pinned: boolean) =>
    invoke<Note>("save_note", { id, title, contentMd, pinned }),
  setNotePinned: (id: string, pinned: boolean) => invoke<Note>("set_note_pinned", { id, pinned }),
  deleteNote: (id: string) => invoke<NoteSummary[]>("delete_note", { id }),
  savePngAsset: (noteId: string, bytes: number[]) =>
    invoke<AssetRef>("save_png_asset", { noteId, bytes }),
  readAssetBytes: (markdownPath: string) =>
    invoke<number[]>("read_asset_bytes", { markdownPath }),
  getOpenShortcut: () => invoke<string>("get_open_shortcut"),
  setOpenShortcut: (shortcut: string) => invoke<string>("set_open_shortcut", { shortcut }),
  getSyncAccountState: () => invoke<SyncAccountState>("get_sync_account_state"),
  loginSync: (serverBaseUrl: string, email: string, password: string) =>
    invoke<SyncAccountState>("login_sync", { serverBaseUrl, email, password }),
  syncNow: () => invoke<string>("sync_now"),
  openExternalUrl: (url: string) => invoke<string>("open_external_url", { url }),
};
