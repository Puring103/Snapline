import { EditorContent, useEditor } from "@tiptap/react";
import type { Editor } from "@tiptap/core";
import type { EditorView } from "@tiptap/pm/view";
import { useEffect, useRef, type ReactNode } from "react";
import { api } from "../platform/api";
import { blobUrlFromBytes } from "../features/assets/assetDisplay";
import { fileUrlFromMarkdownPath } from "../features/assets/assetUrl";
import { copySelectedMarkdown, cutSelectedMarkdown } from "../features/editor/copyMarkdown";
import { shouldApplyEditorMarkdownUpdate } from "../features/editor/editorSync";
import { createMarkdownExtensions, setMarkdownContent } from "../features/editor/editorExtensions";
import { isTransientImageSource, uploadedImageDisplaySource } from "../features/assets/imageUploadDisplay";
import {
  hasTransientImageSource,
  markdownPathFromAssetUrl,
  markdownTextFromClipboard,
  normalizeMarkdown,
  rewriteMarkdownImageSources,
} from "../features/editor/markdown";
import {
  bytesFromPastedImageFile,
  bytesFromTransientImageSource,
  hasPotentialAsyncImageClipboardSource,
  objectUrlFromImageBytes,
  pastedImageSourceFromClipboardAsync,
  pastedImageSourceFromClipboard,
} from "../features/editor/pasteImage";
import { insertClipboardMarkdown } from "../features/editor/pasteMarkdown";
import { startupLog } from "../platform/startupLog";
import { type EditorMode } from "../features/editor/editorMode";
import type { SavedAsset } from "../types";

interface EditorPaneProps {
  bodyMarkdown: string;
  dataDir?: string | null;
  focusRequestId?: number;
  mode: EditorMode;
  onBodyChange: (bodyMarkdown: string) => void;
  onRequestImageSave: (bytes: number[]) => Promise<SavedAsset | null>;
  readOnly?: boolean;
  showToolbar?: boolean;
}

