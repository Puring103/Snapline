# Snapline M1 Local MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first runnable Snapline desktop MVP with local SQLite notes, Tauri commands, a React + Tiptap rendered Markdown editor, autosave, soft delete, and pasted image assets.

**Architecture:** Keep durable behavior in Rust crates (`domain`, `storage`, `platform`, `app-core`) and expose it through Tauri commands. Keep the web frontend thin: it renders the list/editor, serializes editor state to Markdown, debounces saves, and sends pasted image bytes to Rust for local storage.

**Tech Stack:** Rust workspace, Tauri 2, React, TypeScript, Vite, Tiptap, SQLite via `rusqlite`, `uuid`, `chrono`, `serde`, `tempfile`, Vitest.

---

## Scope

This plan implements only M1:

- Tauri desktop shell.
- React + Tiptap rendered Markdown editor.
- Local SQLite persistence.
- New, edit, list, and soft-delete notes.
- Debounced autosave.
- Paste image into editor, save image under note assets, persist Markdown image reference.
- Startup and save timing instrumentation.

It does not implement sync, FTS search, trash restore UI, account login, server APIs, or packaging/release.

## File Structure

- `Cargo.toml`: Rust workspace root.
- `crates/domain`: Pure note and asset types. No SQLite, Tauri, or async runtime.
- `crates/platform`: App data directory and asset path helpers.
- `crates/storage`: SQLite schema and note repository.
- `crates/app-core`: Application use cases that coordinate storage and platform paths.
- `apps/desktop-tauri/package.json`: Frontend scripts and dependencies.
- `apps/desktop-tauri/src-tauri`: Tauri app, command handlers, and state wiring.
- `apps/desktop-tauri/src`: React UI, Tiptap editor, Markdown conversion, autosave, pasted image handling.

## Task 1: Rust Workspace And Domain Types

**Files:**
- Create: `Cargo.toml`
- Create: `crates/domain/Cargo.toml`
- Create: `crates/domain/src/lib.rs`
- Create: `crates/domain/src/note.rs`
- Create: `crates/domain/src/asset.rs`
- Test: `crates/domain/src/note.rs`

- [ ] **Step 1: Create the Rust workspace manifests**

Create `Cargo.toml`:

```toml
[workspace]
members = [
  "crates/domain",
  "crates/platform",
  "crates/storage",
  "crates/app-core",
  "apps/desktop-tauri/src-tauri"
]
resolver = "2"

[workspace.package]
edition = "2021"
license = "UNLICENSED"
version = "0.1.0"

[workspace.dependencies]
anyhow = "1"
chrono = { version = "0.4", features = ["serde"] }
directories = "5"
rusqlite = { version = "0.31", features = ["bundled"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tempfile = "3"
thiserror = "1"
uuid = { version = "1", features = ["v4", "serde"] }
```

Create `crates/domain/Cargo.toml`:

```toml
[package]
name = "snapline-domain"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
chrono.workspace = true
serde.workspace = true
uuid.workspace = true
```

- [ ] **Step 2: Define note and asset domain code**

Create `crates/domain/src/lib.rs`:

```rust
pub mod asset;
pub mod note;

pub use asset::{AssetId, AssetRef};
pub use note::{derive_title, Note, NoteId, NoteSummary};
```

Create `crates/domain/src/note.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NoteId(pub Uuid);

impl NoteId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl std::fmt::Display for NoteId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub id: NoteId,
    pub title: String,
    pub content_md: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteSummary {
    pub id: NoteId,
    pub title: String,
    pub updated_at: DateTime<Utc>,
}

pub fn derive_title(content_md: &str) -> String {
    content_md
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.trim_start_matches('#').trim().to_string())
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| "Untitled".to_string())
}

#[cfg(test)]
mod tests {
    use super::derive_title;

    #[test]
    fn derives_title_from_first_non_empty_heading() {
        assert_eq!(derive_title("\n# Daily note\nbody"), "Daily note");
    }

    #[test]
    fn falls_back_for_empty_content() {
        assert_eq!(derive_title(" \n\t"), "Untitled");
    }
}
```

Create `crates/domain/src/asset.rs`:

```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AssetId(pub Uuid);

impl AssetId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl std::fmt::Display for AssetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetRef {
    pub markdown_path: String,
}
```

- [ ] **Step 3: Run the failing or passing domain tests**

Run:

```powershell
cargo test -p snapline-domain
```

Expected: `2 passed`.

- [ ] **Step 4: Commit task 1**

Run:

```powershell
git add Cargo.toml crates/domain
git commit -m "feat: add domain model"
```

## Task 2: SQLite Storage

**Files:**
- Create: `crates/storage/Cargo.toml`
- Create: `crates/storage/src/lib.rs`
- Create: `crates/storage/src/repository.rs`
- Test: `crates/storage/src/repository.rs`

