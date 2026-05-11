import { Extension } from "@tiptap/core";
import { Plugin, PluginKey } from "@tiptap/pm/state";
import { Decoration, DecorationSet } from "@tiptap/pm/view";
import type { ReactNode } from "react";

const searchHighlightKey = new PluginKey("searchHighlight");

export function searchTermsFromQuery(query: string): string[] {
  return Array.from(
    new Set(
      query
        .trim()
        .split(/\s+/)
        .map((term) => term.toLocaleLowerCase())
        .filter(Boolean),
    ),
  ).sort((a, b) => b.length - a.length);
}

export function HighlightedText({ query, text }: { query: string; text: string }) {
  const terms = searchTermsFromQuery(query);
  if (terms.length === 0) {
    return <>{text}</>;
  }

  const parts = highlightedTextParts(text, terms);
  return (
    <>
      {parts.map((part, index) =>
        part.highlight ? (
          <mark className="searchHighlight" key={`${index}-${part.text}`}>
            {part.text}
          </mark>
        ) : (
          <span key={`${index}-${part.text}`}>{part.text}</span>
        ),
      )}
    </>
  );
}

export function createSearchHighlightExtension(query?: string) {
  const terms = searchTermsFromQuery(query ?? "");

  return Extension.create({
    name: "searchHighlight",

    addProseMirrorPlugins() {
      return [
        new Plugin({
          key: searchHighlightKey,
          props: {
            decorations(state) {
              if (terms.length === 0) {
                return DecorationSet.empty;
              }

              const decorations: Decoration[] = [];
              state.doc.descendants((node, pos) => {
                if (!node.isText || !node.text) {
                  return true;
                }

                for (const match of highlightedTextParts(node.text, terms)) {
                  if (!match.highlight) {
                    continue;
                  }

                  decorations.push(
                    Decoration.inline(pos + match.from, pos + match.to, {
                      class: "searchHighlight",
                    }),
                  );
                }

                return true;
              });

              return DecorationSet.create(state.doc, decorations);
            },
          },
        }),
      ];
    },
  });
}

interface HighlightPart {
  from: number;
  highlight: boolean;
  text: string;
  to: number;
}

function highlightedTextParts(text: string, terms: string[]): HighlightPart[] {
  const ranges = highlightRanges(text, terms);
  if (ranges.length === 0) {
    return [{ from: 0, to: text.length, text, highlight: false }];
  }

  const parts: HighlightPart[] = [];
  let cursor = 0;
  for (const range of ranges) {
    if (cursor < range.from) {
      parts.push({
        from: cursor,
        to: range.from,
        text: text.slice(cursor, range.from),
        highlight: false,
      });
    }

    parts.push({
      from: range.from,
      to: range.to,
      text: text.slice(range.from, range.to),
      highlight: true,
    });
    cursor = range.to;
  }

  if (cursor < text.length) {
    parts.push({
      from: cursor,
      to: text.length,
      text: text.slice(cursor),
      highlight: false,
    });
  }

  return parts;
}

function highlightRanges(text: string, terms: string[]): Array<{ from: number; to: number }> {
  const lowerText = text.toLocaleLowerCase();
  const ranges: Array<{ from: number; to: number }> = [];
  let index = 0;

  while (index < text.length) {
    const match = terms
      .map((term) => ({ term, index: lowerText.indexOf(term, index) }))
      .filter((candidate) => candidate.index !== -1)
      .sort((a, b) => a.index - b.index || b.term.length - a.term.length)[0];

    if (!match) {
      break;
    }

    const from = match.index;
    const to = from + match.term.length;
    const previous = ranges[ranges.length - 1];
    if (!previous || from >= previous.to) {
      ranges.push({ from, to });
    }

    index = to;
  }

  return ranges;
}
