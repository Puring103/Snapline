import { EditorContent, useEditor } from "@tiptap/react";
import type { Editor } from "@tiptap/core";
import { useEffect, useRef } from "react";
import { api } from "../platform/api";
import { blobUrlFromBytes } from "../features/assets/assetDisplay";
import { createMarkdownExtensions, setMarkdownContent } from "../features/editor/editorExtensions";
import { createSearchHighlightExtension } from "../features/search/highlight";
import { startupLog } from "../platform/startupLog";

interface MarkdownPreviewProps {
  dataDir?: string | null;
  highlightQuery?: string;
  markdown: string;
}

export function MarkdownPreview({ dataDir = null, highlightQuery = "", markdown }: MarkdownPreviewProps) {
  const assetBlobUrls = useRef(new Map<string, string>());

  const editor = useEditor({
    extensions: [...createMarkdownExtensions(), createSearchHighlightExtension(highlightQuery)],
    content: markdown,
    contentType: "markdown",
    editable: false,
  }, [highlightQuery]);

  useEffect(() => {
    if (!editor) return;
    startupLog("preview_ready");
    void hydratePreviewImages(editor, assetBlobUrls.current);
  }, [dataDir, editor]);

  useEffect(() => {
    if (!editor) return;
    setMarkdownContent(editor, markdown);
    void hydratePreviewImages(editor, assetBlobUrls.current);
  }, [dataDir, editor, markdown]);

  useEffect(() => {
    return () => {
      for (const source of assetBlobUrls.current.values()) {
        URL.revokeObjectURL(source);
      }
    };
  }, []);

  if (!editor) {
    return <div className="noteRowPreview">Loading preview...</div>;
  }

  return <EditorContent editor={editor} className="noteRowPreview markdownSurface" />;
}

async function hydratePreviewImages(
  editor: Editor,
  assetBlobUrls: Map<string, string>,
): Promise<void> {
  const imageType = editor.state.schema.nodes.image;
  const sources = new Set<string>();

  editor.state.doc.descendants((node) => {
    if (node.type === imageType && typeof node.attrs.src === "string" && node.attrs.src.startsWith("assets/")) {
      sources.add(node.attrs.src);
    }
    return true;
  });

  for (const source of sources) {
    try {
      const displaySource = await displaySourceForAsset(source, assetBlobUrls);
      updatePreviewImageSource(editor, source, displaySource);
    } catch {
      // Keep the storage path in place; previews can retry on refresh.
    }
  }
}

function updatePreviewImageSource(editor: Editor, source: string, displaySource: string) {
  const imageType = editor.state.schema.nodes.image;
  editor.commands.command(({ tr, state }) => {
    let updated = false;
    state.doc.descendants((node, pos) => {
      if (node.type !== imageType || node.attrs.src !== source) {
        return true;
      }
      tr.setNodeMarkup(pos, undefined, {
        ...node.attrs,
        src: displaySource,
      });
      updated = true;
      return true;
    });

    if (updated) {
      tr.setMeta("addToHistory", false);
    }
    return updated;
  });
}

async function displaySourceForAsset(
  markdownPath: string,
  assetBlobUrls: Map<string, string>,
): Promise<string> {
  const cached = assetBlobUrls.get(markdownPath);
  if (cached) {
    return cached;
  }

  const bytes = await api.readAssetBytes(markdownPath);
  const source = blobUrlFromBytes(bytes);
  assetBlobUrls.set(markdownPath, source);
  return source;
}
