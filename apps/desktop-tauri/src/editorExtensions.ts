import CodeBlockLowlight from "@tiptap/extension-code-block-lowlight";
import Image from "@tiptap/extension-image";
import { TaskItem, TaskList } from "@tiptap/extension-list";
import Placeholder from "@tiptap/extension-placeholder";
import { Table } from "@tiptap/extension-table";
import { TableCell } from "@tiptap/extension-table-cell";
import { TableHeader } from "@tiptap/extension-table-header";
import { TableRow } from "@tiptap/extension-table-row";
import StarterKit from "@tiptap/starter-kit";
import { Markdown } from "@tiptap/markdown";
import { all, createLowlight } from "lowlight";
import type { Editor } from "@tiptap/core";
import { codeBlockLanguageClass } from "./markdown";

const lowlight = createLowlight(all);

export function createMarkdownExtensions(placeholder?: string) {
  return [
    StarterKit.configure({
      codeBlock: false,
    }),
    Image.configure({ inline: false }),
    TaskList,
    TaskItem.configure({
      nested: true,
    }),
    Table.configure({
      resizable: true,
    }),
    TableRow,
    TableHeader,
    TableCell,
    CodeBlockLowlight.configure({
      lowlight,
      HTMLAttributes: {
        class: "codeBlock",
      },
    }).extend({
      renderHTML({ node, HTMLAttributes }) {
        const languageClass = codeBlockLanguageClass(node.attrs.language);
        const classes = ["codeBlock", languageClass, HTMLAttributes.class].filter(Boolean).join(" ");

        return [
          "pre",
          { ...HTMLAttributes, class: classes },
          ["code", {}, 0],
        ];
      },
    }),
    Markdown,
    Placeholder.configure({
      placeholder: placeholder ?? "",
    }),
  ];
}

export function setMarkdownContent(editor: Editor, markdown: string) {
  editor.commands.setContent(markdown, { contentType: "markdown" });
}
