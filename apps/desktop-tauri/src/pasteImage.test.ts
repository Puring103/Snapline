import { describe, expect, it, vi } from "vitest";
import {
  bytesFromPastedImageFile,
  bytesFromTransientImageSource,
  hasPotentialAsyncImageClipboardSource,
  objectUrlFromImageBytes,
  pastedImageSourceFromClipboardAsync,
  pastedImageSourceFromClipboard,
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

  it("re-encodes non-png pasted image files through the png encoder", async () => {
    const encoder = vi.fn(async () => [1, 2, 3, 4]);
    const file = new File([new Uint8Array([255, 216, 255, 224])], "clip.jpg", {
      type: "image/jpeg",
    });

    await expect(bytesFromPastedImageFile(file, encoder)).resolves.toEqual([1, 2, 3, 4]);
    expect(encoder).toHaveBeenCalledWith(file);
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

  it("accepts image clipboard items even when the item kind is not file", () => {
    const file = new File(["image"], "clip.png", { type: "image/png" });

    expect(
      pastedImageFileFromClipboard({
        files: [],
        items: [
          {
            kind: "string",
            type: "image/png",
            getAsFile: () => file,
          },
        ],
      }),
    ).toBe(file);
  });

  it("extracts pasted images from html data urls", () => {
    const file = pastedImageFileFromClipboard({
      getData: (type: string) => {
        if (type === "text/html") {
          return '<img src="data:image/png;base64,iVBORw==">';
        }
        return "";
      },
    });

    expect(file).toBeInstanceOf(File);
    expect(file?.type).toBe("image/png");
  });

  it("extracts Linux file URI images from uri-list clipboard data", () => {
    expect(
      pastedImageSourceFromClipboard({
        getData: (type: string) => {
          if (type === "text/uri-list") {
            return "# copied from file manager\nfile:///home/wtl/Pictures/Screenshot%202026-05-01.png\n";
          }
          return "";
        },
      }),
    ).toEqual({
      kind: "local-file",
      path: "/home/wtl/Pictures/Screenshot 2026-05-01.png",
      mimeType: "image/png",
    });
  });

  it("extracts Linux file URI images from plain text clipboard data", () => {
    expect(
      pastedImageSourceFromClipboard({
        getData: (type: string) => {
          if (type === "text/plain") {
            return "file:///home/wtl/Pictures/plain.bmp";
          }
          return "";
        },
      }),
    ).toEqual({
      kind: "local-file",
      path: "/home/wtl/Pictures/plain.bmp",
      mimeType: "image/bmp",
    });
  });

  it("extracts Linux file URI images from async uri-list clipboard items", async () => {
    await expect(
      pastedImageSourceFromClipboardAsync({
        items: [
          {
            kind: "string",
            type: "text/uri-list",
            getAsFile: () => null,
            getAsString: (callback: (value: string) => void) => {
              callback("file:///home/wtl/Pictures/async.png");
            },
          },
        ],
      }),
    ).resolves.toEqual({
      kind: "local-file",
      path: "/home/wtl/Pictures/async.png",
      mimeType: "image/png",
    });
  });

  it("detects Linux async image clipboard sources before falling back to text paste", () => {
    expect(
      hasPotentialAsyncImageClipboardSource({
        getData: (type: string) => (type === "text/plain" ? "file:///home/wtl/Pictures/async.png" : ""),
        items: [
          {
            kind: "string",
            type: "text/uri-list",
            getAsFile: () => null,
            getAsString: (callback: (value: string) => void) => {
              callback("file:///home/wtl/Pictures/async.png");
            },
          },
        ],
      }),
    ).toBe(true);
  });

  it("does not flag non-image text clipboard items as async image sources", () => {
    expect(
      hasPotentialAsyncImageClipboardSource({
        items: [
          {
            kind: "string",
            type: "application/x-custom",
            getAsFile: () => null,
            getAsString: (callback: (value: string) => void) => {
              callback("plain text");
            },
          },
        ],
      }),
    ).toBe(false);
  });

  it("extracts GNOME copied file image URI clipboard items", async () => {
    await expect(
      pastedImageSourceFromClipboardAsync({
        items: [
          {
            kind: "string",
            type: "x-special/gnome-copied-files",
            getAsFile: () => null,
            getAsString: (callback: (value: string) => void) => {
              callback("copy\nfile:///home/wtl/Pictures/gnome.webp\n");
            },
          },
        ],
      }),
    ).resolves.toEqual({
      kind: "local-file",
      path: "/home/wtl/Pictures/gnome.webp",
      mimeType: "image/webp",
    });
  });

  it("extracts Linux file URI images from pasted html", () => {
    expect(
      pastedImageSourceFromClipboard({
        getData: (type: string) => {
          if (type === "text/html") {
            return '<img src="file:///home/wtl/Pictures/photo.jpg">';
          }
          return "";
        },
      }),
    ).toEqual({
      kind: "local-file",
      path: "/home/wtl/Pictures/photo.jpg",
      mimeType: "image/jpeg",
    });
  });

  it("ignores non-image local file URI clipboard data", () => {
    expect(
      pastedImageSourceFromClipboard({
        getData: (type: string) => {
          if (type === "text/uri-list") {
            return "file:///home/wtl/Documents/report.pdf";
          }
          return "";
        },
      }),
    ).toBeNull();
  });

  it("creates a displayable object url for local image bytes", () => {
    const createdBlobs: Blob[] = [];
    const createObjectUrl = vi.fn((blob: Blob) => {
      createdBlobs.push(blob);
      return "blob:local-image";
    });

    expect(objectUrlFromImageBytes([255, 216, 255, 224], "image/jpeg", createObjectUrl)).toBe(
      "blob:local-image",
    );
    expect(createdBlobs[0]?.type).toBe("image/jpeg");
    expect(createdBlobs[0]?.size).toBe(4);
  });
});