export function EditorPane({
  bodyMarkdown,
  dataDir = null,
  focusRequestId = 0,
  mode,
  onBodyChange,
  onRequestImageSave,
  readOnly = false,
  showToolbar = false,
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

          const imageSource = pastedImageSourceFromClipboard(clipboardData);
          if (imageSource) {
            event.preventDefault();
            logImagePaste("dom_image_source", imageSource.kind);
            pasteImageSource(view, imageSource);
            return true;
          }

          if (hasPotentialAsyncImageClipboardSource(clipboardData)) {
            event.preventDefault();
            void pastedImageSourceFromClipboardAsync(clipboardData).then((asyncImageSource) => {
              if (asyncImageSource) {
                logImagePaste("async_image_source", asyncImageSource.kind);
                pasteImageSource(view, asyncImageSource);
                return;
              }

              const fallbackMarkdownText = markdownTextFromClipboard(clipboardData);
              const activeEditor = editorRef.current;
              if (fallbackMarkdownText && activeEditor) {
                insertClipboardMarkdown(activeEditor, fallbackMarkdownText);
              }
            });
            return true;
          }

          const markdownText = markdownTextFromClipboard(clipboardData);
          if (markdownText) {
            const activeEditor = editorRef.current;
            if (!activeEditor) {
              return false;
            }

            event.preventDefault();
            return insertClipboardMarkdown(activeEditor, markdownText);
          }

          event.preventDefault();
          void pasteNativeClipboardImage(view);
          return true;
        },
        click: (_view, event) => {
          const link = linkElementFromEvent(event);
          if (!link) {
            return false;
          }

          const activeEditor = editorRef.current;
          if (!activeEditor) {
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
      {showToolbar && mode === "preview" && !readOnly ? (
        <EditorToolbar editor={editor} />
      ) : null}
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
        logImagePaste("asset_save_empty");
        removeImageSource(activeEditor, source);
        return;
      }

      logImagePaste("asset_saved", asset.markdown_path);
      const displaySource = uploadedImageDisplaySource(source, asset.asset_url);
      uploadedImageSources.current.set(displaySource, asset.markdown_path);
      hydratedImageSources.current.set(displaySource, asset.markdown_path);
      assetBlobUrls.current.set(asset.markdown_path, displaySource);
      updateImageSource(activeEditor, source, displaySource);
      onBodyChange(
        rewriteMarkdownImageSources(
          normalizeMarkdown(activeEditor.getMarkdown() ?? ""),
          restoreImageSourceForMarkdown,
        ),
      );
    } catch (err) {
      logImagePaste("upload_failed", String(err));
      const activeEditor = editorRef.current;
      if (activeEditor) {
        removeImageSource(activeEditor, source);
      }
    } finally {
      uploadingImageSources.current.delete(source);
    }
  }

  function pasteImageSource(
    view: EditorView,
    imageSource: NonNullable<Awaited<ReturnType<typeof pastedImageSourceFromClipboardAsync>>>,
  ) {
    if (imageSource.kind === "file") {
      const placeholderSrc = URL.createObjectURL(imageSource.file);
      const bytesPromise = bytesFromPastedImageFile(imageSource.file);
      uploadingImageSources.current.add(placeholderSrc);

      view.dispatch(
        view.state.tr.replaceSelectionWith(
          view.state.schema.nodes.image.create({ src: placeholderSrc }),
        ),
      );

      void uploadTransientImageSource(placeholderSrc, bytesPromise);
    } else {
      void pasteLocalImageFile(view, imageSource.path, imageSource.mimeType);
    }
  }

  async function pasteLocalImageFile(view: EditorView, path: string, mimeType: string) {
    try {
      const originalBytes = await api.readLocalImageFile(path);
      logImagePaste("local_file_read", `${path} bytes=${originalBytes.length}`);
      const placeholderSrc = objectUrlFromImageBytes(originalBytes, mimeType);
      const bytesPromise = bytesFromPastedImageFile(
        new Blob([new Uint8Array(originalBytes)], { type: mimeType }),
      );
      uploadingImageSources.current.add(placeholderSrc);

      view.dispatch(
        view.state.tr.replaceSelectionWith(
          view.state.schema.nodes.image.create({ src: placeholderSrc }),
        ),
      );

      void uploadTransientImageSource(placeholderSrc, bytesPromise);
    } catch (err) {
      logImagePaste("local_file_read_failed", `${path} ${String(err)}`);
      // Unsupported or unreadable local file paste; keep the editor unchanged.
    }
  }

  async function pasteNativeClipboardImage(view: EditorView) {
    try {
      const bytes = await api.readClipboardImagePng();
      if (!bytes) {
        logImagePaste("native_clipboard_empty");
        return;
      }

      logImagePaste("native_clipboard_image", `bytes=${bytes.length}`);
      pasteImageBytes(view, bytes, "image/png");
    } catch (err) {
      logImagePaste("native_clipboard_failed", String(err));
      // The native clipboard fallback is best-effort; normal paste already had no image data.
    }
  }

  function pasteImageBytes(view: EditorView, bytes: number[], mimeType: string) {
    const placeholderSrc = objectUrlFromImageBytes(bytes, mimeType);
    uploadingImageSources.current.add(placeholderSrc);

    view.dispatch(
      view.state.tr.replaceSelectionWith(
        view.state.schema.nodes.image.create({ src: placeholderSrc }),
      ),
    );

    void uploadTransientImageSource(placeholderSrc, Promise.resolve(bytes));
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

function EditorToolbar({ editor }: { editor: Editor | null }) {
  if (!editor) return null;

  return (
    <div className="editorToolbar">
      <ToolbarButton
        active={editor.isActive("bold")}
        label="Bold"
        onClick={() => editor.chain().focus().toggleBold().run()}
      >
        <BoldIcon />
      </ToolbarButton>
      <ToolbarButton
        active={editor.isActive("italic")}
        label="Italic"
        onClick={() => editor.chain().focus().toggleItalic().run()}
      >
        <ItalicIcon />
      </ToolbarButton>
      <ToolbarButton
        active={editor.isActive("code")}
        label="Inline code"
        onClick={() => editor.chain().focus().toggleCode().run()}
      >
        <InlineCodeIcon />
      </ToolbarButton>
      <div className="editorToolbarDivider" />
      <ToolbarButton
        active={editor.isActive("heading", { level: 1 })}
        label="Heading 1"
        onClick={() => editor.chain().focus().toggleHeading({ level: 1 }).run()}
      >
        <HeadingIcon level={1} />
      </ToolbarButton>
      <ToolbarButton
        active={editor.isActive("heading", { level: 2 })}
        label="Heading 2"
        onClick={() => editor.chain().focus().toggleHeading({ level: 2 }).run()}
      >
        <HeadingIcon level={2} />
      </ToolbarButton>
      <div className="editorToolbarDivider" />
      <ToolbarButton
        active={editor.isActive("bulletList")}
        label="Bullet list"
        onClick={() => editor.chain().focus().toggleBulletList().run()}
      >
        <BulletListIcon />
      </ToolbarButton>
      <ToolbarButton
        active={editor.isActive("orderedList")}
        label="Ordered list"
        onClick={() => editor.chain().focus().toggleOrderedList().run()}
      >
        <OrderedListIcon />
      </ToolbarButton>
      <ToolbarButton
        active={editor.isActive("taskList")}
        label="Task list"
        onClick={() => editor.chain().focus().toggleTaskList().run()}
      >
        <TaskListIcon />
      </ToolbarButton>
      <div className="editorToolbarDivider" />
      <ToolbarButton
        active={editor.isActive("blockquote")}
        label="Blockquote"
        onClick={() => editor.chain().focus().toggleBlockquote().run()}
      >
        <BlockquoteIcon />
      </ToolbarButton>
      <ToolbarButton
        active={editor.isActive("codeBlock")}
        label="Code block"
        onClick={() => editor.chain().focus().toggleCodeBlock().run()}
      >
        <CodeBlockIcon />
      </ToolbarButton>
    </div>
  );
}

function ToolbarButton({
  active,
  label,
  onClick,
  children,
}: {
  active: boolean;
  label: string;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      aria-label={label}
      className={active ? "editorToolbarBtn editorToolbarBtnActive" : "editorToolbarBtn"}
      onMouseDown={(e) => {
        e.preventDefault();
        onClick();
      }}
      title={label}
      type="button"
    >
      {children}
    </button>
  );
}

function BoldIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 4h8a4 4 0 0 1 0 8H6z"/><path d="M6 12h9a4 4 0 0 1 0 8H6z"/></svg>;
}

function ItalicIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M19 4h-9M14 20H5M15 4 9 20"/></svg>;
}

function InlineCodeIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M9 8L5 12l4 4M15 8l4 4-4 4"/></svg>;
}

function HeadingIcon({ level }: { level: 1 | 2 }) {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="M4 6v12M4 12h8M12 6v12" />
      <text x="15" y="19" style={{ fontSize: "9px", fontWeight: 700, fill: "currentColor", stroke: "none" }}>{level}</text>
    </svg>
  );
}

function BulletListIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M9 6h10M9 12h10M9 18h10"/><circle cx="4" cy="6" r="1.2" style={{ fill: "currentColor", stroke: "none" }} /><circle cx="4" cy="12" r="1.2" style={{ fill: "currentColor", stroke: "none" }} /><circle cx="4" cy="18" r="1.2" style={{ fill: "currentColor", stroke: "none" }} /></svg>;
}

function OrderedListIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M10 6h10M10 12h10M10 18h10"/><text x="2" y="9" style={{ fontSize: "8px", fontWeight: 700, fill: "currentColor", stroke: "none" }}>1</text><text x="2" y="15" style={{ fontSize: "8px", fontWeight: 700, fill: "currentColor", stroke: "none" }}>2</text><text x="2" y="21" style={{ fontSize: "8px", fontWeight: 700, fill: "currentColor", stroke: "none" }}>3</text></svg>;
}

function TaskListIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="5" width="5" height="5" rx="1"/><path d="M4.5 7.5l1 1 2-2"/><path d="M10 7.5h10M10 12.5h10M10 17.5h10"/><rect x="3" y="10" width="5" height="5" rx="1"/><rect x="3" y="15" width="5" height="5" rx="1"/></svg>;
}

function BlockquoteIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3 8h12M3 12h10M3 16h8"/><path d="M20 7c-2.5 1-2.5 3.5-2.5 3.5H20v4h-4V11c0-3 2-5 4-6l.5 2z"/></svg>;
}

function CodeBlockIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><rect x="2" y="4" width="20" height="16" rx="3"/><path d="M8 9l-3 3 3 3M16 9l3 3-3 3M13 8l-2 8"/></svg>;
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

function logImagePaste(event: string, detail = "") {
  console.info(`[snapline:image-paste] ${event}${detail ? ` ${detail}` : ""}`);
}
