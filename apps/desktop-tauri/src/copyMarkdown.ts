import type { Editor } from "@tiptap/core";
import { normalizeMarkdown } from "./markdown";

export function selectedMarkdown(editor: Editor): string {
  const { selection } = editor.state;

  if (selection.empty) {
    return normalizeMarkdown(editor.getMarkdown());
  }

  const selectedContent = selection.content().content.toJSON();
  const doc = {
    type: "doc",
    content: Array.isArray(selectedContent) ? selectedContent : [selectedContent],
  };

  return normalizeMarkdown(editor.markdown?.serialize(doc) ?? editor.getText());
}

export function copySelectedMarkdown(editor: Editor, clipboardData: DataTransfer | null): boolean {
  if (!clipboardData) {
    return false;
  }

  const markdown = selectedMarkdown(editor);
  clipboardData.setData("text/plain", markdown);
  clipboardData.setData("text/markdown", markdown);
  return true;
}