- [ ] **Step 1: Add storage manifest**

Create `crates/storage/Cargo.toml`:

```toml
[package]
name = "snapline-storage"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
anyhow.workspace = true
chrono.workspace = true
rusqlite.workspace = true
snapline-domain = { path = "../domain" }
uuid.workspace = true

[dev-dependencies]
tempfile.workspace = true
```

- [ ] **Step 2: Write repository tests first**

Create `crates/storage/src/lib.rs`:

```rust
pub mod repository;

pub use repository::NoteRepository;
```

Create `crates/storage/src/repository.rs` with tests and a skeleton:

```rust
use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use snapline_domain::{derive_title, Note, NoteId, NoteSummary};
use std::path::Path;
use uuid::Uuid;

pub struct NoteRepository {
    conn: Connection,
}

impl NoteRepository {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        let repo = Self { conn };
        repo.migrate()?;
        Ok(repo)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let repo = Self { conn };
        repo.migrate()?;
        Ok(repo)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS notes (
              id TEXT PRIMARY KEY,
              title TEXT NOT NULL DEFAULT '',
              content_md TEXT NOT NULL DEFAULT '',
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              deleted_at TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_notes_deleted_updated
            ON notes (deleted_at, updated_at DESC);
            ",
        )?;
        Ok(())
    }

    pub fn create_note(&self, now: DateTime<Utc>) -> Result<Note> {
        let note = Note {
            id: NoteId::new(),
            title: "Untitled".to_string(),
            content_md: String::new(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
        };
        self.conn.execute(
            "INSERT INTO notes (id, title, content_md, created_at, updated_at, deleted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
            params![
                note.id.to_string(),
                note.title,
                note.content_md,
                note.created_at.to_rfc3339(),
                note.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(note)
    }

    pub fn update_note_content(
        &self,
        id: &NoteId,
        content_md: &str,
        now: DateTime<Utc>,
    ) -> Result<Note> {
        let title = derive_title(content_md);
        self.conn.execute(
            "UPDATE notes SET title = ?1, content_md = ?2, updated_at = ?3 WHERE id = ?4 AND deleted_at IS NULL",
            params![title, content_md, now.to_rfc3339(), id.to_string()],
        )?;
        self.get_note(id)
    }

    pub fn get_note(&self, id: &NoteId) -> Result<Note> {
        self.conn.query_row(
            "SELECT id, title, content_md, created_at, updated_at, deleted_at FROM notes WHERE id = ?1",
            params![id.to_string()],
            row_to_note,
        ).map_err(Into::into)
    }

    pub fn list_recent(&self, limit: usize) -> Result<Vec<NoteSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, updated_at FROM notes
             WHERE deleted_at IS NULL
             ORDER BY updated_at DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(NoteSummary {
                id: NoteId(parse_uuid(row.get::<_, String>(0)?)?),
                title: row.get(1)?,
                updated_at: parse_time(row.get::<_, String>(2)?)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    pub fn soft_delete(&self, id: &NoteId, now: DateTime<Utc>) -> Result<()> {
        self.conn.execute(
            "UPDATE notes SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2",
            params![now.to_rfc3339(), id.to_string()],
        )?;
        Ok(())
    }
}

fn row_to_note(row: &rusqlite::Row<'_>) -> rusqlite::Result<Note> {
    let deleted: Option<String> = row.get(5)?;
    Ok(Note {
        id: NoteId(parse_uuid(row.get::<_, String>(0)?)?),
        title: row.get(1)?,
        content_md: row.get(2)?,
        created_at: parse_time(row.get::<_, String>(3)?)?,
        updated_at: parse_time(row.get::<_, String>(4)?)?,
        deleted_at: deleted.map(parse_time).transpose()?,
    })
}

fn parse_uuid(value: String) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(&value).map_err(|err| rusqlite::Error::ToSqlConversionFailure(Box::new(err)))
}

fn parse_time(value: String) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&value)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|err| rusqlite::Error::ToSqlConversionFailure(Box::new(err)))
}

#[cfg(test)]
mod tests {
    use super::NoteRepository;
    use chrono::{TimeZone, Utc};

    #[test]
    fn creates_updates_lists_and_soft_deletes_note() {
        let repo = NoteRepository::open_in_memory().unwrap();
        let t1 = Utc.with_ymd_and_hms(2026, 4, 29, 1, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2026, 4, 29, 1, 1, 0).unwrap();
        let t3 = Utc.with_ymd_and_hms(2026, 4, 29, 1, 2, 0).unwrap();

        let note = repo.create_note(t1).unwrap();
        let updated = repo.update_note_content(&note.id, "# Hello\nBody", t2).unwrap();

        assert_eq!(updated.title, "Hello");
        assert_eq!(repo.list_recent(10).unwrap().len(), 1);

        repo.soft_delete(&note.id, t3).unwrap();
        assert!(repo.list_recent(10).unwrap().is_empty());
        assert!(repo.get_note(&note.id).unwrap().deleted_at.is_some());
    }

    #[test]
    fn persists_to_disk() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("snapline.db");
        let t1 = Utc.with_ymd_and_hms(2026, 4, 29, 2, 0, 0).unwrap();
        let note_id = {
            let repo = NoteRepository::open(&db_path).unwrap();
            let note = repo.create_note(t1).unwrap();
            repo.update_note_content(&note.id, "Persistent", t1).unwrap();
            note.id
        };

        let repo = NoteRepository::open(&db_path).unwrap();
        assert_eq!(repo.get_note(&note_id).unwrap().content_md, "Persistent");
    }
}
```

