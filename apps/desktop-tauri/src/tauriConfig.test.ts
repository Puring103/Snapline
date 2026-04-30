import { describe, expect, it } from "vitest";
import mainCapability from "../src-tauri/capabilities/main.json";
import config from "../src-tauri/tauri.conf.json";

describe("tauri security config", () => {
  it("allows markdown images to load from remote http and https sources", () => {
    const csp = config.app.security.csp;

    expect(csp).toContain("img-src");
    expect(csp).toContain("https:");
    expect(csp).toContain("http:");
  });

  it("opens the main note window at the compact note size", () => {
    expect(config.app.windows[0]).toMatchObject({
      url: "/?mode=note",
      width: 340,
      height: 440,
      minWidth: 300,
      minHeight: 260,
      resizable: true,
      decorations: false,
    });
  });

  it("allows the custom chrome to start native window dragging", () => {
    expect(mainCapability.permissions).toContain("core:window:allow-start-dragging");
  });
});
