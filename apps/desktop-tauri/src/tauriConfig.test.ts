import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

describe("tauri security config", () => {
  it("allows markdown images to load from remote http and https sources", () => {
    const config = JSON.parse(
      readFileSync(join(process.cwd(), "src-tauri", "tauri.conf.json"), "utf8"),
    );

    const csp = config.app.security.csp;

    expect(csp).toContain("img-src");
    expect(csp).toContain("https:");
    expect(csp).toContain("http:");
  });
});
