import { describe, expect, it } from "vitest";
import cliSchema from "@tauri-apps/cli/config.schema.json";
import mainCapability from "../src-tauri/capabilities/main.json";
import androidConfig from "../src-tauri/tauri.android.conf.json";
import config from "../src-tauri/tauri.conf.json";
import linuxConfig from "../src-tauri/tauri.linux.conf.json";

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

  it("builds the Windows installer by default", () => {
    expect(config.bundle.targets).toEqual(["nsis"]);
  });

  it("builds AppImage and deb packages on Linux", () => {
    expect(linuxConfig.bundle.targets).toEqual(["appimage", "deb"]);
  });

  it("opens the Android build with the mobile route", () => {
    expect(androidConfig.app.windows[0]).toMatchObject({
      url: "/?mode=android",
      width: 430,
      height: 860,
      minWidth: 320,
      minHeight: 640,
      decorations: false,
    });
  });

  it("builds Windows and Linux desktop packages across platform configs", () => {
    expect(config.bundle.targets).toEqual(
      expect.arrayContaining(["nsis"]),
    );
    expect(linuxConfig.bundle.targets).toEqual(
      expect.arrayContaining(["appimage", "deb"]),
    );
  });

  it("allows local asset protocol reads from Windows and Linux app data directories", () => {
    const scope = config.app.security.assetProtocol.scope;

    expect(scope).toEqual(
      expect.arrayContaining([
        "$APPDATA/Snapline/assets/**",
        "$APPLOCALDATA/Snapline/assets/**",
        "$DATA/Snapline/assets/**",
        "$HOME/.local/share/Snapline/assets/**",
      ]),
    );
  });

  it("uses only Tauri-supported asset scope base directory variables", () => {
    const schema = cliSchema.definitions.FsScope.description;
    const supportedVariables = Array.from(schema.matchAll(/`(\$[A-Z]+)`/g)).map(
      (match) => match[1],
    );

    for (const scopedPath of config.app.security.assetProtocol.scope) {
      const variable = scopedPath.match(/^(\$[A-Z]+)/)?.[1];

      expect(variable ? supportedVariables : []).toContain(variable);
    }
  });
  it("allows the custom chrome to start native window dragging", () => {
    expect(mainCapability.permissions).toContain("core:window:allow-start-dragging");
  });
});
