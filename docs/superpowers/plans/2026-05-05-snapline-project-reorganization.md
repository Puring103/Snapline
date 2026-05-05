# Snapline Project Reorganization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reorganize the Snapline repository into a clearer structure with a top-level project entry point, grouped frontend modules, and split Rust core modules while preserving existing behavior.

**Architecture:** The work proceeds in layers: repository entry points first, then frontend file relocation with stable exports, then Rust module decomposition with stable public facades. Each stage keeps runtime behavior intact and is followed by targeted verification so structural regressions are caught before the next layer moves.

**Tech Stack:** Rust workspace, Tauri 2, React 18, TypeScript, Vite, Vitest, rusqlite

---

## File Structure Map

- Create: `README.md`
- Create: `apps/desktop-tauri/src/components/`
- Create: `apps/desktop-tauri/src/features/assets/`
- Create: `apps/desktop-tauri/src/features/editor/`
- Create: `apps/desktop-tauri/src/features/sync/`
- Create: `apps/desktop-tauri/src/platform/`
- Create: `crates/app-core/src/app.rs`
- Create: `crates/app-core/src/assets.rs`
- Create: `crates/app-core/src/bootstrap.rs`
- Create: `crates/app-core/src/settings.rs`
- Create: `crates/app-core/src/sync.rs`
- Create: `crates/app-core/src/notes.rs`
- Create: `crates/storage/src/repository/mod.rs`
- Create: `crates/storage/src/repository/notes.rs`
- Create: `crates/storage/src/repository/sync_queue.rs`
- Create: `crates/storage/src/repository/settings.rs`
- Modify: `apps/desktop-tauri/src/App.tsx`
- Modify: `apps/desktop-tauri/src/main.tsx`
- Modify: `apps/desktop-tauri/src/*` imports affected by file moves
- Modify: `crates/app-core/src/lib.rs`
- Modify: `crates/storage/src/lib.rs`
- Delete: `crates/storage/src/repository.rs`

### Task 1: Repository Entry And Docs

**Files:**
- Create: `README.md`
- Modify: `docs/linux.md`
- Modify: `docs/self-hosting.md`
- Modify: `docs/superpowers/specs/2026-05-05-snapline-project-reorganization-design.md`

- [ ] **Step 1: Write the failing documentation acceptance checklist**

```md
- [ ] Root README explains what Snapline is
- [ ] Root README shows repository map
- [ ] Root README includes desktop dev commands
- [ ] Root README includes Rust test commands
- [ ] Root README links Linux and self-hosting docs
```

- [ ] **Step 2: Verify README does not exist yet**

Run: `Test-Path README.md`
Expected: `False`

- [ ] **Step 3: Add the root README with repository entry points**

```md
# Snapline

Snapline is a desktop-first note app built with Tauri, React, and Rust, with local storage and optional sync services.

## Repository Map
- `apps/desktop-tauri/`: desktop UI and Tauri integration
- `crates/app-core/`: application use cases
- `crates/storage/`: local persistence
- `crates/sync-client/`: client sync logic
- `crates/sync-server/`: sync server

## Development
```bash
cd apps/desktop-tauri
npm install
npm run dev
```

## Testing
```bash
cd apps/desktop-tauri
npm test
cargo test
```
```

- [ ] **Step 4: Link operational docs from the README**

```md
See:

- `docs/linux.md`
- `docs/self-hosting.md`
- `docs/superpowers/specs/` for design history
- `docs/superpowers/plans/` for implementation history
```

- [ ] **Step 5: Verify the README and docs render as the new repository entry**

Run: `Get-Content README.md`
Expected: Contains project summary, repository map, development commands, testing commands, and docs links.

- [ ] **Step 6: Commit**

```bash
git add README.md docs/linux.md docs/self-hosting.md docs/superpowers/specs/2026-05-05-snapline-project-reorganization-design.md
git commit -m "docs: add project entry points"
```

### Task 2: Frontend Module Reorganization