- [ ] **Step 3: Run storage tests**

Run:

```powershell
cargo test -p snapline-storage
```

Expected: both repository tests pass.

- [ ] **Step 4: Commit task 2**

Run:

```powershell
git add crates/storage Cargo.toml
git commit -m "feat: add sqlite note storage"
```

## Task 3: Platform Paths And App Core

**Files:**
- Create: `crates/platform/Cargo.toml`
- Create: `crates/platform/src/lib.rs`
- Create: `crates/app-core/Cargo.toml`
- Create: `crates/app-core/src/lib.rs`
- Test: `crates/app-core/src/lib.rs`

- [ ] **Step 1: Add platform path helpers**

Create `crates/platform/Cargo.toml`:

```toml
[package]
name = "snapline-platform"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
anyhow.workspace = true
directories.workspace = true
snapline-domain = { path = "../domain" }
```

Create `crates/platform/src/lib.rs`:

```rust
use anyhow::{anyhow, Result};
use directories::ProjectDirs;
use snapline_domain::{AssetId, NoteId};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub db_path: PathBuf,
}

impl AppPaths {
    pub fn resolve() -> Result<Self> {
        let dirs = ProjectDirs::from("", "", "Snapline")
            .ok_or_else(|| anyhow!("could not resolve Snapline data directory"))?;
        Ok(Self::from_data_dir(dirs.data_dir()))
    }

    pub fn from_data_dir(data_dir: impl AsRef<Path>) -> Self {
        let data_dir = data_dir.as_ref().to_path_buf();
        Self {
            db_path: data_dir.join("snapline.db"),
            data_dir,
        }
    }

    pub fn note_asset_dir(&self, note_id: &NoteId) -> PathBuf {
        self.data_dir.join("assets").join("notes").join(note_id.to_string())
    }

    pub fn note_asset_path(&self, note_id: &NoteId, asset_id: &AssetId, ext: &str) -> PathBuf {
        self.note_asset_dir(note_id).join(format!("{}.{}", asset_id, ext))
    }

    pub fn markdown_asset_path(&self, note_id: &NoteId, asset_id: &AssetId, ext: &str) -> String {
        format!("assets/notes/{}/{}.{}", note_id, asset_id, ext)
    }
}
```

- [ ] **Step 2: Add app-core use cases and tests**

Create `crates/app-core/Cargo.toml`:

```toml
[package]
name = "snapline-app-core"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
anyhow.workspace = true
chrono.workspace = true
serde.workspace = true
snapline-domain = { path = "../domain" }
snapline-platform = { path = "../platform" }
snapline-storage = { path = "../storage" }
uuid.workspace = true

[dev-dependencies]
tempfile.workspace = true
```

Create `crates/app-core/src/lib.rs`:

