# Snapline Compact Notes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn Snapline into a compact sticky-note style app with editable titles, pinned notes, configurable global open shortcut, draft notes that are only persisted after editing, and inline markdown/image editing without a separate preview screen.

**Architecture:** Keep persistence and note ordering in Rust, keep the window/panel behavior in Tauri, and keep the editor/rendering flow in React + Tiptap. The backend owns note metadata, pinned ordering, shortcut configuration, and image asset storage; the frontend owns the compact single-note summary screen, hidden list panel, and markdown editing UX.

**Tech Stack:** Rust, SQLite, Tauri 2, React, TypeScript, Tiptap, CSS.

---

### Task 1: Remove the frontend path permission dependency

**Files:**
- Modify: `apps/desktop-tauri/src/App.tsx`
- Modify: `apps/desktop-tauri/src/EditorPane.tsx`
- Modify: `apps/desktop-tauri/src/api.ts`
- Modify: `apps/desktop-tauri/src/types.ts`
- Modify: `crates/app-core/src/lib.rs`
- Modify: `crates/domain/src/asset.rs`
- Modify: `crates/platform/src/lib.rs`
- Modify: `apps/desktop-tauri/src-tauri/src/main.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn bootstrap_does_not_require_tauri_path_resolution() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_data_dir(dir.path());
    let core = AppCore::with_repo(paths, NoteRepository::open_in_memory().unwrap());

    let state = core.bootstrap().unwrap();
    assert_eq!(state.current.title, "Untitled");
}
```

- [ ] **Step 2: Run the test and confirm the frontend path call is the only blocker**

Run: `cargo test -p snapline-app-core bootstrap_does_not_require_tauri_path_resolution -v`
Expected: pass after removing `@tauri-apps/api/path` use from the frontend and replacing image hydration with backend-provided absolute paths.

- [ ] **Step 3: Implement backend-provided asset resolution**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetRef {
    pub markdown_path: String,
    pub filesystem_path: String,
}
```

```ts
// apps/desktop-tauri/src/api.ts
savePngAsset: (noteId: string, bytes: number[]) =>
  invoke<AssetRef>("save_png_asset", { note_id: noteId, bytes }),
```

- [ ] **Step 4: Verify the app boots without `path.resolve_directory`**

Run: `npm run dev` and open the app in the in-app browser.
Expected: no permission error banner, editor loads, image paste still works.

---

### Task 2: Add pinned notes, draft notes, and editable titles in storage

**Files:**
- Modify: `crates/domain/src/note.rs`
- Modify: `crates/storage/src/repository.rs`
- Modify: `crates/app-core/src/lib.rs`

- [ ] **Step 1: Add a failing repository test for pinned ordering**

```rust
#[test]
fn pinned_notes_sort_before_recent_notes() {
    let repo = NoteRepository::open_in_memory().unwrap();
    let now = Utc.with_ymd_and_hms(2026, 4, 29, 3, 0, 0).unwrap();
    let first = repo.create_note(now).unwrap();
    let second = repo.create_note(now).unwrap();

    repo.set_pinned(&second.id, true, now).unwrap();
    let notes = repo.list_recent(10).unwrap();

    assert_eq!(notes[0].id, second.id);
    assert_eq!(notes[1].id, first.id);
}
```

- [ ] **Step 2: Extend the note schema and CRUD methods**

```rust
pub struct Note {
    pub id: NoteId,
    pub title: String,
    pub content_md: String,
    pub pinned: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}