**Files:**
- Create: `apps/desktop-tauri/src/components/EditorPane.tsx`
- Create: `apps/desktop-tauri/src/components/MarkdownPreview.tsx`
- Create: `apps/desktop-tauri/src/components/SyncSettings.tsx`
- Create: `apps/desktop-tauri/src/features/assets/assetDisplay.ts`
- Create: `apps/desktop-tauri/src/features/assets/assetUrl.ts`
- Create: `apps/desktop-tauri/src/features/assets/imageUploadDisplay.ts`
- Create: `apps/desktop-tauri/src/features/editor/copyMarkdown.ts`
- Create: `apps/desktop-tauri/src/features/editor/editorExtensions.ts`
- Create: `apps/desktop-tauri/src/features/editor/editorMode.ts`
- Create: `apps/desktop-tauri/src/features/editor/editorSync.ts`
- Create: `apps/desktop-tauri/src/features/editor/markdown.ts`
- Create: `apps/desktop-tauri/src/features/editor/pasteImage.ts`
- Create: `apps/desktop-tauri/src/features/editor/pasteMarkdown.ts`
- Create: `apps/desktop-tauri/src/features/sync/session.ts`
- Create: `apps/desktop-tauri/src/features/sync/syncStatus.ts`
- Create: `apps/desktop-tauri/src/platform/api.ts`
- Create: `apps/desktop-tauri/src/platform/startupLog.ts`
- Create: `apps/desktop-tauri/src/platform/window.ts`
- Modify: `apps/desktop-tauri/src/App.tsx`
- Modify: `apps/desktop-tauri/src/main.tsx`
- Modify: affected test files to use new import paths

- [ ] **Step 1: Write a focused frontend regression test target list**

```txt
apps/desktop-tauri/src/pasteImage.test.ts
apps/desktop-tauri/src/tauriConfig.test.ts
apps/desktop-tauri/src/window.test.ts
apps/desktop-tauri/src/session.test.ts
apps/desktop-tauri/src/imageUploadDisplay.test.ts
```

- [ ] **Step 2: Run the focused frontend tests before moving files**

Run: `npm test -- --run src/pasteImage.test.ts src/tauriConfig.test.ts src/window.test.ts src/session.test.ts src/imageUploadDisplay.test.ts`
Expected: PASS

- [ ] **Step 3: Move frontend helpers into grouped directories while preserving exports**

```ts
// Example bridge file pattern during transition
export * from "./features/editor/pasteImage";
```

- [ ] **Step 4: Update component imports to match the new grouped structure**

```ts
import { api } from "./platform/api";
import { EditorPane } from "./components/EditorPane";
import { fileUrlFromMarkdownPath } from "./features/assets/assetUrl";
import { pastedImageSourceFromClipboard } from "./features/editor/pasteImage";
import { syncStatusLabel } from "./features/sync/syncStatus";
```

- [ ] **Step 5: Slim `App.tsx` by keeping it as composition root instead of helper owner**

```ts
export function App() {
  const route = useMemo(readAppRoute, []);
  useThemeSync();
  return route.mode === "list" ? <NotesListWindow /> : <NoteEditorWindow noteId={route.noteId} />;
}
```

- [ ] **Step 6: Run the frontend test suite after the move**

Run: `npm test`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add apps/desktop-tauri/src
git commit -m "refactor: reorganize desktop frontend modules"
```

### Task 3: Split App Core

**Files:**
- Create: `crates/app-core/src/app.rs`
- Create: `crates/app-core/src/assets.rs`
- Create: `crates/app-core/src/bootstrap.rs`
- Create: `crates/app-core/src/notes.rs`
- Create: `crates/app-core/src/settings.rs`
- Create: `crates/app-core/src/sync.rs`
- Modify: `crates/app-core/src/lib.rs`

- [ ] **Step 1: Run app-core tests before decomposition**

Run: `cargo test -p snapline-app-core`
Expected: PASS

- [ ] **Step 2: Extract public types and constructor wiring into `app.rs` and `bootstrap.rs`**

```rust
pub struct AppCore {
    pub(crate) repo: NoteRepository,
    pub(crate) paths: AppPaths,
}

impl AppCore {
    pub fn open(paths: AppPaths) -> Result<Self> { /* existing logic */ }
    pub fn with_repo(paths: AppPaths, repo: NoteRepository) -> Self { Self { repo, paths } }
}
```

- [ ] **Step 3: Extract note and asset operations into focused modules**

```rust
impl AppCore {
    pub fn create_note(&self) -> Result<Note> { /* existing logic */ }
    pub fn save_note(&self, id: &NoteId, title: &str, content_md: &str, pinned: bool) -> Result<Note> { /* existing logic */ }
    pub fn save_png_asset(&self, note_id: &NoteId, png_bytes: &[u8]) -> Result<AssetRef> { /* existing logic */ }
}
```

- [ ] **Step 4: Extract sync and settings methods into dedicated modules**

```rust
impl AppCore {
    pub fn get_open_shortcut(&self) -> Result<String> { /* existing logic */ }
    pub fn sync_account_state(&self) -> Result<SyncAccountState> { /* existing logic */ }
    pub fn import_snapshot(&self, notes: &[Note], cursor: i64) -> Result<()> { /* existing logic */ }
}
```

- [ ] **Step 5: Reduce `lib.rs` to module declarations, exports, and tests**

```rust
mod app;
mod assets;
mod bootstrap;
mod notes;
mod settings;
mod sync;

