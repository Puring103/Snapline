---
name: snapline-packaging
description: Documents Snapline's required validation, desktop packaging, Android signing, artifact collection, and common project commands. Use when modifying Snapline code, building releases, packaging desktop or Android installers, cleaning build output, or checking the project's standard commands.
---

# Snapline Commands

## Required Before Commit

Before every commit, run the complete validation and test set unless the user explicitly says not to.

Run these from the repo root:
```powershell
cargo fmt
cargo check
cargo clippy
cargo test
```

Run these from `apps/client`:
```powershell
npm run build
npm run test
```
If tests are unavailable or fail for an environmental reason, report the exact blocker.

## Packaging Requires Explicit Instruction

Do not package before commit by default. Build installable packages only when the user explicitly asks for packaging, a release build, an installer, desktop package, Android package, APK, or signed APK.

## Desktop Package

When explicitly requested, build the desktop installer:
```powershell
Set-Location apps/client
npx tauri build
Set-Location ..\..
```

Copy the built desktop installer into the unified package folder:
```powershell
New-Item -ItemType Directory -Force -Path release-artifacts | Out-Null
Copy-Item -Force target/release/bundle/nsis/Snapline_0.1.0_x64-setup.exe release-artifacts/
```

## Android Package

Only build Android when the user explicitly asks for an Android/mobile package, APK, or signed APK.

Android signing must use the project-local signing files:
- `android-signing/snapline-release.jks`
- `android-signing/snapline-release.properties`
- `scripts/sign-android-release.ps1`

Build and sign:
```powershell
Set-Location apps/client
npx tauri android build
Set-Location ..\..
powershell -ExecutionPolicy Bypass -File scripts\sign-android-release.ps1
```

The signing script writes the signed APK to:
```text
release-artifacts/Snapline_0.1.0_universal-release-signed.apk
```

## Artifact Folder Rule

All final installable packages must be copied into `release-artifacts/`. Do not leave the user hunting through `target/`, `dist/`, or Android Gradle output directories.

Expected retained package names:
- `release-artifacts/Snapline_0.1.0_x64-setup.exe`
- `release-artifacts/Snapline_0.1.0_universal-release-signed.apk`

## Common Commands

Run the desktop app in development:
```powershell
Set-Location apps/client
npm run dev
```

Run Tauri commands:
```powershell
Set-Location apps/client
npm run tauri -- <command>
```

Run all Rust tests:
```powershell
cargo test
```

Run a Rust package test:
```powershell
cargo test -p snapline-domain
```

Clean temporary Rust and client build output only when requested:
```powershell
cargo clean
Remove-Item -Recurse -Force apps/client/dist, apps/client/src-tauri/target -ErrorAction SilentlyContinue
```
