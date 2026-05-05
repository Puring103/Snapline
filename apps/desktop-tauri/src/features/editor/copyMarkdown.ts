import type { Editor } from "@tiptap/core";
import { normalizeMarkdown, rewriteMarkdownImageSources } from "./markdown";

export function selectedMarkdown(editor: Editor, restoreImageSource: (source: string) => string = (source) => source): string {
  const { selection } = editor.state;

  if (selection.empty) {
    return normalizeMarkdown(rewriteMarkdownImageSources(editor.getMarkdown(), restoreImageSource));
  }

  const selectedContent = selection.content().content.toJSON();
  const doc = {
    type: "doc",
    content: Array.isArray(selectedContent) ? selectedContent : [selectedContent],
  };

  return normalizeMarkdown(rewriteMarkdownImageSources(editor.markdown?.serialize(doc) ?? editor.getText(), restoreImageSource));
}

export function copySelectedMarkdown(
  editor: Editor,
  clipboardData: DataTransfer | null,
  restoreImageSource?: (source: string) => string,
): boolean {
  if (!clipboardData) {
    return false;
  }

  const markdown = selectedMarkdown(editor, restoreImageSource);
  clipboardData.setData("text/plain", markdown);
  clipboardData.setData("text/markdown", markdown);
  return true;
}

export function cutSelectedMarkdown(
  editor: Editor,
  clipboardData: DataTransfer | null,
  restoreImageSource?: (source: string) => string,
): boolean {
  if (!copySelectedMarkdown(editor, clipboardData, restoreImageSource)) {
    return false;
  }

  editor.commands.deleteSelection();
  return true;
}
