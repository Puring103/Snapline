import { Editor } from "@tiptap/core";
import { TextSelection } from "@tiptap/pm/state";
import { describe, expect, it, vi } from "vitest";
import { copySelectedMarkdown, selectedMarkdown } from "./copyMarkdown";
import { createMarkdownExtensions, setMarkdownContent } from "./editorExtensions";

describe("copy markdown", () => {
  it("copies selected editor content as markdown", () => {
    const editor = new Editor({
      extensions: createMarkdownExtensions(),
      content: "",
    });
    setMarkdownContent(editor, "**Bold** text\n\n> Quote");
    const start = editor.state.doc.textBetween(0, editor.state.doc.content.size).indexOf("Bold") + 1;
    editor.view.dispatch(
      editor.state.tr.setSelection(TextSelection.create(editor.state.doc, start, start + "Bold".length)),
    );

    expect(selectedMarkdown(editor)).toBe("**Bold**");
  });

  it("writes markdown to plain and markdown clipboard formats", () => {
    const editor = new Editor({
      extensions: createMarkdownExtensions(),
      content: "",
    });
    setMarkdownContent(editor, "> Quote");
    const clipboardData = {
      setData: vi.fn(),
    } as unknown as DataTransfer;

    expect(copySelectedMarkdown(editor, clipboardData)).toBe(true);
    expect(clipboardData.setData).toHaveBeenCalledWith("text/plain", "> Quote");
    expect(clipboardData.setData).toHaveBeenCalledWith("text/markdown", "> Quote");
  });
});
