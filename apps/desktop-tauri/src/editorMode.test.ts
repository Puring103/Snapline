import { describe, expect, it } from "vitest";
import { DEFAULT_EDITOR_MODE, toggleEditorMode } from "./editorMode";

describe("editor mode", () => {
  it("defaults to preview mode", () => {
    expect(DEFAULT_EDITOR_MODE).toBe("preview");
  });

  it("toggles between preview and source modes", () => {
    expect(toggleEditorMode("preview")).toBe("source");
    expect(toggleEditorMode("source")).toBe("preview");
  });
});
