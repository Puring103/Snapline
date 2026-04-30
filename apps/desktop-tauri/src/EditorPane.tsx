import { EditorContent, useEditor } from "@tiptap/react";
import type { Editor } from "@tiptap/core";
import { useEffect, useRef } from "react";
import { api } from "./api";
import { blobUrlFromBytes } from "./assetDisplay";
import { fileUrlFromMarkdownPath } from "./assetUrl";
import { copySelectedMarkdown, cutSelectedMarkdown } from "./copyMarkdown";
import { shouldApplyEditorMarkdownUpdate } from "./editorSync";
import { createMarkdownExtensions, setMarkdownContent } from "./editorExtensions";
import {
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
import { type EditorMode } from "./editorMode";
import type { SavedAsset } from "./types";

interface EditorPaneProps {
  bodyMarkdown: string;
  dataDir?: string | null;
  focusRequestId?: number;
  mode: EditorMode;
  onBodyChange: (bodyMarkdown: string) => void;
  onRequestImageSave: (bytes: number[]) => Promise<SavedAsset | null>;
  readOnly?: boolean;
}

export function EditorPane({
  bodyMarkdown,
  dataDir = null,
  focusRequestId = 0,
  mode,
  onBodyChange,
  onRequestImageSave,
  readOnly = false,
}: EditorPaneProps) {
  const suppressNextUpdate = useRef(false);
  const editorRef = useRef<Editor | null>(null);
  const uploadingImageSources = useRef(new Set<string>());
  const uploadedImageSources = useRef(new Map<string, string>());
  const hydratedImageSources = useRef(new Map<string, string>());
  const assetBlobUrls = useRef(new Map<string, string>());

  const editor = useEditor({
    extensions: createMarkdownExtensions("Write before the thought fades..."),
    content: bodyMarkdown,
    contentType: "markdown",
    editable: !readOnly && mode === "preview",
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

          if (!copySelectedMarkdown(activeEditor, event.clipboardData, imageSourceForClipboard)) {
            return false;
          }

          event.preventDefault();
          return true;
        },
        cut: (_view, event) => {
          if (readOnly) return false;

          const activeEditor = editorRef.current;
          if (!activeEditor) {
            return false;
          }

          if (!cutSelectedMarkdown(activeEditor, event.clipboardData, imageSourceForClipboard)) {
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
        click: (_view, event) => {
          const link = linkElementFromEvent(event);
          if (!link) {
            return false;
          }

          event.preventDefault();
          openLink(link);
          return true;
        },
      },
    },
    onUpdate: ({ editor }) => {
      if (suppressNextUpdate.current) return;

      const nextMarkdown = rewriteMarkdownImageSources(
        normalizeMarkdown(editor.getMarkdown() ?? ""),
        restoreImageSourceForMarkdown,
      );
      onBodyChange(nextMarkdown);
      void uploadTransientImages(editor);
    },
  });

  editorRef.current = editor;

  useEffect(() => {
    if (!editor) return;
    startupLog("editor_ready");
    void hydrateEditorImageNodes(editor);
    void uploadTransientImages(editor);
  }, [dataDir, editor]);

  useEffect(() => {
    if (!editor) return;
    editor.setEditable(!readOnly && mode === "preview");
  }, [editor, mode, readOnly]);

  useEffect(() => {
    return () => {
      for (const source of uploadedImageSources.current.keys()) {
        if (source.startsWith("blob:")) {
          URL.revokeObjectURL(source);
        }
      }
      for (const source of assetBlobUrls.current.values()) {
        URL.revokeObjectURL(source);
      }
    };
  }, []);

  useEffect(() => {
    if (!editor || readOnly || focusRequestId === 0) return;
    editor.commands.focus("end");
  }, [editor, focusRequestId, readOnly]);

  useEffect(() => {
    if (!editor) return;

    const currentMarkdown = normalizeMarkdown(editor.getMarkdown() ?? "");
    const currentStorageMarkdown = normalizeMarkdown(
      rewriteMarkdownImageSources(currentMarkdown, restoreImageSourceForMarkdown),
    );
    const nextStorageMarkdown = normalizeMarkdown(bodyMarkdown);

    if (!shouldApplyEditorMarkdownUpdate(currentStorageMarkdown, nextStorageMarkdown)) {
      return;
    }

    suppressNextUpdate.current = true;
    setMarkdownContent(editor, bodyMarkdown);
    queueMicrotask(() => {
      suppressNextUpdate.current = false;
    });
    void hydrateEditorImageNodes(editor);
  }, [bodyMarkdown, dataDir, editor]);

  if (!editor) {
    return mode === "source" ? renderSourceEditor() : <div className="editorState">Loading editor</div>;
  }

  return (
    <div className="editorShell">
      <div className={mode === "preview" ? "editorPreviewLayer" : "editorPreviewLayer hidden"}>
        <EditorContent editor={editor} className="editorSurface markdownSurface" />
      </div>
      {mode === "source" ? renderSourceEditor() : null}
      {mode === "preview" && hasTransientImageSource(bodyMarkdown) ? (
        <div className="editorHint">Uploading image...</div>
      ) : null}
    </div>
  );

  async function uploadTransientImages(activeEditor: Editor) {
    const imageType = activeEditor.state.schema.nodes.image;
    const sources: string[] = [];

    activeEditor.state.doc.descendants((node) => {
      if (node.type === imageType && isTransientImageSource(node.attrs.src)) {
        if (uploadedImageSources.current.has(node.attrs.src)) {
          return true;
        }
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

      uploadedImageSources.current.set(source, asset.markdown_path);
      hydratedImageSources.current.set(source, asset.markdown_path);
      assetBlobUrls.current.set(asset.markdown_path, source);
      onBodyChange(
        rewriteMarkdownImageSources(
          normalizeMarkdown(activeEditor.getMarkdown() ?? ""),
          restoreImageSourceForMarkdown,
        ),
      );
    } catch {
      const activeEditor = editorRef.current;
      if (activeEditor) {
        removeImageSource(activeEditor, source);
      }
    } finally {
      uploadingImageSources.current.delete(source);
    }
  }

  function renderSourceEditor() {
    return (
      <textarea
        aria-label="Note body source"
        className="editorSurface editorLoadingSurface editorSourceSurface"
        onChange={(event) => onBodyChange(event.target.value)}
        placeholder="Write before the thought fades..."
        spellCheck={false}
        value={bodyMarkdown}
      />
    );
  }

  function restoreImageSourceForMarkdown(source: string): string {
    return (
      uploadedImageSources.current.get(source)
      ?? hydratedImageSources.current.get(source)
      ?? markdownPathFromAssetUrl(source)
    );
  }

  function imageSourceForClipboard(source: string): string {
    const markdownPath = restoreImageSourceForMarkdown(source);
    return dataDir ? fileUrlFromMarkdownPath(dataDir, markdownPath) : markdownPath;
  }

  async function hydrateEditorImageNodes(activeEditor: Editor) {
    const imageType = activeEditor.state.schema.nodes.image;
    const sources = new Set<string>();

    activeEditor.state.doc.descendants((node) => {
      if (node.type === imageType && typeof node.attrs.src === "string") {
        sources.add(node.attrs.src);
      }
      return true;
    });

    for (const source of sources) {
      const markdownPath = hydratedImageSources.current.get(source) ?? source;
      if (!markdownPath.startsWith("assets/")) {
        continue;
      }

      try {
        const displaySource = await displaySourceForAsset(markdownPath);
        hydratedImageSources.current.set(displaySource, markdownPath);
        suppressNextUpdate.current = true;
        updateImageSource(activeEditor, source, displaySource);
        queueMicrotask(() => {
          suppressNextUpdate.current = false;
        });
      } catch {
        // Keep the storage path in place; a later refresh can retry hydration.
      }
    }
  }

  async function displaySourceForAsset(markdownPath: string): Promise<string> {
    const cached = assetBlobUrls.current.get(markdownPath);
    if (cached) {
      return cached;
    }

    const bytes = await api.readAssetBytes(markdownPath);
    const source = blobUrlFromBytes(bytes);
    assetBlobUrls.current.set(markdownPath, source);
    return source;
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
      return true;
    });

    return updated;
  });
}

function linkElementFromEvent(event: MouseEvent): HTMLAnchorElement | null {
  const target = event.target;
  if (!(target instanceof Element) && !(target instanceof Text)) {
    return null;
  }

  const element = target instanceof Text ? target.parentElement : target;
  const link = element?.closest("a[href]");
  return link instanceof HTMLAnchorElement ? link : null;
}

function openLink(link: HTMLAnchorElement) {
  const rawHref = link.getAttribute("href") ?? "";
  if (rawHref.startsWith("#")) {
    document.getElementById(rawHref.slice(1))?.scrollIntoView({ block: "center" });
    return;
  }

  void api.openExternalUrl(link.href).catch(() => {
    window.open(link.href, "_blank", "noopener,noreferrer");
  });
}