```

```rust
pub fn update_note_title(&self, id: &NoteId, title: &str, now: DateTime<Utc>) -> Result<Note>;
pub fn set_pinned(&self, id: &NoteId, pinned: bool, now: DateTime<Utc>) -> Result<Note>;
```

- [ ] **Step 3: Add draft-note bootstrap behavior**

```rust
pub fn bootstrap(&self) -> Result<BootstrapState> {
    let notes = self.repo.list_recent(50)?;
    let current = notes
        .first()
        .map(|item| self.repo.get_note(&item.id))
        .transpose()?
        .unwrap_or_else(|| self.repo.create_note(Utc::now())?);
    Ok(BootstrapState { notes, current })
}
```

- [ ] **Step 4: Verify save semantics only persist after editing**

Run: `cargo test -p snapline-storage -p snapline-app-core -v`
Expected: notes created on bootstrap are not duplicated when the editor stays unchanged; empty-content notes keep title `"Untitled"` until edited.

---

### Task 3: Rebuild the desktop UI into a compact sticky-note layout

**Files:**
- Modify: `apps/desktop-tauri/src/App.tsx`
- Modify: `apps/desktop-tauri/src/EditorPane.tsx`
- Modify: `apps/desktop-tauri/src/styles.css`
- Modify: `apps/desktop-tauri/src/types.ts`

- [ ] **Step 1: Write a failing UI smoke test for the compact header and hidden list**

```ts
it("starts on a summary view with the note list hidden", async () => {
  render(<App />);
  expect(screen.getByRole("button", { name: /list/i })).toBeInTheDocument();
  expect(screen.queryByLabelText("Notes")).toBeNull();
});
```

- [ ] **Step 2: Implement the summary header, editable title, pin toggle, and list drawer**

```tsx
<header className="topBar">
  <input value={title} onChange={...} className="titleInput" />
  <button onClick={togglePinned} aria-pressed={pinned}>Pin</button>
  <button onClick={() => setListOpen((value) => !value)}>List</button>
</header>
```

```tsx
{listOpen ? (
  <aside className="noteDrawer" aria-label="Notes">
    {notes.map((note) => (
      <button key={note.id} className={note.pinned ? "noteRow pinned" : "noteRow"}>
        <span>{note.title}</span>
        <button onClick={() => void deleteNote(note.id)}>Delete</button>
      </button>
    ))}
  </aside>
) : null}
```

- [ ] **Step 3: Make the editor start as a fresh draft and skip save when unchanged**

```tsx
const [dirty, setDirty] = useState(false);
const initialMarkdown = useMemo(() => note.content_md, [note.id]);
```

```ts
if (!dirty) return;
```

- [ ] **Step 4: Verify the layout at app scale**

Run: `npm run build`
Expected: build passes, the window fits the smaller sticky-note layout, and the editor remains inline without a separate preview route.

---

### Task 4: Add configurable global open shortcut and topmost window behavior

**Files:**
- Modify: `apps/desktop-tauri/src-tauri/src/main.rs`
- Modify: `apps/desktop-tauri/src-tauri/Cargo.toml`
- Modify: `apps/desktop-tauri/src-tauri/tauri.conf.json`
- Modify: `apps/desktop-tauri/src/App.tsx`
- Modify: `apps/desktop-tauri/src/api.ts`
- Modify: `apps/desktop-tauri/src/types.ts`

- [ ] **Step 1: Add a failing integration test for shortcut persistence**

```rust
#[test]
fn stores_and_loads_shortcut_config() {
    let dir = tempfile::tempdir().unwrap();
    let core = AppCore::open(AppPaths::from_data_dir(dir.path())).unwrap();
    core.set_open_shortcut("Ctrl+Alt+S").unwrap();
    assert_eq!(core.get_open_shortcut().unwrap().as_deref(), Some("Ctrl+Alt+S"));
}
```

- [ ] **Step 2: Store the shortcut and register/unregister it through Tauri**

```rust
pub fn set_open_shortcut(&self, shortcut: Option<&str>) -> Result<()>;
pub fn get_open_shortcut(&self) -> Result<Option<String>>;
```

- [ ] **Step 3: Add a compact settings row in the UI for changing the shortcut**

```tsx
<input value={shortcut} onChange={...} placeholder="Ctrl+Alt+Space" />
<button onClick={saveShortcut}>Save</button>
```

- [ ] **Step 4: Verify the window can be hidden and reopened by the configured shortcut**

Run: `cargo test --workspace && npm test && npm run build`
Expected: shortcut config persists, and the app can be shown from the configured global shortcut.

---

### Task 5: Full verification and polish

**Files:**
- Review: all touched files

- [ ] **Step 1: Run the full Rust and frontend test suite**

Run: `cargo test --workspace`
Run: `cargo check -p snapline-desktop`
Run: `npm test`
Run: `npm run build`

- [ ] **Step 2: Launch the desktop app and verify the runtime behavior**

Run: `npm run dev`
Open: `http://localhost:1420/`
Expected: compact summary screen, editable title, list drawer, delete/new actions, pinned ordering, image paste, and no path permission error.

- [ ] **Step 3: Commit the working tree**

```bash
git add .
git commit -m "feat: compact sticky-note desktop workflow"
```
