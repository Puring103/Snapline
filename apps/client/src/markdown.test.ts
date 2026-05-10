import { describe, expect, it } from "vitest";
import {
  codeBlockLanguageClass,
  markdownTextFromClipboard,
  imageMarkdown,
  hasTransientImageSource,
  markdownPathFromAssetUrl,
  replaceMarkdownImageSource,
  rewriteMarkdownImageSources,
  stripTransientImageSources,
} from "./features/editor/markdown";

describe("markdown helpers", () => {
  it("renders markdown image references", () => {
    expect(imageMarkdown("assets/notes/note/image.png")).toBe(
      "![](assets/notes/note/image.png)",
    );
  });

  it("replaces a markdown image source inline", () => {
    const source = [
      "Paragraph",
      "",
      "![](blob:temp-image)",
      "",
      "![alt](assets/notes/note/keep.png)",
    ].join("\n");

    expect(replaceMarkdownImageSource(source, "blob:temp-image", "assets/notes/note/image.png")).toBe(
      ["Paragraph", "", "![](assets/notes/note/image.png)", "", "![alt](assets/notes/note/keep.png)"].join(
        "\n",
      ),
    );
  });

  it("rewrites markdown image sources through a mapper", () => {
    expect(
      rewriteMarkdownImageSources(
        "![](assets/notes/note/image.png)",
        (source) => `asset://localhost/${source}`,
      ),
    ).toBe("![](asset://localhost/assets/notes/note/image.png)");
  });

  it("round-trips asset urls back to markdown paths", () => {
    expect(
      markdownPathFromAssetUrl("asset://localhost/assets/notes/note/image.png"),
    ).toBe("assets/notes/note/image.png");
  });

  it("detects transient pasted image sources", () => {
    expect(hasTransientImageSource("![](blob:temp-image)")).toBe(true);
    expect(hasTransientImageSource("![](asset://localhost/assets/notes/note/image.png)")).toBe(false);
  });

  it("strips transient image sources before a draft is persisted", () => {
    expect(stripTransientImageSources("Keep\n\n![](blob:temp-image)\n\nMore")).toBe(
      "Keep\n\n\n\nMore",
    );
  });

  it("prefers markdown clipboard text over plain text", () => {
    expect(
      markdownTextFromClipboard({
        getData: (type: string) => {
          if (type === "text/markdown") return "# Heading\n\n- Item";
          if (type === "text/plain") return "# plain";
          return "";
        },
      }),
    ).toBe("# Heading\n\n- Item");
  });

  it("falls back to plain text clipboard content", () => {
    expect(
      markdownTextFromClipboard({
        getData: (type: string) => {
          if (type === "text/plain") return "## Plain";
          return "";
        },
      }),
    ).toBe("## Plain");
  });

  it("normalizes code block language classes for highlighting", () => {
    expect(codeBlockLanguageClass("TypeScript")).toBe("language-typescript");
    expect(codeBlockLanguageClass("c++")).toBe("language-c");
    expect(codeBlockLanguageClass("")).toBe(null);
  });
});
