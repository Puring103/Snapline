import CodeBlockLowlight from "@tiptap/extension-code-block-lowlight";
import Image from "@tiptap/extension-image";
import Placeholder from "@tiptap/extension-placeholder";
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
