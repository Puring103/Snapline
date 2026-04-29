import { EditorContent, useEditor } from "@tiptap/react";
import { useEffect } from "react";
import { createMarkdownExtensions, setMarkdownContent } from "./editorExtensions";
import { assetUrlFromMarkdownPath, rewriteMarkdownImageSources } from "./markdown";
import { startupLog } from "./startupLog";

interface MarkdownPreviewProps {
  markdown: string;
}

export function MarkdownPreview({ markdown }: MarkdownPreviewProps) {
  const editor = useEditor({
    extensions: createMarkdownExtensions(),
    content: rewriteMarkdownImageSources(markdown, assetUrlFromMarkdownPath),
    contentType: "markdown",
    editable: false,
  });

  useEffect(() => {
    if (!editor) return;
    startupLog("preview_ready");
  }, [editor]);

  useEffect(() => {
    if (!editor) return;
    setMarkdownContent(editor, rewriteMarkdownImageSources(markdown, assetUrlFromMarkdownPath));
  }, [editor, markdown]);

  if (!editor) {
    return <div className="noteRowPreview">Loading preview...</div>;
  }

  return <EditorContent editor={editor} className="noteRowPreview" />;
}
