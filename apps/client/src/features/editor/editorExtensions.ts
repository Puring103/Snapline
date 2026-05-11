import CodeBlockLowlight from "@tiptap/extension-code-block-lowlight";
import Image from "@tiptap/extension-image";
import Link from "@tiptap/extension-link";
import { Mathematics } from "@tiptap/extension-mathematics";
import { TaskItem, TaskList } from "@tiptap/extension-list";
import Placeholder from "@tiptap/extension-placeholder";
import { Table } from "@tiptap/extension-table";
import { TableCell } from "@tiptap/extension-table-cell";
import { TableHeader } from "@tiptap/extension-table-header";
import { TableRow } from "@tiptap/extension-table-row";
import StarterKit from "@tiptap/starter-kit";
import { Markdown } from "@tiptap/markdown";
import { all, createLowlight } from "lowlight";
import Underline from "@tiptap/extension-underline";
import { mergeAttributes, Node, type Editor } from "@tiptap/core";
import { codeBlockLanguageClass } from "./markdown";

const lowlight = createLowlight(all);

const FootnoteReference = Node.create({
  name: "footnoteReference",
  group: "inline",
  inline: true,
  atom: true,

  addAttributes() {
    return {
      label: {
        default: "",
      },
    };
  },

  parseHTML() {
    return [{ tag: "sup[data-footnote-reference]" }];
  },

  renderHTML({ node, HTMLAttributes }) {
    const label = String(node.attrs.label ?? "");
    const safeId = footnoteDomId(label);
    return [
      "sup",
      mergeAttributes(HTMLAttributes, {
        "data-footnote-reference": label,
        class: "footnoteReference",
      }),
      ["a", { href: `#${safeId}`, "data-footnote-link": label }, label],
    ];
  },

  markdownTokenizer: {
    name: "footnoteReference",
    level: "inline" as const,
    start: "[^",
    tokenize(src: string) {
      const match = src.match(/^\[\^([^\]\s]+)](?!:)/);
      if (!match) {
        return undefined;
      }

      return {
        type: "footnoteReference",
        raw: match[0],
        label: match[1],
      };
    },
  },

  parseMarkdown: (token, helpers) =>
    helpers.createNode("footnoteReference", { label: token.label ?? token.text ?? "" }),

  renderMarkdown: (node) => `[^${node.attrs?.label ?? ""}]`,
});

const FootnoteDefinition = Node.create({
  name: "footnoteDefinition",
  group: "block",
  content: "inline*",

  addAttributes() {
    return {
      label: {
        default: "",
      },
    };
  },

  parseHTML() {
    return [{ tag: "div[data-footnote-definition]" }];
  },

  renderHTML({ node, HTMLAttributes }) {
    const label = String(node.attrs.label ?? "");
    return [
      "div",
      mergeAttributes(HTMLAttributes, {
        id: footnoteDomId(label),
        "data-footnote-definition": label,
        class: "footnoteDefinition",
      }),
      ["span", { class: "footnoteDefinitionLabel" }, `${label}. `],
      ["span", { class: "footnoteDefinitionContent" }, 0],
    ];
  },

  markdownTokenizer: {
    name: "footnoteDefinition",
    level: "block" as const,
    start(src: string) {
      const index = src.search(/(^|\n) {0,3}\[\^[^\]\s]+]:/);
      return index === -1 ? -1 : index;
    },
    tokenize(src: string, _tokens, lexer) {
      const match = src.match(/^ {0,3}\[\^([^\]\s]+)]:[ \t]*(.*)(?:\n|$)/);
      if (!match) {
        return undefined;
      }

      return {
        type: "footnoteDefinition",
        raw: match[0],
        label: match[1],
        tokens: lexer.inlineTokens(match[2] ?? ""),
      };
    },
  },

  parseMarkdown: (token, helpers) =>
    helpers.createNode(
      "footnoteDefinition",
      { label: token.label ?? "" },
      helpers.parseInline(token.tokens ?? []),
    ),

  renderMarkdown: (node, helpers) => {
    const label = node.attrs?.label ?? "";
    const content = helpers.renderChildren(node.content ?? []);
    return `[^${label}]: ${content}`;
  },
});

export function createMarkdownExtensions(placeholder?: string) {
  return [
    StarterKit.configure({
      codeBlock: false,
      link: false,
      underline: false,
    }),
    Underline,
    Image.configure({ inline: false }),
    Link.configure({
      autolink: true,
      linkOnPaste: true,
      openOnClick: true,
      HTMLAttributes: {
        target: "_blank",
        rel: "noopener noreferrer",
      },
    }),
    Mathematics.configure({
      katexOptions: {
        displayMode: false,
        throwOnError: false,
      },
    }),
    FootnoteReference,
    FootnoteDefinition,
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

function footnoteDomId(label: string): string {
  return `fn-${label.replace(/[^a-zA-Z0-9_-]+/g, "-")}`;
}
