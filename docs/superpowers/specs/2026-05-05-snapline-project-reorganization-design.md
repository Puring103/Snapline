# Snapline Project Reorganization Design

**Date:** 2026-05-05

## Goal

Reorganize the Snapline repository into a clearer, more maintainable structure without intentionally changing user-facing behavior for note editing, sync, asset upload, or desktop app workflows. This effort should leave the project easier to understand, easier to navigate, and safer to extend.

## Scope

This reorganization covers four areas:

1. Rebuild the repository entry points so the project has a clear top-level introduction and navigation path.
2. Restructure the desktop frontend source tree into grouped modules by responsibility instead of a flat file layout.
3. Split oversized Rust application and storage modules into smaller focused modules with stable public entry points.
4. Normalize project documentation and engineering entry points so contributors can build, test, and reason about the codebase with less tribal knowledge.

This work does not aim to redesign product behavior, change sync semantics, alter note storage formats, or introduce new end-user features beyond bug fixes required to preserve existing behavior during restructuring.

## Non-Goals

- Redesigning the editor UX or visual system
- Rewriting the sync architecture
- Changing database schema for organizational reasons alone
- Replacing the Rust workspace layout
- Renaming crates unless required for correctness
- Large-scale logic rewrites mixed into structural cleanup

## Current Problems

### Repository entry is unclear

The repository has no top-level `README.md`, so a new contributor has no single place to learn what Snapline is, how the workspace is organized, how to run the desktop app, how to test it, or how to work with the sync server and Linux support.

### Frontend source is overly flat

`apps/desktop-tauri/src` currently stores components, feature helpers, platform adapters, and tests side by side. This makes ownership fuzzy and increases the cost of tracing related behavior such as editor actions, asset handling, or sync UI behavior.

### Core Rust files are too large

`crates/app-core/src/lib.rs` and `crates/storage/src/repository.rs` each carry multiple responsibilities. This makes the application core and persistence layer harder to scan, reason about, and test in isolation.

### Documentation hierarchy is weak

There are useful documents in `docs/`, including Linux and self-hosting notes plus internal design and planning artifacts, but there is no clear distinction between user-facing documentation and internal process records.

## Design Principles

### Preserve behavior while changing structure

The primary axis of change is organization. Functionality should remain stable unless a defect is uncovered during the move. Structural edits should be staged so regressions are attributable and testable.

### Make the reading path obvious

A contributor should be able to answer three questions quickly:

- What is this project?
- Where does a given responsibility live?
- What command should I run next?

### Favor stable public seams

Module internals may move, but external entry points should remain intentionally small and predictable. This is especially important for `app-core`, `storage`, and frontend platform adapters.

### Group by responsibility

Files that change together should live together. The structure should reflect product capabilities and system responsibilities more than technical accident.

## Target Repository Structure

The workspace root remains the same, but the navigation model becomes clearer:

- `README.md`
  The primary project introduction and contributor starting point.
- `apps/desktop-tauri/`
  Desktop application frontend and Tauri integration.
- `crates/domain/`
  Shared domain model and sync payload definitions.
- `crates/app-core/`
  Application use cases and business orchestration.
- `crates/storage/`
  Local persistence, repository behavior, and sync queue state.
- `crates/sync-client/`
  Client-side sync protocol and processing.
- `crates/sync-server/`
  Server-side API, auth, storage integration, and sync behavior.
- `docs/`
  Human-oriented documentation with a clear distinction between operational docs and internal design history.

## Documentation Design

### Top-level README

Add a root `README.md` that includes:

- Project summary
- High-level architecture
- Repository map
- Desktop app development commands
- Rust workspace test commands
- Sync server local development entry points
- Links to Linux support and self-hosting documentation

The README becomes the only required first stop for new contributors.

### Docs hierarchy

Keep existing documents, but give them clearer roles:

- `docs/linux.md`
  Platform-specific desktop support notes
- `docs/self-hosting.md`
  Operational sync server guidance
- `docs/superpowers/specs/`
  Historical design specs
- `docs/superpowers/plans/`
  Historical implementation plans

The README should point to operational docs directly and mention that design and plan documents are internal project history rather than onboarding material.

## Frontend Structure Design

The desktop frontend should move from a flat `src` layout to grouped modules with clear ownership.

### Proposed layout

- `apps/desktop-tauri/src/components/`
  Reusable UI components such as editor pane, markdown preview, and sync settings UI.
- `apps/desktop-tauri/src/features/editor/`
  Editor behavior including modes, extensions, paste handling, copy logic, and editor sync helpers.
- `apps/desktop-tauri/src/features/assets/`
  Asset URL resolution, asset display, upload display formatting, and image handling helpers.
- `apps/desktop-tauri/src/features/sync/`
  Sync session, sync status, and sync-related UI helpers.
