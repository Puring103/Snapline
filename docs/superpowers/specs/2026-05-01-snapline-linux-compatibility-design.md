# Snapline Linux Compatibility Design

Date: 2026-05-01

## Goal

Snapline should provide a Linux desktop experience that matches the current Windows experience wherever the underlying desktop environment permits it. Linux users should be able to install or run a packaged desktop app, open the same compact note window, use local image assets, launch in the background through autostart, open external links, and summon the app with the configured global shortcut.

## Current State

The runtime is already partly cross-platform:

- The desktop app is a Tauri 2 application with shared Rust core crates.
- App data paths are resolved with `directories::ProjectDirs`.
- External URLs use `explorer.exe` on Windows, `open` on macOS, and `xdg-open` on Linux and other Unix desktops.
- The main window already supports compact sizing, custom chrome dragging, close-to-hide behavior, background launch, global shortcut registration, and autostart plugin registration.

The Linux gaps are configuration and verification:

- Tauri bundle targets only include the Windows `nsis` installer.
- The custom asset protocol scope only lists Windows-oriented app data paths.
- Tests do not lock Linux bundle and asset-scope expectations.
- Documentation does not state the Linux desktop dependencies or build command.

## Scope

This change will implement full first-pass Linux compatibility for the existing desktop feature set:

- Keep Windows support intact.
- Add Linux package targets: AppImage and deb.
- Ensure local note assets can be loaded from Linux XDG app data locations.
- Keep the compact borderless window behavior consistent across Windows and Linux.
- Keep global shortcut, background launch, close-to-hide, and autostart behavior enabled on Linux.
- Document Linux runtime and build expectations.
- Add tests that protect the Linux compatibility configuration.

Out of scope for this pass:

- Flatpak, Snap, rpm, or distro repository publishing.
- A tray icon or platform diagnostics UI.
- Custom Wayland portal integration beyond what Tauri and its plugins provide.
- CI matrix setup, unless the existing project already has CI files to extend.

## Design

### Tauri Bundle Targets

Update `apps/desktop-tauri/src-tauri/tauri.conf.json` so `bundle.targets` includes:

- `nsis` for Windows.
- `appimage` for broad Linux binary distribution.
- `deb` for Debian and Ubuntu-style installation.

The existing Windows `nsis` installer settings and app icons remain unchanged.

### Asset Protocol Scope

Keep the existing Windows asset scopes and add Linux-compatible scopes that cover the app data directory Snapline uses on Linux. The scope must stay constrained to `Snapline/assets/**` so the custom asset protocol cannot read arbitrary user files.

The expected Linux path shape is the XDG data directory for the Snapline app, typically:

- `$XDG_DATA_HOME/Snapline/assets/**`, or
- `$HOME/.local/share/Snapline/assets/**` when `XDG_DATA_HOME` is not set.

Where Tauri supports base directory variables, prefer them over hard-coded absolute paths. If a variable does not cover the required Linux data location, add the narrowest safe fallback pattern.

### Runtime Behavior

No new platform abstraction is required for the first pass. Existing runtime behavior should continue:

- `open_url_with_system` uses `xdg-open` on Linux.
- `AppPaths::resolve()` uses `ProjectDirs`, producing platform-native app data locations.
- `tauri_plugin_autostart` remains registered with `--background`.
- `tauri_plugin_global_shortcut` remains registered and failures are logged rather than blocking app startup.
- The main window remains borderless, compact, resizable, and close-to-hide.

Linux desktop environments vary, especially around global shortcuts under Wayland. The app should not crash if shortcut registration fails. A future diagnostics UI can expose degraded shortcut support if real-world testing shows it is needed.

### Documentation

Add Linux notes to project documentation covering:

- Linux package targets produced by Tauri.
- Common runtime/build dependencies expected by Tauri desktop apps, especially WebKitGTK, GTK, librsvg, AppIndicator, and `xdg-open`/`xdg-utils`.
- The build command from the desktop app package.
- The known limitation that global shortcut support can depend on the Linux session and compositor.

### Tests

Extend existing configuration tests in `apps/desktop-tauri/src/tauriConfig.test.ts`:

- Assert `bundle.targets` includes `nsis`, `appimage`, and `deb`.
- Assert asset protocol scope contains Windows app data scopes and Linux XDG/local share scopes.
- Keep the existing compact borderless window and drag permission tests.

Run:

- `npm test` from `apps/desktop-tauri`.
- `cargo test` from the workspace root.

If full Linux Tauri bundling cannot run in the local environment because system GUI packages are missing, report that separately from unit-test status.

## Success Criteria

- Windows installer support remains configured.
- Linux AppImage and deb packaging are configured.
- Local pasted/synced assets can be served from Linux app data locations.
- Existing compact-window workflow remains unchanged.
- Tests cover the Linux-specific configuration.
- Documentation tells a Linux developer or packager how to build and what system dependencies are expected.
