export type EditorMode = "preview" | "source";

export const DEFAULT_EDITOR_MODE: EditorMode = "preview";

export function toggleEditorMode(mode: EditorMode): EditorMode {
  return mode === "preview" ? "source" : "preview";
}
