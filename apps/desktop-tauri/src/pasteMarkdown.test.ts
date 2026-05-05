import { describe, expect, it, vi } from "vitest";
import { Editor } from "@tiptap/core";
import { createMarkdownExtensions } from "./features/editor/editorExtensions";
import { insertClipboardMarkdown } from "./features/editor/pasteMarkdown";

describe("paste markdown", () => {
  it("falls back to plain text insertion when markdown insertion fails", () => {
    const insertText = vi.fn();
    const editor = {
      commands: {
        insertContent: vi.fn(() => false),
        command: vi.fn((handler) =>
          handler({
            tr: {
              insertText,
            },
            dispatch: vi.fn(),
          }),
        ),
      },
    };

    expect(insertClipboardMarkdown(editor, "| A | B |\n| - | - |\n| 1 | 2 |")).toBe(true);
    expect(editor.commands.insertContent).toHaveBeenCalledWith("| A | B |\n| - | - |\n| 1 | 2 |", {
      contentType: "markdown",
    });
    expect(insertText).toHaveBeenCalledWith("| A | B |\n| - | - |\n| 1 | 2 |");
  });

  it("falls back to plain text insertion when markdown insertion throws", () => {
    const insertText = vi.fn();
    const editor = {
      commands: {
        insertContent: vi.fn(() => {
          throw new Error("unsupported markdown");
        }),
        command: vi.fn((handler) =>
          handler({
            tr: {
              insertText,
            },
            dispatch: vi.fn(),
          }),
        ),
      },
    };

    expect(insertClipboardMarkdown(editor, "<unknown>custom block</unknown>")).toBe(true);
    expect(insertText).toHaveBeenCalledWith("<unknown>custom block</unknown>");
  });

  it("inserts known unsupported markdown syntax as plain text directly", () => {
    const insertText = vi.fn();
    const editor = {
      commands: {
        insertContent: vi.fn(() => true),
        command: vi.fn((handler) =>
          handler({
            tr: {
              insertText,
            },
            dispatch: vi.fn(),
          }),
        ),
      },
    };

    expect(insertClipboardMarkdown(editor, "$$\na^2 + b^2 = c^2\n$$")).toBe(true);
    expect(editor.commands.insertContent).not.toHaveBeenCalled();
    expect(insertText).toHaveBeenCalledWith("$$\na^2 + b^2 = c^2\n$$");
  });

  it("inserts footnote markdown through the markdown parser", () => {
    const editor = {
      commands: {
        insertContent: vi.fn(() => true),
        command: vi.fn(),
      },
    };

    expect(insertClipboardMarkdown(editor, "A note[^1]\n\n[^1]: Footnote text")).toBe(true);
    expect(editor.commands.insertContent).toHaveBeenCalledWith("A note[^1]\n\n[^1]: Footnote text", {
      contentType: "markdown",
    });
    expect(editor.commands.command).not.toHaveBeenCalled();
  });


  it("inserts supported table markdown as a table", () => {
    const editor = new Editor({
      extensions: createMarkdownExtensions(),
      content: "",
    });

    insertClipboardMarkdown(editor, "| A | B |\n| - | - |\n| 1 | 2 |");

    expect(editor.getHTML()).toContain("<table");
    expect(editor.getHTML()).toContain("<td");
    expect(editor.getMarkdown()).toContain("| A");
    expect(editor.getMarkdown()).toContain("| B");
    expect(editor.getMarkdown()).toContain("| 1");
    expect(editor.getMarkdown()).toContain("| 2");
  });
});