pub use app::AppCore;
pub use bootstrap::{BootstrapState, SyncAccountState};
```

- [ ] **Step 6: Run app-core tests after the split**

Run: `cargo test -p snapline-app-core`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add crates/app-core/src
git commit -m "refactor: split app core modules"
```

### Task 4: Split Storage Repository

**Files:**
- Create: `crates/storage/src/repository/mod.rs`
- Create: `crates/storage/src/repository/notes.rs`
- Create: `crates/storage/src/repository/settings.rs`
- Create: `crates/storage/src/repository/sync_queue.rs`
- Modify: `crates/storage/src/lib.rs`
- Delete: `crates/storage/src/repository.rs`

- [ ] **Step 1: Run storage tests before decomposition**

Run: `cargo test -p snapline-storage`
Expected: PASS

- [ ] **Step 2: Move connection setup and migration wiring into `repository/mod.rs`**

```rust
pub struct NoteRepository {
    conn: Connection,
}

impl NoteRepository {
    pub fn open(path: &Path) -> Result<Self> { /* existing logic */ }
    pub fn open_in_memory() -> Result<Self> { /* existing logic */ }
    fn migrate(&self) -> Result<()> { /* existing logic */ }
}
```

- [ ] **Step 3: Extract note persistence operations into `repository/notes.rs`**

```rust
impl NoteRepository {
    pub fn create_note(&self, now: DateTime<Utc>, owner_account_id: Option<&str>) -> Result<Note> { /* existing logic */ }
    pub fn save_note(&self, id: &NoteId, title: &str, content_md: &str, pinned: bool, now: DateTime<Utc>, owner_account_id: Option<&str>) -> Result<Note> { /* existing logic */ }
    pub fn get_note_for_owner(&self, id: &NoteId, owner_account_id: Option<&str>) -> Result<Note> { /* existing logic */ }
}
```

- [ ] **Step 4: Extract settings and sync queue operations into dedicated modules**

```rust
impl NoteRepository {
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> { /* existing logic */ }
    pub fn set_setting(&self, key: &str, value: Option<&str>) -> Result<()> { /* existing logic */ }
    pub fn enqueue_change(&self, account_id: Option<&str>, note_id: &NoteId, op_type: SyncOpType, base_version: i64, payload: &SyncPayload, now: DateTime<Utc>) -> Result<()> { /* existing logic */ }
}
```

- [ ] **Step 5: Keep `crates/storage/src/lib.rs` as the stable export layer**

```rust
pub mod repository;
pub mod sync;

pub use repository::NoteRepository;
pub use sync::{ChangeQueueItem, SyncState};
```

- [ ] **Step 6: Run storage tests and dependent crate tests after the split**

Run: `cargo test -p snapline-storage -p snapline-app-core`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add crates/storage/src crates/app-core/src
git commit -m "refactor: split storage repository modules"
```

### Task 5: Full Verification And Cleanup

**Files:**
- Modify: any broken imports, doc links, or test paths discovered during verification

- [ ] **Step 1: Run the frontend suite**

Run: `npm test`
Expected: PASS

- [ ] **Step 2: Run the workspace Rust tests**

Run: `cargo test`
Expected: PASS

- [ ] **Step 3: Run the desktop production build**

Run: `npm run build`
Expected: PASS

- [ ] **Step 4: Review git diff for accidental behavior changes**

Run: `git diff --stat`
Expected: Shows structural reorganization, docs updates, and limited behavior-preserving fixes only.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "chore: finish project reorganization"
```

## Self-Review

- Spec coverage: The plan covers repository entry points, frontend structure, app-core split, storage split, and verification.
- Placeholder scan: No `TODO` or `TBD` placeholders remain; each task names exact files and commands.
- Type consistency: `AppCore`, `BootstrapState`, `SyncAccountState`, and `NoteRepository` remain the stable public seams across tasks.
