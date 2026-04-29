import { describe, expect, it } from "vitest";
import { shouldApplyEditorMarkdownUpdate } from "./editorSync";

describe("editor markdown synchronization", () => {
  it("does not reapply a stale transient image after the editor has a durable image source", () => {
    expect(
      shouldApplyEditorMarkdownUpdate(
        "![](asset://localhost/assets/notes/note/image.png)",
        "![](blob:temporary-image)",
      ),
    ).toBe(false);
  });

  it("applies normal external markdown changes", () => {
    expect(
      shouldApplyEditorMarkdownUpdate("Old body", "New body"),
    ).toBe(true);
  });
});