- `apps/desktop-tauri/src/platform/`
  Tauri API bindings, window integration, startup logging, and runtime configuration adapters.
- `apps/desktop-tauri/src/`
  Minimal app entry files such as `main.tsx`, `App.tsx`, shared `types.ts`, and any intentionally root-level composition files.

### Frontend migration rules

- Prefer moving files before renaming exported functions or types.
- Keep tests close to the modules they verify when practical.
- Avoid mixing broad behavior rewrites with file relocation.
- Allow root-level `App.tsx` to remain the app composition entry point, but reduce how many unrelated helpers it directly owns.

## Rust Application Core Design

`crates/app-core` should shift from a single oversized module into focused modules with `lib.rs` acting primarily as an export surface.

### Proposed layout

- `crates/app-core/src/lib.rs`
  Public exports and module declarations only.
- `crates/app-core/src/app.rs`
  `AppCore` type definition and shared constructor wiring.
- `crates/app-core/src/bootstrap.rs`
  Startup and bootstrap state assembly.
- `crates/app-core/src/notes.rs`
  Note lifecycle operations such as create, load, save, title update, pinning, and delete.
- `crates/app-core/src/assets.rs`
  Asset save, local path resolution, URL resolution, remote asset storage, and asset metadata helpers.
- `crates/app-core/src/sync.rs`
  Sync login state, queue operations, snapshot import, conflict handling, and sync cursor updates.
- `crates/app-core/src/settings.rs`
  Local settings such as global shortcut persistence.

### App-core boundary rules

- `AppCore` remains the main public application facade.
- Internal helper methods may move into feature-focused impl blocks or helper functions.
- Public behavior and serialized types should stay stable unless a bug fix requires change.

## Rust Storage Design

`crates/storage` should separate repository responsibilities by data concern instead of concentrating all logic in one large file.

### Proposed layout

- `crates/storage/src/lib.rs`
  Public exports only.
- `crates/storage/src/repository/mod.rs`
  Repository type, connection setup, and cross-module glue.
- `crates/storage/src/repository/notes.rs`
  Note persistence and retrieval.
- `crates/storage/src/repository/assets.rs`
  Asset-related persistence helpers when asset-specific repository logic is extracted from the current repository implementation.
- `crates/storage/src/repository/sync_queue.rs`
  Change queue and sync state operations.
- `crates/storage/src/repository/settings.rs`
  Key-value settings storage.
- `crates/storage/src/repository/bootstrap.rs`
  Schema setup, migrations, and shared initialization helpers if separation improves clarity.

### Storage boundary rules

- Preserve the current repository API unless a change is required to support clearer internal organization.
- Do not introduce schema changes solely to match the new file structure.
- Prefer extracting cohesive blocks of SQL and repository logic intact before refining internals.

## Execution Strategy

The reorganization should be performed in controlled stages:

1. Add the README and documentation navigation updates.
2. Reorganize frontend files into grouped directories while keeping behavior stable.
3. Split `app-core` into focused modules with a stable public facade.
4. Split `storage` into focused repository modules with a stable public facade.
5. Re-run and repair imports, tests, and any broken integration points.
6. Run verification across frontend and Rust codepaths.

This ordering ensures the project becomes easier to navigate early while containing risk during deeper code movement.

## Testing Strategy

Structural changes must be guarded by verification:

- Run frontend unit tests in `apps/desktop-tauri`.
- Run Rust tests for the workspace or at minimum for touched crates.
- Run the existing formatting and build checks used by the repository.
- Add targeted regression tests before risky refactors if current coverage is insufficient around moved logic.

Because this is a structural reorganization, passing tests are a required completion condition rather than a nice-to-have.

## Risks

### Import and module breakage

Moving frontend files and Rust modules can easily break imports or module visibility. The mitigation is to move in small steps and preserve export surfaces.

### Behavioral drift during structural edits

When restructuring large files, there is a temptation to "clean up" logic simultaneously. This should be resisted unless a change is required for correctness or to keep tests passing.

### Uneven test coverage

Not every structural seam is equally protected by tests. If a risky area lacks coverage, add a focused test before or during the move.

## Success Criteria

The reorganization is successful when:

- The repository has a clear top-level `README.md`.
- The main frontend responsibilities are grouped into understandable directories.
- `crates/app-core/src/lib.rs` is reduced to a focused export layer.
- `crates/storage/src/repository.rs` is replaced or reduced in favor of smaller repository modules.
- Existing supported behavior continues to work.
- Relevant tests pass after the restructure.

## Open Decisions Resolved

The following decisions are explicitly set for this effort:

- This is an aggressive structural reorganization, not a light cleanup.
- Public behavior should remain stable unless a bug fix is necessary.
- Frontend and Rust core organization are both in scope in the same effort.
- Historical design and plan docs remain in place but are demoted behind clearer operational entry points.
