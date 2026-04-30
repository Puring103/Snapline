import { Editor } from "@tiptap/core";
import { TextSelection } from "@tiptap/pm/state";
import { afterEach, describe, expect, it, vi } from "vitest";
import { copySelectedMarkdown, cutSelectedMarkdown, selectedMarkdown } from "./copyMarkdown";
import { createMarkdownExtensions, setMarkdownContent } from "./editorExtensions";

const editors: Editor[] = [];

afterEach(() => {
  while (editors.length > 0) {
    editors.pop()?.destroy();
  }
});

describe("copy markdown", () => {
  it("copies selected editor content as markdown", () => {
    const editor = new Editor({
      extensions: createMarkdownExtensions(),
      content: "",
    });
    editors.push(editor);
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
    editors.push(editor);
    setMarkdownContent(editor, "> Quote");
    const clipboardData = {
      setData: vi.fn(),
    } as unknown as DataTransfer;

    expect(copySelectedMarkdown(editor, clipboardData)).toBe(true);
    expect(clipboardData.setData).toHaveBeenCalledWith("text/plain", "> Quote");
    expect(clipboardData.setData).toHaveBeenCalledWith("text/markdown", "> Quote");
  });

  it("restores display image urls to original markdown when copying", () => {
    const editor = new Editor({
      extensions: createMarkdownExtensions(),
      content: "",
    });
    editors.push(editor);
    setMarkdownContent(editor, "![](asset://localhost/C:/Snapline/assets/notes/n/image.png)");
    const clipboardData = {
      setData: vi.fn(),
    } as unknown as DataTransfer;

    expect(
      copySelectedMarkdown(editor, clipboardData, (source) =>
        source === "asset://localhost/C:/Snapline/assets/notes/n/image.png"
          ? "assets/notes/n/image.png"
          : source,
      ),
    ).toBe(true);
    expect(clipboardData.setData).toHaveBeenCalledWith("text/plain", "![](assets/notes/n/image.png)");
  });

  it("cuts selected content as markdown and deletes the editor selection", () => {
    const editor = new Editor({
      extensions: createMarkdownExtensions(),
      content: "",
    });
    editors.push(editor);
    setMarkdownContent(editor, "Before **Bold** after");
    const start = editor.state.doc.textBetween(0, editor.state.doc.content.size).indexOf("Bold") + 1;
    editor.view.dispatch(
      editor.state.tr.setSelection(TextSelection.create(editor.state.doc, start, start + "Bold".length)),
    );
    const clipboardData = {
      setData: vi.fn(),
    } as unknown as DataTransfer;

    expect(cutSelectedMarkdown(editor, clipboardData)).toBe(true);
    expect(clipboardData.setData).toHaveBeenCalledWith("text/plain", "**Bold**");
    expect(editor.getText()).toBe("Before  after");
  });
});
