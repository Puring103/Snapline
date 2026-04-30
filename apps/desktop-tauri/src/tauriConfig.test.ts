import { describe, expect, it } from "vitest";
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
      width: 380,
      height: 500,
      minWidth: 320,
      minHeight: 300,
      resizable: true,
    });
  });
});
