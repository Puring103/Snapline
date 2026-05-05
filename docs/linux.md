# Snapline Linux Support

Snapline supports Linux as a Tauri desktop target alongside Windows. The Linux-specific Tauri configuration builds AppImage and deb packages and keeps the same compact window, close-to-hide behavior, background launch argument, global shortcut registration, autostart registration, local asset loading, and external URL opening path used by the desktop app.

## Package Targets

The desktop Tauri bundle targets are:

- Base config: `nsis` for Windows.
- Linux config: `appimage` for portable Linux distribution.
- Linux config: `deb` for Debian and Ubuntu-style installation.

## Build Command

From `apps/desktop-tauri`:

```bash
npm run build
npx tauri build
```

## Linux System Dependencies

Tauri Linux builds commonly require WebKitGTK, GTK, librsvg, AppIndicator, and build tooling. On Debian, Ubuntu, or Linux Mint systems, install the Linux prerequisites before running `npx tauri build`:

```bash
sudo apt install \
  libwebkit2gtk-4.1-dev \
  libgtk-3-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  libsoup-3.0-dev \
  xdg-utils
```

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
