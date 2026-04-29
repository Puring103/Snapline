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

  it("supports pasted task list markdown", () => {
    const editor = new Editor({
      extensions: createMarkdownExtensions(),
      content: "",
    });

    setMarkdownContent(editor, "- [x] Done\n- [ ] Todo");

    expect(editor.getHTML()).toContain('data-type="taskList"');
    expect(editor.getHTML()).toContain('data-type="taskItem"');
    expect(editor.getHTML()).toContain("checked");
  });

  it("supports pasted table markdown", () => {
    const editor = new Editor({
      extensions: createMarkdownExtensions(),
      content: "",
    });

    setMarkdownContent(editor, "| Name | Status |\n| --- | --- |\n| Paste | Works |");

    expect(editor.getHTML()).toContain("<table");
    expect(editor.getHTML()).toContain("<th");
    expect(editor.getHTML()).toContain("<td");
    expect(editor.getMarkdown()).toContain("| Name");
  });

  it("supports pasted image markdown", () => {
    const editor = new Editor({
      extensions: createMarkdownExtensions(),
      content: "",
    });

    setMarkdownContent(editor, "Before\n\n![Alt](https://example.com/image.png)");

    expect(editor.getHTML()).toContain("<img");
    expect(editor.getHTML()).toContain('src="https://example.com/image.png"');
    expect(editor.getMarkdown()).toContain("![Alt](https://example.com/image.png)");
  });

  it("supports pasted blockquote markdown", () => {
    const editor = new Editor({
      extensions: createMarkdownExtensions(),
      content: "",
    });

    setMarkdownContent(editor, "> Quoted line\n>\n> - Item");

    expect(editor.getHTML()).toContain("<blockquote>");
    expect(editor.getMarkdown()).toContain("> Quoted line");
  });
});
