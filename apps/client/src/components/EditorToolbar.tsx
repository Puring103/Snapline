import type { Editor } from "@tiptap/core";
import type { ReactNode } from "react";
import { type EditorMode } from "../features/editor/editorMode";

interface EditorToolbarProps {
  editor: Editor | null;
  mode: EditorMode;
  onModeToggle: () => void;
  showModeToggle: boolean;
  stateVersion: number;
  variant: "compact" | "full";
}

export function EditorToolbar({
  editor,
  mode,
  onModeToggle,
  showModeToggle,
  stateVersion: _stateVersion,
  variant,
}: EditorToolbarProps) {
  if (!editor && !showModeToggle) {
    return null;
  }

  return (
    <div className="editorToolbar">
      {editor ? (
        <>
          {variant === "full" ? (
            <>
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
              <ToolbarButton
                active={editor.isActive("heading", { level: 3 })}
                label="Heading 3"
                onClick={() => editor.chain().focus().toggleHeading({ level: 3 }).run()}
              >
                <HeadingIcon level={3} />
              </ToolbarButton>
              <div className="editorToolbarDivider" />
            </>
          ) : null}
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
            active={editor.isActive("strike")}
            label="Strikethrough"
            onClick={() => editor.chain().focus().toggleStrike().run()}
          >
            <StrikeIcon />
          </ToolbarButton>
          <ToolbarButton
            active={editor.isActive("underline")}
            label="Underline"
            onClick={() => editor.chain().focus().toggleUnderline().run()}
          >
            <UnderlineIcon />
          </ToolbarButton>
          <ToolbarButton
            active={editor.isActive("code")}
            label="Inline code"
            onClick={() => editor.chain().focus().toggleCode().run()}
          >
            <InlineCodeIcon />
          </ToolbarButton>
          <div className="editorToolbarDivider" />
          {variant === "full" ? (
            <>
              <ToolbarButton
                disabled={!editor.can().undo()}
                active={false}
                label="Undo"
                onClick={() => editor.chain().focus().undo().run()}
              >
                <UndoIcon />
              </ToolbarButton>
              <ToolbarButton
                disabled={!editor.can().redo()}
                active={false}
                label="Redo"
                onClick={() => editor.chain().focus().redo().run()}
              >
                <RedoIcon />
              </ToolbarButton>
              <div className="editorToolbarDivider" />
            </>
          ) : null}
          {variant === "full" ? (
            <>
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
                active={false}
                label="Indent"
                onClick={() => indentSelection(editor)}
              >
                <IndentIcon />
              </ToolbarButton>
              <ToolbarButton
                active={false}
                label="Outdent"
                onClick={() => outdentSelection(editor)}
              >
                <OutdentIcon />
              </ToolbarButton>
            </>
          ) : null}
          <ToolbarButton
            active={editor.isActive("taskList")}
            label="Task list"
            onClick={() => editor.chain().focus().toggleTaskList().run()}
          >
            <TaskListIcon />
          </ToolbarButton>
          <div className="editorToolbarDivider" />
          {variant === "full" ? (
            <ToolbarButton
              active={editor.isActive("blockquote")}
              label="Blockquote"
              onClick={() => editor.chain().focus().toggleBlockquote().run()}
            >
              <BlockquoteIcon />
            </ToolbarButton>
          ) : null}
          <ToolbarButton
            active={editor.isActive("codeBlock")}
            label="Code block"
            onClick={() => editor.chain().focus().toggleCodeBlock().run()}
          >
            <CodeBlockIcon />
          </ToolbarButton>
        </>
      ) : null}
      {showModeToggle ? (
        <>
          <div className="editorToolbarSpacer" />
          <div className="editorToolbarDivider" />
          <ToolbarButton
            active={false}
            label={mode === "preview" ? "Switch to source mode" : "Switch to preview mode"}
            onClick={onModeToggle}
          >
            {mode === "preview" ? <SourceModeToolbarIcon /> : <PreviewModeToolbarIcon />}
          </ToolbarButton>
        </>
      ) : null}
    </div>
  );
}

function ToolbarButton({
  active,
  disabled = false,
  label,
  onClick,
  children,
}: {
  active: boolean;
  disabled?: boolean;
  label: string;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      aria-label={label}
      className={active ? "editorToolbarBtn editorToolbarBtnActive" : "editorToolbarBtn"}
      disabled={disabled}
      onMouseDown={(event) => {
        event.preventDefault();
        if (disabled) return;
        onClick();
      }}
      title={label}
      type="button"
    >
      {children}
    </button>
  );
}

