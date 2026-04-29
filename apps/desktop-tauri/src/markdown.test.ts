import { describe, expect, it } from "vitest";
import {
  imageMarkdown,
  normalizeMarkdown,
  replaceMarkdownImageSource,
  titleFromMarkdown,
} from "./markdown";

describe("markdown helpers", () => {
  it("normalizes line endings and trims trailing whitespace", () => {
    expect(normalizeMarkdown("# A\r\nBody\n\n")).toBe("# A\nBody");
  });

  it("derives a title from the first visible markdown line", () => {
    expect(titleFromMarkdown("\n## Heading\nBody")).toBe("Heading");
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
});