```rust
use anyhow::{bail, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use snapline_domain::{AssetId, AssetRef, Note, NoteId, NoteSummary};
use snapline_platform::AppPaths;
use snapline_storage::NoteRepository;
use std::fs;

pub struct AppCore {
    repo: NoteRepository,
    paths: AppPaths,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapState {
    pub notes: Vec<NoteSummary>,
    pub current: Note,
}

impl AppCore {
    pub fn open(paths: AppPaths) -> Result<Self> {
        fs::create_dir_all(&paths.data_dir)?;
        let repo = NoteRepository::open(&paths.db_path)?;
        Ok(Self { repo, paths })
    }

    pub fn with_repo(paths: AppPaths, repo: NoteRepository) -> Self {
        Self { repo, paths }
    }

    pub fn bootstrap(&self) -> Result<BootstrapState> {
        let mut notes = self.repo.list_recent(50)?;
        let current = if let Some(first) = notes.first() {
            self.repo.get_note(&first.id)?
        } else {
            self.repo.create_note(Utc::now())?
        };
        notes = self.repo.list_recent(50)?;
        Ok(BootstrapState { notes, current })
    }

    pub fn create_note(&self) -> Result<Note> {
        self.repo.create_note(Utc::now())
    }

    pub fn get_note(&self, id: &NoteId) -> Result<Note> {
        self.repo.get_note(id)
    }

    pub fn save_note(&self, id: &NoteId, content_md: &str) -> Result<Note> {
        self.repo.update_note_content(id, content_md, Utc::now())
    }

    pub fn delete_note(&self, id: &NoteId) -> Result<Vec<NoteSummary>> {
        self.repo.soft_delete(id, Utc::now())?;
        self.repo.list_recent(50)
    }

    pub fn save_png_asset(&self, note_id: &NoteId, png_bytes: &[u8]) -> Result<AssetRef> {
        if png_bytes.is_empty() {
            bail!("image bytes are empty");
        }
        let asset_id = AssetId::new();
        let dir = self.paths.note_asset_dir(note_id);
        fs::create_dir_all(&dir)?;
        let path = self.paths.note_asset_path(note_id, &asset_id, "png");
        fs::write(path, png_bytes)?;
        Ok(AssetRef {
            markdown_path: self.paths.markdown_asset_path(note_id, &asset_id, "png"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::AppCore;
    use snapline_platform::AppPaths;
    use snapline_storage::NoteRepository;

    #[test]
    fn bootstrap_creates_first_note() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let repo = NoteRepository::open_in_memory().unwrap();
        let core = AppCore::with_repo(paths, repo);

        let state = core.bootstrap().unwrap();

        assert_eq!(state.notes.len(), 1);
        assert_eq!(state.current.title, "Untitled");
    }

    #[test]
    fn saves_png_asset_under_note_directory() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let repo = NoteRepository::open_in_memory().unwrap();
        let core = AppCore::with_repo(paths, repo);
        let note = core.create_note().unwrap();

        let asset = core.save_png_asset(&note.id, &[137, 80, 78, 71]).unwrap();

        assert!(asset.markdown_path.starts_with(&format!("assets/notes/{}/", note.id)));
        assert!(dir.path().join(&asset.markdown_path).exists());
    }
}
```

- [ ] **Step 3: Run app-core tests**

Run:

```powershell
cargo test -p snapline-app-core
```

Expected: both app-core tests pass.

- [ ] **Step 4: Commit task 3**

Run:

```powershell
git add crates/platform crates/app-core Cargo.toml
git commit -m "feat: add app core use cases"
```

## Task 4: Tauri Shell And Commands

**Files:**
- Create: `apps/desktop-tauri/package.json`
- Create: `apps/desktop-tauri/index.html`
- Create: `apps/desktop-tauri/vite.config.ts`
- Create: `apps/desktop-tauri/tsconfig.json`
- Create: `apps/desktop-tauri/src-tauri/Cargo.toml`
- Create: `apps/desktop-tauri/src-tauri/build.rs`
- Create: `apps/desktop-tauri/src-tauri/tauri.conf.json`
- Create: `apps/desktop-tauri/src-tauri/src/main.rs`

- [ ] **Step 1: Add Tauri frontend project files**

Create `apps/desktop-tauri/package.json`:

```json
{
  "name": "snapline-desktop",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "test": "vitest run",
    "tauri": "tauri"
  },
  "dependencies": {
    "@tauri-apps/api": "^2.0.0",
    "@tiptap/core": "^3.0.0",
    "@tiptap/extension-image": "^3.0.0",
    "@tiptap/extension-link": "^3.0.0",
    "@tiptap/extension-placeholder": "^3.0.0",
    "@tiptap/markdown": "^3.0.0",
    "@tiptap/react": "^3.0.0",
    "@tiptap/starter-kit": "^3.0.0",
    "react": "^18.3.1",
    "react-dom": "^18.3.1"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2.0.0",
    "@testing-library/react": "^15.0.0",
    "@types/react": "^18.3.1",
    "@types/react-dom": "^18.3.1",
    "@vitejs/plugin-react": "^4.2.1",
    "typescript": "^5.5.0",
    "vite": "^5.4.0",
    "vitest": "^1.6.0"
  }
}
```

Create `apps/desktop-tauri/index.html`:

```html
<div id="root"></div>
<script type="module" src="/src/main.tsx"></script>
```

Create `apps/desktop-tauri/vite.config.ts`:

```ts
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
});
```

