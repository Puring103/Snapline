import type { CommandProps } from "@tiptap/core";

type PasteEditor = {
  commands: {
    insertContent: (value: string, options: { contentType: "markdown" }) => boolean;
    command: (handler: (props: CommandProps) => boolean) => boolean;
  };
};

export function insertClipboardMarkdown(editor: PasteEditor, markdownText: string): boolean {
  if (hasUnsupportedMarkdownSyntax(markdownText)) {
    return insertPlainText(editor, markdownText);
  }

  try {
    const inserted = editor.commands.insertContent(markdownText, {
      contentType: "markdown",
    });

    if (inserted) {
      return true;
    }
  } catch {
    // Fall through to plain text insertion for markdown the editor cannot parse yet.
  }

  return insertPlainText(editor, markdownText);
}

function insertPlainText(editor: PasteEditor, text: string): boolean {
  return editor.commands.command(({ tr, dispatch }) => {
    if (!dispatch) {
      return true;
    }

    tr.insertText(text);
    return true;
  });
}

function hasUnsupportedMarkdownSyntax(markdownText: string): boolean {
  return [
    /^\s*:::/m,
  ].some((pattern) => pattern.test(markdownText));
}
