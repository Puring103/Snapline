import { describe, expect, it, vi } from "vitest";
import {
  bytesFromPastedImageFile,
  bytesFromTransientImageSource,
  pastedImageFileFromClipboard,
} from "./pasteImage";

describe("paste image helpers", () => {
  it("reads pasted image files without fetching the blob url", async () => {
    const originalFetch = globalThis.fetch;
    globalThis.fetch = vi.fn(() => {
      throw new Error("fetch should not be used for pasted files");
    }) as unknown as typeof fetch;

    try {
      const file = new File([new Uint8Array([137, 80, 78, 71])], "clip.png", {
        type: "image/png",
      });

      await expect(bytesFromPastedImageFile(file)).resolves.toEqual([137, 80, 78, 71]);
      expect(globalThis.fetch).not.toHaveBeenCalled();
    } finally {
      globalThis.fetch = originalFetch;
    }
  });

  it("decodes transient data urls without using fetch", async () => {
    const originalFetch = globalThis.fetch;
    globalThis.fetch = vi.fn(() => {
      throw new Error("fetch should not be used for data urls");
    }) as unknown as typeof fetch;

    try {
      await expect(bytesFromTransientImageSource("data:image/png;base64,iVBORw==")).resolves.toEqual([
        137,
        80,
        78,
        71,
      ]);
      expect(globalThis.fetch).not.toHaveBeenCalled();
    } finally {
      globalThis.fetch = originalFetch;
    }
  });

  it("reads image files from clipboard items first", () => {
    const file = new File(["image"], "clip.png", { type: "image/png" });

    expect(
      pastedImageFileFromClipboard({
        files: [],
        items: [
          {
            kind: "file",
            type: "image/png",
            getAsFile: () => file,
          },
        ],
      }),
    ).toBe(file);
  });

  it("falls back to clipboard files when item data is unavailable", () => {
    const file = new File(["image"], "clip.png", { type: "image/png" });

    expect(
      pastedImageFileFromClipboard({
        files: [file],
        items: [
          {
            kind: "string",
            type: "text/plain",
            getAsFile: () => null,
          },
        ],
      }),
    ).toBe(file);
  });
});