Create `apps/desktop-tauri/tsconfig.json`:

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "useDefineForClassFields": true,
    "lib": ["DOM", "DOM.Iterable", "ES2020"],
    "allowJs": false,
    "skipLibCheck": true,
    "esModuleInterop": true,
    "allowSyntheticDefaultImports": true,
    "strict": true,
    "forceConsistentCasingInFileNames": true,
    "module": "ESNext",
    "moduleResolution": "Node",
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "react-jsx"
  },
  "include": ["src"]
}
```

- [ ] **Step 2: Add Tauri Rust command handlers**

Create `apps/desktop-tauri/src-tauri/Cargo.toml`:

```toml
[package]
name = "snapline-desktop"
version.workspace = true
edition.workspace = true
license.workspace = true

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
anyhow.workspace = true
serde.workspace = true
snapline-app-core = { path = "../../../crates/app-core" }
snapline-domain = { path = "../../../crates/domain" }
snapline-platform = { path = "../../../crates/platform" }
tauri = { version = "2", features = [] }
tokio = { version = "1", features = ["sync"] }
uuid.workspace = true
```

Create `apps/desktop-tauri/src-tauri/build.rs`:

```rust
fn main() {
    tauri_build::build();
}
```

Create `apps/desktop-tauri/src-tauri/tauri.conf.json`:

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Snapline",
  "version": "0.1.0",
  "identifier": "app.snapline.desktop",
  "build": {
    "beforeDevCommand": "npm run dev",
    "devUrl": "http://localhost:1420",
    "beforeBuildCommand": "npm run build",
    "frontendDist": "../dist"
  },
  "app": {
    "windows": [
      {
        "title": "Snapline",
        "width": 1120,
        "height": 760,
        "minWidth": 860,
        "minHeight": 560
      }
    ]
  }
}
```

Create `apps/desktop-tauri/src-tauri/src/main.rs`:

```rust
use snapline_app_core::{AppCore, BootstrapState};
use snapline_domain::{AssetRef, Note, NoteId, NoteSummary};
use snapline_platform::AppPaths;
use std::sync::Mutex;
use tauri::State;

struct AppState {
    core: Mutex<AppCore>,
}

#[tauri::command]
fn bootstrap(state: State<'_, AppState>) -> Result<BootstrapState, String> {
    let started = std::time::Instant::now();
    let result = state.core.lock().map_err(|_| "app state lock poisoned".to_string())?.bootstrap();
    eprintln!("snapline.bootstrap_ms={}", started.elapsed().as_millis());
    result.map_err(|err| err.to_string())
}

#[tauri::command]
fn create_note(state: State<'_, AppState>) -> Result<Note, String> {
    state.core.lock().map_err(|_| "app state lock poisoned".to_string())?.create_note().map_err(|err| err.to_string())
}

#[tauri::command]
fn get_note(state: State<'_, AppState>, id: String) -> Result<Note, String> {
    let id = parse_note_id(id)?;
    state.core.lock().map_err(|_| "app state lock poisoned".to_string())?.get_note(&id).map_err(|err| err.to_string())
}

#[tauri::command]
fn save_note(state: State<'_, AppState>, id: String, content_md: String) -> Result<Note, String> {
    let id = parse_note_id(id)?;
    let started = std::time::Instant::now();
    let result = state.core.lock().map_err(|_| "app state lock poisoned".to_string())?.save_note(&id, &content_md);
    eprintln!("snapline.save_note_ms={}", started.elapsed().as_millis());
    result.map_err(|err| err.to_string())
}

#[tauri::command]
fn delete_note(state: State<'_, AppState>, id: String) -> Result<Vec<NoteSummary>, String> {
    let id = parse_note_id(id)?;
    state.core.lock().map_err(|_| "app state lock poisoned".to_string())?.delete_note(&id).map_err(|err| err.to_string())
}

#[tauri::command]
fn save_png_asset(state: State<'_, AppState>, note_id: String, bytes: Vec<u8>) -> Result<AssetRef, String> {
    let note_id = parse_note_id(note_id)?;
    state.core.lock().map_err(|_| "app state lock poisoned".to_string())?.save_png_asset(&note_id, &bytes).map_err(|err| err.to_string())
}

fn parse_note_id(value: String) -> Result<NoteId, String> {
    uuid::Uuid::parse_str(&value)
        .map(NoteId)
        .map_err(|err| format!("invalid note id: {err}"))
}

fn main() {
    let paths = AppPaths::resolve().expect("resolve app paths");
    let core = AppCore::open(paths).expect("open app core");

    tauri::Builder::default()
        .manage(AppState {
            core: Mutex::new(core),
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            create_note,
            get_note,
            save_note,
            delete_note,
            save_png_asset
        ])
        .run(tauri::generate_context!())
        .expect("error while running Snapline");
}
```

- [ ] **Step 3: Check Tauri Rust compilation**

Run:

```powershell
cargo check -p snapline-desktop
```

Expected: command handlers compile.

- [ ] **Step 4: Commit task 4**

Run:

```powershell
git add apps/desktop-tauri Cargo.toml
git commit -m "feat: add tauri command shell"
```

## Task 5: Frontend Types, Markdown Adapter, And API Client

**Files:**
- Create: `apps/desktop-tauri/src/types.ts`
- Create: `apps/desktop-tauri/src/api.ts`
- Create: `apps/desktop-tauri/src/markdown.ts`
- Create: `apps/desktop-tauri/src/markdown.test.ts`

- [ ] **Step 1: Add shared frontend types**

