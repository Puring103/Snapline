import { Editor } from "@tiptap/core";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { createMarkdownExtensions, setMarkdownContent } from "./features/editor/editorExtensions";
import { createSearchHighlightExtension, HighlightedText, searchTermsFromQuery } from "./features/search/highlight";

describe("search highlight", () => {
  it("deduplicates query terms case-insensitively", () => {
    expect(searchTermsFromQuery("Roadmap roadmap  next")).toEqual(["roadmap", "next"]);
  });

  it("highlights matching title text without changing its casing", () => {
    expect(renderToStaticMarkup(<HighlightedText query="road" text="Product Roadmap" />)).toContain(
      '<mark class="searchHighlight">Road</mark>',
    );
  });

  it("adds proseMirror decorations for markdown preview matches", () => {
    const editor = new Editor({
      extensions: [...createMarkdownExtensions(), createSearchHighlightExtension("alpha beta")],
      content: "",
    });

    setMarkdownContent(editor, "Alpha **beta** gamma");

    expect(editor.view.dom.querySelectorAll(".searchHighlight")).toHaveLength(2);
    expect(editor.getMarkdown()).toContain("Alpha **beta** gamma");

    editor.destroy();
  });
});
