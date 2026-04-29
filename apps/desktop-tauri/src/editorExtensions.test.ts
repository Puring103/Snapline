import { Editor } from "@tiptap/core";
import { describe, expect, it } from "vitest";
import { createMarkdownExtensions, setMarkdownContent } from "./editorExtensions";

describe("editor extensions", () => {
  it("sets markdown content as rendered document nodes", () => {
    const editor = new Editor({
      extensions: createMarkdownExtensions(),
      content: "",
    });

    setMarkdownContent(editor, "- **Item**");

    expect(editor.getHTML()).toContain("<ul>");
    expect(editor.getHTML()).toContain("<strong>Item</strong>");
  });
});