Create `apps/desktop-tauri/src/types.ts`:

```ts
export type NoteId = string;

export interface Note {
  id: NoteId;
  title: string;
  content_md: string;
  created_at: string;
  updated_at: string;
  deleted_at: string | null;
}

export interface NoteSummary {
  id: NoteId;
  title: string;
  updated_at: string;
}

export interface BootstrapState {
  notes: NoteSummary[];
  current: Note;
}

export interface AssetRef {
  markdown_path: string;
}
```

- [ ] **Step 2: Add Tauri API wrapper**

Create `apps/desktop-tauri/src/api.ts`:

```ts
import { invoke } from "@tauri-apps/api/core";
import type { AssetRef, BootstrapState, Note, NoteSummary } from "./types";

export const api = {
  bootstrap: () => invoke<BootstrapState>("bootstrap"),
  createNote: () => invoke<Note>("create_note"),
  getNote: (id: string) => invoke<Note>("get_note", { id }),
  saveNote: (id: string, contentMd: string) =>
    invoke<Note>("save_note", { id, contentMd }),
  deleteNote: (id: string) => invoke<NoteSummary[]>("delete_note", { id }),
  savePngAsset: (noteId: string, bytes: number[]) =>
    invoke<AssetRef>("save_png_asset", { noteId, bytes }),
};
```

- [ ] **Step 3: Add minimal Markdown adapter and tests**

Create `apps/desktop-tauri/src/markdown.ts`:

```ts
export function normalizeMarkdown(markdown: string): string {
  return markdown.replace(/\r\n/g, "\n").trimEnd();
}

export function imageMarkdown(path: string): string {
  return `![](${path})`;
}

export function titleFromMarkdown(markdown: string): string {
  const first = normalizeMarkdown(markdown)
    .split("\n")
    .map((line) => line.trim())
    .find(Boolean);
  if (!first) return "Untitled";
  return first.replace(/^#+\s*/, "").trim() || "Untitled";
}
```

Create `apps/desktop-tauri/src/markdown.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { imageMarkdown, normalizeMarkdown, titleFromMarkdown } from "./markdown";

describe("markdown helpers", () => {
  it("normalizes line endings and trims trailing whitespace", () => {
    expect(normalizeMarkdown("# A\r\nBody\n\n")).toBe("# A\nBody");
  });

  it("derives display title from markdown", () => {
    expect(titleFromMarkdown("\n## Heading\nBody")).toBe("Heading");
    expect(titleFromMarkdown("")).toBe("Untitled");
  });

  it("creates markdown image references", () => {
    expect(imageMarkdown("assets/notes/note/image.png")).toBe("![](assets/notes/note/image.png)");
  });
});
```

- [ ] **Step 4: Run frontend helper tests**

Run:

```powershell
cd apps\desktop-tauri
npm install
npm test
```

Expected: all Vitest tests pass.

- [ ] **Step 5: Commit task 5**

Run:

```powershell
git add apps/desktop-tauri/package-lock.json apps/desktop-tauri/src
git commit -m "feat: add desktop frontend helpers"
```

## Task 6: React Shell, Tiptap Editor, Autosave, And Paste Image

**Files:**
- Create: `apps/desktop-tauri/src/main.tsx`
- Create: `apps/desktop-tauri/src/App.tsx`
- Create: `apps/desktop-tauri/src/EditorPane.tsx`
- Create: `apps/desktop-tauri/src/styles.css`

- [ ] **Step 1: Add React entrypoint**

Create `apps/desktop-tauri/src/main.tsx`:

```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import "./styles.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
```

- [ ] **Step 2: Add main app shell**

Create `apps/desktop-tauri/src/App.tsx`:

```tsx
import { useEffect, useState } from "react";
import { api } from "./api";
import { EditorPane } from "./EditorPane";
import type { Note, NoteSummary } from "./types";

export function App() {
  const [notes, setNotes] = useState<NoteSummary[]>([]);
  const [current, setCurrent] = useState<Note | null>(null);
  const [status, setStatus] = useState("Loading");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const started = performance.now();
    api
      .bootstrap()
      .then((state) => {
        setNotes(state.notes);
        setCurrent(state.current);
        setStatus("Saved");
        console.info("snapline.frontend_bootstrap_ms", Math.round(performance.now() - started));
      })
      .catch((err) => {
        setError(String(err));
        setStatus("Error");
      });
  }, []);

  async function createNote() {
    const note = await api.createNote();
    setCurrent(note);
    setNotes((existing) => [{ id: note.id, title: note.title, updated_at: note.updated_at }, ...existing]);
  }

  async function deleteCurrent() {
    if (!current) return;
    const nextNotes = await api.deleteNote(current.id);
    setNotes(nextNotes);
    setCurrent(null);
  }

  function onSaved(note: Note) {
    setCurrent(note);
    setNotes((existing) => {
      const without = existing.filter((item) => item.id !== note.id);
      return [{ id: note.id, title: note.title, updated_at: note.updated_at }, ...without];
    });
  }

  return (
    <main className="app">
      <aside className="sidebar">
        <div className="sidebarHeader">
          <div className="brand">Snapline</div>
          <button onClick={createNote} title="New note">+</button>
        </div>
        <nav className="noteList">
          {notes.map((note) => (
            <button
              className={note.id === current?.id ? "noteItem active" : "noteItem"}
              key={note.id}
              onClick={async () => {
                const selected = await api.getNote(note.id);
                setCurrent(selected);
              }}
            >
              {note.title}
            </button>
          ))}
        </nav>
      </aside>
      <section className="workspace">
        <header className="topbar">
          <span>{status}</span>
          <button disabled={!current} onClick={deleteCurrent}>Delete</button>
        </header>
        {error ? <div className="error">{error}</div> : null}
        {current ? (
          <EditorPane note={current} setStatus={setStatus} onSaved={onSaved} />
        ) : (
          <div className="empty">Create or select a note</div>
        )}
      </section>
    </main>
  );
}
```