function indentSelection(editor: Editor) {
  if (editor.isActive("taskItem")) {
    editor.chain().focus().sinkListItem("taskItem").run();
    return;
  }
  if (editor.isActive("bulletList") || editor.isActive("orderedList")) {
    editor.chain().focus().sinkListItem("listItem").run();
    return;
  }
  editor.chain().focus().insertContent("  ").run();
}

function outdentSelection(editor: Editor) {
  if (editor.isActive("taskItem")) {
    editor.chain().focus().liftListItem("taskItem").run();
    return;
  }
  if (editor.isActive("bulletList") || editor.isActive("orderedList")) {
    editor.chain().focus().liftListItem("listItem").run();
  }
}

function UndoIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M9 7 5 11l4 4" /><path d="M5 11h9a5 5 0 0 1 0 10h-1" /></svg>;
}

function RedoIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="m15 7 4 4-4 4" /><path d="M19 11h-9a5 5 0 0 0 0 10h1" /></svg>;
}

function BoldIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 4h8a4 4 0 0 1 0 8H6z" /><path d="M6 12h9a4 4 0 0 1 0 8H6z" /></svg>;
}

function ItalicIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M19 4h-9M14 20H5M15 4 9 20" /></svg>;
}

function InlineCodeIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M9 8L5 12l4 4M15 8l4 4-4 4" /></svg>;
}

function StrikeIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 12h12" /><path d="M16 6.5A4.8 4.8 0 0 0 12.3 5H10a3 3 0 0 0 0 6h4a3 3 0 0 1 0 6h-2.5A5 5 0 0 1 7 14.5" /></svg>;
}

function UnderlineIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 4v7a5 5 0 0 0 10 0V4" /><path d="M5 21h14" /></svg>;
}

function HeadingIcon({ level }: { level: 1 | 2 | 3 }) {
  const levelPath = level === 1
    ? "M20 19v-5l-2 1"
    : level === 2
      ? "M18 15.5a2 2 0 0 1 4 0c0 1.8-4 3.5-4 3.5h4"
      : "M18 14h3l-2 2a2 2 0 1 1-1 3.7";
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 5v14M16 5v14M5 12h11" /><path d={levelPath} /></svg>;
}

function BulletListIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M9 6h11M9 12h11M9 18h11" /><path d="M4 6h.01M4 12h.01M4 18h.01" /></svg>;
}

function OrderedListIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M10 6h10M10 12h10M10 18h10" /><path d="M4 5h1v4M3.8 9h2.4M3.7 11.5h2.1L4 14h2M4 17h1.7a1 1 0 0 1 0 2H4.4M5.7 19a1 1 0 0 1 0 2H4" /></svg>;
}

function TaskListIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="5" width="5" height="5" rx="1" /><path d="M4.5 7.5l1 1 2-2" /><path d="M10 7.5h10M10 12.5h10M10 17.5h10" /><rect x="3" y="10" width="5" height="5" rx="1" /><rect x="3" y="15" width="5" height="5" rx="1" /></svg>;
}

function BlockquoteIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 7h10M4 12h8M4 17h10" /><path d="M19 7c-2 1-2 3.5-2 3.5H20V16h-5v-5c0-2.6 1.4-4.6 3.7-5.8z" /></svg>;
}

function CodeBlockIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><rect x="2" y="4" width="20" height="16" rx="3" /><path d="M8 9l-3 3 3 3M16 9l3 3-3 3M13 8l-2 8" /></svg>;
}

function IndentIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 6h16M12 12h8M4 18h16" /><path d="m4 10 4 2-4 2z" /></svg>;
}

function OutdentIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 6h16M12 12h8M4 18h16" /><path d="m8 10-4 2 4 2z" /></svg>;
}

function SourceModeToolbarIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M9 7l-4 5 4 5M15 7l4 5-4 5M13 5l-2 14" /></svg>;
}

function PreviewModeToolbarIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3.5 12s3.5-6.5 8.5-6.5 8.5 6.5 8.5 6.5-3.5 6.5-8.5 6.5S3.5 12 3.5 12Z" /><path d="M12 9.2a2.8 2.8 0 1 0 0 5.6 2.8 2.8 0 0 0 0-5.6Z" /></svg>;
}
