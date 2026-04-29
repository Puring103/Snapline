import { EditorContent, useEditor } from "@tiptap/react";
import type { Editor } from "@tiptap/core";
import { useEffect, useRef } from "react";
import { copySelectedMarkdown } from "./copyMarkdown";
import { shouldApplyEditorMarkdownUpdate } from "./editorSync";
import { createMarkdownExtensions, setMarkdownContent } from "./editorExtensions";
import {
  assetUrlFromMarkdownPath,
  hasTransientImageSource,
  markdownPathFromAssetUrl,
  markdownTextFromClipboard,
  normalizeMarkdown,
  rewriteMarkdownImageSources,
} from "./markdown";
import {
  bytesFromPastedImageFile,
  bytesFromTransientImageSource,
  pastedImageFileFromClipboard,
} from "./pasteImage";
import { insertClipboardMarkdown } from "./pasteMarkdown";
import { startupLog } from "./startupLog";
import type { SavedAsset } from "./types";

interface EditorPaneProps {
  bodyMarkdown: string;
  focusRequestId?: number;
  onBodyChange: (bodyMarkdown: string) => void;
  onRequestImageSave: (bytes: number[]) => Promise<SavedAsset | null>;
  readOnly?: boolean;
}

export function EditorPane({
  bodyMarkdown,
  focusRequestId = 0,
  onBodyChange,
  onRequestImageSave,
  readOnly = false,
}: EditorPaneProps) {
  const suppressNextUpdate = useRef(false);
  const editorRef = useRef<Editor | null>(null);
  const uploadingImageSources = useRef(new Set<string>());

  const editor = useEditor({
    extensions: createMarkdownExtensions("Write before the thought fades..."),
    content: rewriteMarkdownImageSources(bodyMarkdown, assetUrlFromMarkdownPath),
    contentType: "markdown",
    editable: !readOnly,
    editorProps: {
      handleDOMEvents: {
        contextmenu: (_view, event) => {
          event.preventDefault();
          return true;
        },
        copy: (_view, event) => {
          const activeEditor = editorRef.current;
          if (!activeEditor) {
            return false;
          }

          if (!copySelectedMarkdown(activeEditor, event.clipboardData)) {
            return false;
          }

          event.preventDefault();
          return true;
        },
        paste: (view, event) => {
          if (readOnly) return false;

          const clipboardData = event.clipboardData;
          if (!clipboardData) {
            return false;
          }

          const file = pastedImageFileFromClipboard(clipboardData);
          if (file) {
            event.preventDefault();
            const placeholderSrc = URL.createObjectURL(file);
            const bytesPromise = bytesFromPastedImageFile(file);
            uploadingImageSources.current.add(placeholderSrc);

            view.dispatch(
              view.state.tr.replaceSelectionWith(
                view.state.schema.nodes.image.create({ src: placeholderSrc }),
              ),
            );

            void uploadTransientImageSource(placeholderSrc, bytesPromise);

            return true;
          }

          const markdownText = markdownTextFromClipboard(clipboardData);
          if (!markdownText) {
            return false;
          }

          const activeEditor = editorRef.current;
          if (!activeEditor) {
            return false;
          }

          event.preventDefault();
          return insertClipboardMarkdown(activeEditor, markdownText);
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
      void uploadTransientImages(editor);
    },
  });

  editorRef.current = editor;

  useEffect(() => {
    if (!editor) return;
    startupLog("editor_ready");
    void uploadTransientImages(editor);
  }, [editor]);

  useEffect(() => {
    if (!editor) return;
    editor.setEditable(!readOnly);
  }, [editor, readOnly]);

  useEffect(() => {
    if (!editor || readOnly || focusRequestId === 0) return;
    editor.commands.focus("end");
  }, [editor, focusRequestId, readOnly]);

  useEffect(() => {
    if (!editor) return;

    const nextDisplayMarkdown = rewriteMarkdownImageSources(
      bodyMarkdown,
      assetUrlFromMarkdownPath,
    );
    const currentMarkdown = normalizeMarkdown(editor.getMarkdown() ?? "");
    const nextMarkdown = normalizeMarkdown(nextDisplayMarkdown);

    if (!shouldApplyEditorMarkdownUpdate(currentMarkdown, nextMarkdown)) {
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
      <EditorContent editor={editor} className="editorSurface markdownSurface" />
      {hasTransientImageSource(bodyMarkdown) ? (
        <div className="editorHint">Uploading image...</div>
      ) : null}
    </div>
  );

  async function uploadTransientImages(activeEditor: Editor) {
    const imageType = activeEditor.state.schema.nodes.image;
    const sources: string[] = [];

    activeEditor.state.doc.descendants((node) => {
      if (node.type === imageType && isTransientImageSource(node.attrs.src)) {
        sources.push(node.attrs.src);
      }
      return true;
    });

    for (const source of sources) {
      if (!uploadingImageSources.current.has(source)) {
        void uploadTransientImageSource(source);
      }
    }
  }

  async function uploadTransientImageSource(source: string, bytesPromise?: Promise<number[]>) {
    uploadingImageSources.current.add(source);

    try {
      const bytes = await (bytesPromise ?? bytesFromTransientImageSource(source));
      const asset = await onRequestImageSave(bytes);
      const activeEditor = editorRef.current;

      if (!activeEditor) {
        return;
      }

      if (!asset) {
        removeImageSource(activeEditor, source);
        return;
      }

      updateImageSource(activeEditor, source, asset.asset_url);
    } catch {
      const activeEditor = editorRef.current;
      if (activeEditor) {
        removeImageSource(activeEditor, source);
      }
    } finally {
      uploadingImageSources.current.delete(source);
      if (source.startsWith("blob:")) {
        URL.revokeObjectURL(source);
      }
    }
  }
}

function isTransientImageSource(source: unknown): source is string {
  return typeof source === "string" && (source.startsWith("blob:") || source.startsWith("data:"));
}

function removeImageSource(editor: Editor, source: string) {
  updateImageSource(editor, source, null);
}

function updateImageSource(editor: Editor, source: string, nextSource: string | null) {
  editor.commands.command(({ tr, state }) => {
    const imageType = state.schema.nodes.image;
    let updated = false;

    state.doc.descendants((node, pos) => {
      if (node.type !== imageType || node.attrs.src !== source) {
        return true;
      }

      if (nextSource) {
        tr.setNodeMarkup(pos, undefined, {
          ...node.attrs,
          src: nextSource,
        });
      } else {
        tr.delete(pos, pos + node.nodeSize);
      }

      updated = true;
      return false;
    });

    return updated;
  });
}