- [ ] **Step 3: Add Tiptap editor with autosave and image paste**

Create `apps/desktop-tauri/src/EditorPane.tsx`:

```tsx
import Image from "@tiptap/extension-image";
import Link from "@tiptap/extension-link";
import { Markdown } from "@tiptap/markdown";
import Placeholder from "@tiptap/extension-placeholder";
import { EditorContent, useEditor } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import { useEffect, useRef } from "react";
import { api } from "./api";
import { imageMarkdown, normalizeMarkdown } from "./markdown";
import type { Note } from "./types";

interface EditorPaneProps {
  note: Note;
  setStatus: (status: string) => void;
  onSaved: (note: Note) => void;
}

export function EditorPane({ note, setStatus, onSaved }: EditorPaneProps) {
  const saveTimer = useRef<number | undefined>();

  const editor = useEditor({
    extensions: [
      StarterKit,
      Link,
      Image,
      Markdown.configure({ markedOptions: { gfm: true, breaks: false } }),
      Placeholder.configure({ placeholder: "Write before the thought fades..." }),
    ],
    content: markdownToHtml(note.content_md),
    contentType: "markdown",
    editorProps: {
      handlePaste(view, event) {
        const file = Array.from(event.clipboardData?.files ?? []).find((item) =>
          item.type.startsWith("image/"),
        );
        if (!file) return false;
        event.preventDefault();
        void file.arrayBuffer().then(async (buffer) => {
          const bytes = Array.from(new Uint8Array(buffer));
          const tempUrl = URL.createObjectURL(file);
          editor?.chain().focus().setImage({ src: tempUrl }).run();
          const asset = await api.savePngAsset(note.id, bytes);
          editor?.chain().focus().updateAttributes("image", { src: asset.markdown_path }).run();
          queueSave(editor?.getMarkdown() ?? imageMarkdown(asset.markdown_path));
        });
        return true;
      },
    },
    onUpdate({ editor }) {
      queueSave(editor.getMarkdown());
    },
  });

  useEffect(() => {
    editor?.commands.setContent(note.content_md, { contentType: "markdown" });
  }, [editor, note.id]);

  function queueSave(contentMd: string) {
    window.clearTimeout(saveTimer.current);
    setStatus("Saving");
    saveTimer.current = window.setTimeout(async () => {
      try {
        const saved = await api.saveNote(note.id, normalizeMarkdown(contentMd));
        onSaved(saved);
        setStatus("Saved");
      } catch (err) {
        console.error(err);
        setStatus("Error");
      }
    }, 600);
  }

  return <EditorContent className="editor" editor={editor} />;
}
```

The editor uses Tiptap's official Markdown extension: set content with `{ contentType: "markdown" }` and save with `editor.getMarkdown()`.

- [ ] **Step 4: Add focused application CSS**

Create `apps/desktop-tauri/src/styles.css`:

```css
html,
body,
#root {
  height: 100%;
  margin: 0;
}

body {
  font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  color: #202124;
  background: #f6f7f8;
}

button {
  font: inherit;
}

.app {
  display: grid;
  grid-template-columns: 260px minmax(0, 1fr);
  height: 100%;
}

.sidebar {
  border-right: 1px solid #d9dde3;
  background: #eef1f4;
  min-width: 0;
}

.sidebarHeader,
.topbar {
  height: 52px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 14px;
  border-bottom: 1px solid #d9dde3;
}

.brand {
  font-weight: 650;
}

.noteList {
  display: flex;
  flex-direction: column;
  padding: 8px;
  gap: 4px;
}

.noteItem {
  border: 0;
  background: transparent;
  border-radius: 6px;
  padding: 9px 10px;
  text-align: left;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}

.noteItem.active,
.noteItem:hover {
  background: #ffffff;
}

.workspace {
  min-width: 0;
  background: #ffffff;
}

.editor {
  max-width: 820px;
  margin: 0 auto;
  padding: 32px 40px;
}

.editor .ProseMirror {
  outline: none;
  min-height: calc(100vh - 140px);
  line-height: 1.58;
  font-size: 16px;
}

.editor img {
  max-width: 100%;
  border-radius: 6px;
}

.error,
.empty {
  padding: 24px;
}
```

