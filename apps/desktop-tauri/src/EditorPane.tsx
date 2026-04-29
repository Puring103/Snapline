import { EditorContent, useEditor } from "@tiptap/react";
import { useEffect, useRef } from "react";
import { createMarkdownExtensions, setMarkdownContent } from "./editorExtensions";
import {
  assetUrlFromMarkdownPath,
  hasTransientImageSource,
  markdownPathFromAssetUrl,
  markdownTextFromClipboard,
  normalizeMarkdown,
  rewriteMarkdownImageSources,
} from "./markdown";
import { startupLog } from "./startupLog";
import type { SavedAsset } from "./types";

interface EditorPaneProps {
  bodyMarkdown: string;
  onBodyChange: (bodyMarkdown: string) => void;
  onRequestImageSave: (bytes: number[]) => Promise<SavedAsset | null>;
  readOnly?: boolean;
}

export function EditorPane({
  bodyMarkdown,
  onBodyChange,
  onRequestImageSave,
  readOnly = false,
}: EditorPaneProps) {
  const suppressNextUpdate = useRef(false);

  const editor = useEditor({
    extensions: createMarkdownExtensions("Write before the thought fades..."),
    content: rewriteMarkdownImageSources(bodyMarkdown, assetUrlFromMarkdownPath),
    contentType: "markdown",
    editable: !readOnly,
    editorProps: {
      handlePaste: (view, event) => {
        if (readOnly) return false;

        const clipboardItems = Array.from(event.clipboardData?.items ?? []);
        const imageItem = clipboardItems.find(
          (item) => item.kind === "file" && item.type.startsWith("image/"),
        );

        if (!imageItem) return false;

        const file = imageItem.getAsFile();
        if (!file) return false;

        event.preventDefault();
        const placeholderSrc = URL.createObjectURL(file);

        view.dispatch(
          view.state.tr.replaceSelectionWith(
            view.state.schema.nodes.image.create({ src: placeholderSrc }),
          ),
        );

        void file.arrayBuffer().then(async (buffer) => {
          try {
            const asset = await onRequestImageSave(Array.from(new Uint8Array(buffer)));
            if (!asset || !editor) {
              return;
            }

            const assetUrl = asset.asset_url;

            editor.commands.command(({ tr, state, dispatch }) => {
              const imageType = state.schema.nodes.image;
              let updated = false;

              state.doc.descendants((node, pos) => {
                if (node.type === imageType && node.attrs.src === placeholderSrc) {
                  tr.setNodeMarkup(pos, undefined, {
                    ...node.attrs,
                    src: assetUrl,
                  });
                  updated = true;
                  return false;
                }
                return true;
              });

              if (updated && dispatch) {
                dispatch(tr);
              }

              return updated;
            });
          } finally {
            URL.revokeObjectURL(placeholderSrc);
          }
        });

        return true;
      },
      handleDOMEvents: {
        paste: (_view, event) => {
          if (readOnly) return false;

          const clipboardData = event.clipboardData;
          if (!clipboardData) {
            return false;
          }

          const clipboardItems = Array.from(clipboardData.items ?? []);
          const imageItem = clipboardItems.find(
            (item) => item.kind === "file" && item.type.startsWith("image/"),
          );
          if (imageItem) {
            return false;
          }

          const markdownText = markdownTextFromClipboard(clipboardData);
          if (!markdownText) {
            return false;
          }

          event.preventDefault();
          editor?.commands.insertContent(markdownText, { contentType: "markdown" });
          return true;
        },
      },
    },
    onUpdate: ({ editor }) => {
      if (suppressNextUpdate.current) return;

      const nextMarkdown = rewriteMarkdownImageSources(
        normalizeMarkdown(editor.getMarkdown() ?? ""),
        markdownPathFromAssetUrl,
      );
      onBodyChange(nextMarkdown);
    },
  });

  useEffect(() => {
    if (!editor) return;
    startupLog("editor_ready");
  }, [editor]);

  useEffect(() => {
    if (!editor) return;
    editor.setEditable(!readOnly);
  }, [editor, readOnly]);

  useEffect(() => {
    if (!editor) return;

    const nextDisplayMarkdown = rewriteMarkdownImageSources(
      bodyMarkdown,
      assetUrlFromMarkdownPath,
    );
    const currentMarkdown = normalizeMarkdown(editor.getMarkdown() ?? "");
    const nextMarkdown = normalizeMarkdown(nextDisplayMarkdown);

    if (currentMarkdown === nextMarkdown) {
      return;
    }

    suppressNextUpdate.current = true;
    setMarkdownContent(editor, nextDisplayMarkdown);
    queueMicrotask(() => {
      suppressNextUpdate.current = false;
    });
  }, [bodyMarkdown, editor]);

  if (!editor) {
    return <div className="editorState">Loading editor</div>;
  }

  return (
    <div className="editorShell">
      <EditorContent editor={editor} className="editorSurface" />
      {hasTransientImageSource(bodyMarkdown) ? (
        <div className="editorHint">Uploading image...</div>
      ) : null}
    </div>
  );
}
