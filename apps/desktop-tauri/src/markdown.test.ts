import { describe, expect, it } from "vitest";
import {
  assetUrlFromMarkdownPath,
  composeDraftMarkdown,
  markdownTextFromClipboard,
  imageMarkdown,
  hasTransientImageSource,
  normalizeMarkdown,
  markdownPathFromAssetUrl,
  replaceMarkdownImageSource,
  rewriteMarkdownImageSources,
  splitDraftMarkdown,
  stripTransientImageSources,
  titleFromMarkdown,
} from "./markdown";

describe("markdown helpers", () => {
  it("normalizes line endings and trims trailing whitespace", () => {
    expect(normalizeMarkdown("# A\r\nBody\n\n")).toBe("# A\nBody");
  });

  it("derives a title from the first visible markdown line", () => {
    expect(titleFromMarkdown("\n## Heading\n# Primary\nBody")).toBe("Primary");
    expect(titleFromMarkdown("   \n")).toBe("Untitled");
  });

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
      rewriteMarkdownImageSources("![](assets/notes/note/image.png)", assetUrlFromMarkdownPath),
    ).toBe("![](asset://localhost/assets/notes/note/image.png)");
  });

  it("round-trips asset urls back to markdown paths", () => {
    expect(
      markdownPathFromAssetUrl("asset://localhost/assets/notes/note/image.png"),
    ).toBe("assets/notes/note/image.png");
  });

  it("composes and splits draft markdown", () => {
    const draft = composeDraftMarkdown("Title", "Body line");
    expect(draft).toBe("# Title\n\nBody line");
    expect(splitDraftMarkdown(draft)).toEqual({ title: "Title", body_md: "Body line" });
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
});
