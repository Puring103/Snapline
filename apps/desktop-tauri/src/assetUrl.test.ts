import { describe, expect, it, vi } from "vitest";
import {
  fileUrlFromMarkdownPath,
  webviewAssetUrlFromFilesystemPath,
  webviewAssetUrlFromMarkdownPath,
} from "./features/assets/assetUrl";

describe("asset urls", () => {
  it("derives a webview url from a filesystem path", () => {
    const convertFileSrc = vi.fn((value: string) => `asset://localhost/${value}`);

    expect(webviewAssetUrlFromFilesystemPath("C:/Snapline/assets/note/image.png", convertFileSrc)).toBe(
      "asset://localhost/C:/Snapline/assets/note/image.png",
    );
    expect(convertFileSrc).toHaveBeenCalledWith("C:/Snapline/assets/note/image.png");
  });

  it("derives a webview url from a markdown asset path", () => {
    const convertFileSrc = vi.fn((value: string) => `asset://localhost/${value}`);

    expect(
      webviewAssetUrlFromMarkdownPath(
        "C:/Users/wtl/AppData/Roaming/Snapline",
        "assets/notes/note/image.png",
        convertFileSrc,
      ),
    ).toBe("asset://localhost/C:/Users/wtl/AppData/Roaming/Snapline\\assets\\notes\\note\\image.png");
    expect(convertFileSrc).toHaveBeenCalledWith(
      "C:/Users/wtl/AppData/Roaming/Snapline\\assets\\notes\\note\\image.png",
    );
  });

  it("leaves non-asset markdown paths unchanged", () => {
    const convertFileSrc = vi.fn((value: string) => `asset://localhost/${value}`);

    expect(webviewAssetUrlFromMarkdownPath("C:/Snapline", "https://example.com/a.png", convertFileSrc)).toBe(
      "https://example.com/a.png",
    );
    expect(convertFileSrc).not.toHaveBeenCalled();
  });

  it("derives a portable file url from a markdown asset path", () => {
    expect(fileUrlFromMarkdownPath("C:/Users/wtl/AppData/Roaming/Snapline", "assets/notes/note/image.png")).toBe(
      "file:///C:/Users/wtl/AppData/Roaming/Snapline/assets/notes/note/image.png",
    );
  });
});
