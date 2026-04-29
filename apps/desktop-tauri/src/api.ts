import { invoke } from "@tauri-apps/api/core";
import type { AssetRef, BootstrapState, Note, NoteSummary } from "./types";

export const api = {
  bootstrap: () => invoke<BootstrapState>("bootstrap"),
  createNote: () => invoke<Note>("create_note"),
  getNote: (id: string) => invoke<Note>("get_note", { id }),
  saveNote: (id: string, contentMd: string) =>
    invoke<Note>("save_note", { id, content_md: contentMd }),
  deleteNote: (id: string) => invoke<NoteSummary[]>("delete_note", { id }),
  savePngAsset: (noteId: string, bytes: number[]) =>
    invoke<AssetRef>("save_png_asset", { note_id: noteId, bytes }),
};
