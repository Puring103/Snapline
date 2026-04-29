import Image from "@tiptap/extension-image";
import Link from "@tiptap/extension-link";
import Placeholder from "@tiptap/extension-placeholder";
import StarterKit from "@tiptap/starter-kit";
import { Markdown } from "@tiptap/markdown";
import { EditorContent, useEditor } from "@tiptap/react";
import { useEffect, useRef } from "react";
import { api } from "./api";
import { normalizeMarkdown, replaceMarkdownImageSource } from "./markdown";
import type { Note } from "./types";

interface EditorPaneProps {
  note: Note;
  onAssetResolved: (markdownPath: string) => Promise<string>;
  onSaved: (note: Note) => void;
  setStatus: (status: string) => void;
}

export function EditorPane({ note, onAssetResolved, onSaved, setStatus }: EditorPaneProps) {
  const timer = useRef<number | null>(null);
  const pendingMarkdown = useRef(note.content_md);

  const editor = useEditor({
    extensions: [
      StarterKit,
      Link,
      Image.configure({ inline: false }),
      Markdown,
      Placeholder.configure({
        placeholder: "Write before the thought fades...",
      }),
    ],
    content: note.content_md,
    contentType: "markdown",
    editorProps: {
      handlePaste: (view, event) => {
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
          const bytes = Array.from(new Uint8Array(buffer));
          const asset = await api.savePngAsset(note.id, bytes);
          const resolved = await onAssetResolved(asset.markdown_path);
          const currentMarkdown = editor?.getMarkdown() ?? "";

          editor?.commands.command(({ tr, state, dispatch }) => {
            const imageType = state.schema.nodes.image;
            let updated = false;

            state.doc.descendants((node, pos) => {
              if (node.type === imageType && node.attrs.src === placeholderSrc) {
                tr.setNodeMarkup(pos, undefined, {
                  ...node.attrs,
                  src: resolved,
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

          scheduleSave(
            replaceMarkdownImageSource(
              replaceMarkdownImageSource(currentMarkdown, placeholderSrc, asset.markdown_path),
              resolved,
              asset.markdown_path,
            ),
          );
          URL.revokeObjectURL(placeholderSrc);
        });

        return true;
      },
    },
    onUpdate: ({ editor }) => {
      scheduleSave(editor.getMarkdown());
    },
  });

  useEffect(() => {
    return () => {
      if (timer.current !== null) {
        window.clearTimeout(timer.current);
        timer.current = null;
        void saveNow(pendingMarkdown.current, false, false);
      }
    };
  }, []);

  async function saveNow(
    contentMd: string,
    notifySaved = true,
    notifyStatus = true,
  ) {
    try {
      const saved = await api.saveNote(note.id, normalizeMarkdown(contentMd));
      pendingMarkdown.current = saved.content_md;
      if (notifySaved) {
        onSaved(saved);
      }
      if (notifyStatus) {
        setStatus("Saved");
      }
    } catch (err) {
      console.error(err);
      if (notifyStatus) {
        setStatus("Error");
      }
    }
  }

  function scheduleSave(contentMd: string) {
    pendingMarkdown.current = contentMd;
    if (timer.current !== null) {
      window.clearTimeout(timer.current);
    }
    setStatus("Saving");
    timer.current = window.setTimeout(async () => {
      timer.current = null;
      await saveNow(contentMd);
    }, 600);
  }

  if (!editor) {
    return <div className="emptyState">Loading editor</div>;
  }

  return (
    <div className="editorShell">
      <EditorContent editor={editor} className="editor" />
    </div>
  );
}
