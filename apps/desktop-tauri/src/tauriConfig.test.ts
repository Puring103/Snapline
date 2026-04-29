import { describe, expect, it } from "vitest";
import config from "../src-tauri/tauri.conf.json";

describe("tauri security config", () => {
  it("allows markdown images to load from remote http and https sources", () => {
    const csp = config.app.security.csp;

    expect(csp).toContain("img-src");
    expect(csp).toContain("https:");
    expect(csp).toContain("http:");
  });
});
