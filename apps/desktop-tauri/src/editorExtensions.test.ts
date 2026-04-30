import { Editor } from "@tiptap/core";
import { afterEach, describe, expect, it } from "vitest";
import { createMarkdownExtensions, setMarkdownContent } from "./editorExtensions";

const editors: Editor[] = [];

afterEach(() => {
  while (editors.length > 0) {
    editors.pop()?.destroy();
  }
});

describe("editor extensions", () => {
  it("sets markdown content as rendered document nodes", () => {
    const editor = new Editor({
      extensions: createMarkdownExtensions(),
      content: "",
    });
    editors.push(editor);

    setMarkdownContent(editor, "- **Item**");

    expect(editor.getHTML()).toContain("<ul>");
    expect(editor.getHTML()).toContain("<strong>Item</strong>");
  });

  it("supports pasted task list markdown", () => {
    const editor = new Editor({
      extensions: createMarkdownExtensions(),
      content: "",
    });
    editors.push(editor);

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
    editors.push(editor);

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
    editors.push(editor);

    setMarkdownContent(editor, "Before\n\n![Alt](https://example.com/image.png)");

    expect(editor.getHTML()).toContain("<img");
    expect(editor.getHTML()).toContain('src="https://example.com/image.png"');
    expect(editor.getMarkdown()).toContain("![Alt](https://example.com/image.png)");
  });

  it("supports pasted link markdown", () => {
    const editor = new Editor({
      extensions: createMarkdownExtensions(),
      content: "",
    });
    editors.push(editor);

    setMarkdownContent(editor, "[OpenAI](https://openai.com)");

    expect(editor.getHTML()).toContain('<a target="_blank"');
    expect(editor.getHTML()).toContain('href="https://openai.com"');
    expect(editor.getMarkdown()).toContain("[OpenAI](https://openai.com)");
  });

  it("supports pasted footnote markdown", () => {
    const editor = new Editor({
      extensions: createMarkdownExtensions(),
      content: "",
    });
    editors.push(editor);

    setMarkdownContent(editor, "A note[^1]\n\n[^1]: Footnote text");

    expect(editor.getHTML()).toContain("footnoteReference");
    expect(editor.getHTML()).toContain("Footnote text");
    expect(editor.getHTML()).not.toContain("[^1]");
    expect(editor.getMarkdown()).toContain("A note[^1]");
    expect(editor.getMarkdown()).toContain("[^1]: Footnote text");
  });

  it("supports pasted blockquote markdown", () => {
    const editor = new Editor({
      extensions: createMarkdownExtensions(),
      content: "",
    });
    editors.push(editor);

    setMarkdownContent(editor, "> Quoted line\n>\n> - Item");

    expect(editor.getHTML()).toContain("<blockquote>");
    expect(editor.getMarkdown()).toContain("> Quoted line");
  });
});
