import { describe, expect, it } from "vitest";
import { isTransientImageSource, uploadedImageDisplaySource } from "./imageUploadDisplay";

describe("image upload display sources", () => {
  it("keeps pasted blob urls visible after upload while storage maps them to the asset path", () => {
    expect(uploadedImageDisplaySource("blob:pasted-image", "asset://localhost/assets/note/image.png")).toBe(
      "blob:pasted-image",
    );
  });

  it("keeps pasted data urls visible after upload while storage maps them to the asset path", () => {
    expect(uploadedImageDisplaySource("data:image/png;base64,iVBORw0=", "asset://localhost/assets/note/image.png")).toBe(
      "data:image/png;base64,iVBORw0=",
    );
  });

  it("uses the saved asset url for non-transient image sources", () => {
    expect(uploadedImageDisplaySource("https://example.com/image.png", "asset://localhost/assets/note/image.png")).toBe(
      "asset://localhost/assets/note/image.png",
    );
  });

  it("recognizes only blob and data urls as transient editor display sources", () => {
    expect(isTransientImageSource("blob:pasted-image")).toBe(true);
    expect(isTransientImageSource("data:image/png;base64,iVBORw0=")).toBe(true);
    expect(isTransientImageSource("asset://localhost/assets/note/image.png")).toBe(false);
  });
});
