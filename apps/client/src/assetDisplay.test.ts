import { describe, expect, it, vi } from "vitest";
import { blobUrlFromBytes } from "./features/assets/assetDisplay";

describe("asset display urls", () => {
  it("creates a png blob url from asset bytes", () => {
    const createdBlobs: Blob[] = [];
    const createObjectUrl = vi.fn((blob: Blob) => {
      createdBlobs.push(blob);
      return "blob:asset";
    });

    expect(blobUrlFromBytes([137, 80, 78, 71], createObjectUrl)).toBe("blob:asset");
    expect(createObjectUrl).toHaveBeenCalledTimes(1);
    const createdBlob = createdBlobs[0];
    expect(createdBlob?.type).toBe("image/png");
    expect(createdBlob?.size).toBe(4);
  });
});