- [ ] **Step 5: Build frontend**

Run:

```powershell
cd apps\desktop-tauri
npm run build
```

Expected: TypeScript and Vite build complete.

- [ ] **Step 6: Commit task 6**

Run:

```powershell
git add apps/desktop-tauri/src
git commit -m "feat: add rendered markdown editor UI"
```

## Task 7: Markdown Round-Trip And Full Checks

**Files:**
- Modify: `apps/desktop-tauri/src/EditorPane.tsx`
- Test: `apps/desktop-tauri/src/markdown.test.ts`

- [ ] **Step 1: Add explicit Markdown round-trip coverage**

Extend `apps/desktop-tauri/src/markdown.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { imageMarkdown, normalizeMarkdown, titleFromMarkdown } from "./markdown";

describe("markdown helpers", () => {
  it("normalizes line endings and trims trailing whitespace", () => {
    expect(normalizeMarkdown("# A\r\nBody\n\n")).toBe("# A\nBody");
  });

  it("derives display title from markdown", () => {
    expect(titleFromMarkdown("\n## Heading\nBody")).toBe("Heading");
    expect(titleFromMarkdown("")).toBe("Untitled");
  });

  it("creates markdown image references", () => {
    expect(imageMarkdown("assets/notes/note/image.png")).toBe("![](assets/notes/note/image.png)");
  });

  it("keeps m1 markdown nodes in normalized storage text", () => {
    const source = [
      "# Heading",
      "",
      "Plain paragraph with **bold** text.",
      "",
      "- One",
      "- Two",
      "",
      "![](assets/notes/note/image.png)"
    ].join("\n");

    expect(normalizeMarkdown(source)).toContain("**bold**");
    expect(normalizeMarkdown(source)).toContain("![](assets/notes/note/image.png)");
  });
});
```

The Tiptap editor in `EditorPane.tsx` must save with `editor.getMarkdown()` and must set loaded content with `contentType: "markdown"`. If this test exposes a lost M1 node during manual verification, fix the Tiptap extension list before completing Task 7.

- [ ] **Step 2: Run Rust and frontend checks**

Run:

```powershell
cargo test
cd apps\desktop-tauri
npm test
npm run build
```

Expected: all tests and builds pass.

- [ ] **Step 3: Commit task 7**

Run:

```powershell
git add apps/desktop-tauri
git commit -m "test: cover markdown persistence format"
```

## Task 8: Manual MVP Verification

**Files:**
- Modify only if verification exposes defects.

- [ ] **Step 1: Run the desktop app**

Run:

```powershell
cd apps\desktop-tauri
npm run tauri dev
```

Expected: Snapline window opens.

- [ ] **Step 2: Verify note persistence**

Manual flow:

1. Create a note.
2. Type a heading and two paragraphs.
3. Wait for `Saved`.
4. Close the app.
5. Reopen the app.

Expected: the note still appears with its content.

- [ ] **Step 3: Verify image paste**

Manual flow:

1. Copy a PNG image to the clipboard.
2. Paste into the editor.
3. Confirm the image appears in the editor.
4. Wait for `Saved`.
5. Close and reopen the app.

Expected:

- The image still appears in the note.
- The file exists under the app data directory at `assets/notes/<note_id>/<image_id>.png`.
- The saved Markdown contains `![](assets/notes/<note_id>/<image_id>.png)`.

- [ ] **Step 4: Verify soft delete**

Manual flow:

1. Select a note.
2. Click Delete.
3. Restart the app.

Expected: deleted note is not shown in the recent list, and the row remains in SQLite with `deleted_at` set.

- [ ] **Step 5: Commit fixes or verification notes**

If code changed:

```powershell
git add .
git commit -m "fix: stabilize m1 desktop verification"
```

If no code changed, do not create an empty commit.

## Self-Review

- Spec coverage: M1 desktop shell, rendered editor, SQLite persistence, autosave, soft delete, image paste, local assets, and performance instrumentation are covered.
- Deferred by design: sync, FTS, restore UI, auth, server APIs, and packaging.
- Risk called out: Markdown round-trip is covered by tests and by the Task 8 manual persistence flow because Tiptap's Markdown extension is beta.
- Plan scan: all code-changing steps include exact files, commands, and expected outcomes.
- Type consistency: Rust uses `content_md` for serialized field names and frontend mirrors the same property names.
