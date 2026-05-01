# Snapline Linux Compatibility Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Linux desktop packaging and configuration coverage so Snapline's Linux experience matches the current Windows workflow as closely as Tauri and the desktop environment permit.

**Architecture:** This is a configuration-first compatibility pass. Tauri package targets and asset protocol scope carry the Linux behavior, while existing Rust runtime code continues to provide cross-platform data paths, external URL opening, global shortcut registration, autostart, and close-to-hide behavior.

**Tech Stack:** Tauri 2, Rust workspace, React/Vite/Vitest, JSON Tauri configuration, Markdown docs.

---

## File Structure

- `apps/desktop-tauri/src/tauriConfig.test.ts`: configuration regression tests for Linux package targets and asset protocol scope.
- `apps/desktop-tauri/src-tauri/tauri.conf.json`: Windows bundle target and shared asset protocol scope.
- `apps/desktop-tauri/src-tauri/tauri.linux.conf.json`: Linux bundle target override automatically merged by Tauri on Linux.
- `docs/linux.md`: Linux build, runtime dependency, packaging, and known desktop-environment behavior notes.

## Task 1: Lock Linux Tauri Configuration With Tests

**Files:**
- Modify: `apps/desktop-tauri/src/tauriConfig.test.ts`

- [ ] **Step 1: Write the failing tests**

Add tests to `apps/desktop-tauri/src/tauriConfig.test.ts`:

```ts
it("builds the Windows installer by default", () => {
  expect(config.bundle.targets).toEqual(["nsis"]);
});

it("builds AppImage and deb packages on Linux", () => {
  expect(linuxConfig.bundle.targets).toEqual(["appimage", "deb"]);
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
```

- [ ] **Step 2: Run tests to verify RED**

Run:

```bash
npm test -- tauriConfig.test.ts
```

from `apps/desktop-tauri`.

Expected: fail because `appimage`, `deb`, and Linux asset scopes are not yet configured.

## Task 2: Add Linux Tauri Bundle and Asset Scope

**Files:**
- Modify: `apps/desktop-tauri/src-tauri/tauri.conf.json`

- [ ] **Step 1: Update bundle targets**

Create `apps/desktop-tauri/src-tauri/tauri.linux.conf.json`:

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "bundle": {
    "targets": ["appimage", "deb"]
  }
}
```

- [ ] **Step 2: Update asset protocol scope**

Change:

```json
"scope": [
  "$APPDATA/Snapline/assets/**",
  "$APPLOCALDATA/Snapline/assets/**"
]
```

to:

```json
"scope": [
  "$APPDATA/Snapline/assets/**",
  "$APPLOCALDATA/Snapline/assets/**",
  "$DATA/Snapline/assets/**",
  "$HOME/.local/share/Snapline/assets/**"
]
```

- [ ] **Step 3: Run tests to verify GREEN**

Run:

```bash
npm test -- tauriConfig.test.ts
```

from `apps/desktop-tauri`.

Expected: pass.

## Task 3: Document Linux Build and Runtime Expectations

**Files:**
- Create: `docs/linux.md`

- [ ] **Step 1: Create Linux documentation**

Create `docs/linux.md` with these sections:

```md
# Snapline Linux Support

Snapline supports Linux as a Tauri desktop target alongside Windows. The Linux configuration builds AppImage and deb packages and keeps the same compact window, close-to-hide behavior, background launch argument, global shortcut registration, autostart registration, local asset loading, and external URL opening path used by the desktop app.

## Package Targets

The desktop Tauri bundle targets are:

- `nsis` for Windows.
- `appimage` for portable Linux distribution.
- `deb` for Debian and Ubuntu-style installation.

## Build Command

From `apps/desktop-tauri`:

```bash
npm run build
npx tauri build
```

## Linux System Dependencies

Tauri Linux builds commonly require WebKitGTK, GTK, librsvg, AppIndicator, and build tooling. On Debian or Ubuntu-style systems, install the Tauri Linux prerequisites for your distribution before running `npx tauri build`.

Runtime external URL opening uses `xdg-open`, so Linux systems should have `xdg-utils` installed.

## App Data and Local Assets

Snapline stores app data in the platform app data directory resolved by Rust's `directories::ProjectDirs`. On Linux this is typically under `$XDG_DATA_HOME/Snapline` or `$HOME/.local/share/Snapline`.

The Tauri asset protocol is scoped only to Snapline's local asset directories:

- `$APPDATA/Snapline/assets/**`
- `$APPLOCALDATA/Snapline/assets/**`
- `$DATA/Snapline/assets/**`
- `$HOME/.local/share/Snapline/assets/**`

## Desktop Environment Notes

Snapline registers the same global shortcut behavior on Linux as Windows. Some Linux Wayland compositors restrict global shortcuts, and support can depend on the desktop session and portal setup. Shortcut registration failures are logged and do not block app startup.

Autostart is registered through the Tauri autostart plugin with the `--background` argument. Desktop-environment autostart behavior can vary, but the Linux app uses the same background launch path as Windows.
```

## Task 4: Full Verification

**Files:**
- No code edits.

- [ ] **Step 1: Run frontend tests**

Run:

```bash
npm test
```

from `apps/desktop-tauri`.

Expected: all Vitest suites pass.

- [ ] **Step 2: Run Rust tests**

Run:

```bash
cargo test
```

from the repository root.

Expected: all Rust tests pass.

- [ ] **Step 3: Attempt Linux bundle build**

Run:

```bash
npx tauri build
```

from `apps/desktop-tauri`.

Expected: build succeeds if required Linux system packages are installed. If it fails due missing system libraries such as WebKitGTK/GTK/AppIndicator, record the exact missing dependency output and treat that as an environment setup issue, not an implementation pass.
