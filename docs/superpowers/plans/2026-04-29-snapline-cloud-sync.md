# Snapline M2 Cloud Sync Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add optional account-based cloud sync with an open source self-hostable backend, including note metadata, soft deletes, pinned state, and pasted image assets.

**Architecture:** Keep Snapline local-first by extending SQLite with sync metadata and an outbound queue. Add a Rust `sync-client` crate for protocol types and client orchestration, and an Axum `sync-server` crate backed by PostgreSQL plus a local filesystem asset store. Wire the desktop UI to login, choose a server URL, display sync status, and run background sync without blocking editing.

**Tech Stack:** Rust workspace, rusqlite, reqwest, Axum, SQLx, PostgreSQL, Tokio, JWT, Argon2, Docker Compose, React, TypeScript, Vitest.

---

## File Structure

- `Cargo.toml`: add `crates/sync-client` and `crates/sync-server`; add shared dependencies.
- `crates/domain/src/note.rs`: add sync metadata to `Note` and `NoteSummary`.
- `crates/domain/src/sync.rs`: define shared sync operation and conflict types.
- `crates/domain/src/asset.rs`: add stable `AssetId` parsing and `AssetMetadata`.
- `crates/storage/src/repository.rs`: migrate notes sync fields; add queue and sync state repositories.
- `crates/storage/src/sync.rs`: focused SQLite sync queue/state implementation.
- `crates/app-core/src/lib.rs`: enqueue sync changes on save, pin, delete, and image save.
- `crates/sync-client`: protocol DTOs, HTTP client, queue processor, mock sync tests.
- `crates/sync-server`: Axum routes, PostgreSQL migrations, auth, sync handlers, asset store.
- `apps/desktop-tauri/src-tauri/src/main.rs`: Tauri commands for login, sync settings, and manual sync.
- `apps/desktop-tauri/src/api.ts`: frontend wrappers for sync commands.
- `apps/desktop-tauri/src/types.ts`: sync status/account types.
- `apps/desktop-tauri/src/App.tsx`: sync status indicator and login/settings entry points.
- `apps/desktop-tauri/src/SyncSettings.tsx`: server URL and account login UI.
- `docker-compose.sync.yml`: self-hosted server plus PostgreSQL.
- `docs/self-hosting.md`: setup, backup, and registration configuration.

## Task 1: Shared Domain Sync Types

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/domain/src/lib.rs`
- Modify: `crates/domain/src/note.rs`
- Modify: `crates/domain/src/asset.rs`
- Create: `crates/domain/src/sync.rs`
- Test: `crates/domain/src/sync.rs`

- [ ] **Step 1: Add serde_json to workspace dependencies**

Edit `Cargo.toml`:

```toml
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

- [ ] **Step 2: Extend domain note types with sync metadata**

In `crates/domain/src/note.rs`, update `Note` and `NoteSummary` fields and `Note::draft`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub id: NoteId,
    pub title: String,
    pub content_md: String,
    pub pinned: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub server_version: i64,
    pub last_modified_by_device: Option<String>,
    pub is_conflict_copy: bool,
    pub source_note_id: Option<NoteId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteSummary {
    pub id: NoteId,
    pub title: String,
    pub preview: String,
    pub preview_md: String,
    pub pinned: bool,
    pub updated_at: DateTime<Utc>,
    pub is_conflict_copy: bool,
    pub source_note_id: Option<NoteId>,
}

impl Note {
    pub fn draft(now: DateTime<Utc>) -> Self {
        Self {
            id: NoteId::new(),
            title: "Untitled".to_string(),
            content_md: String::new(),
            pinned: false,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            server_version: 0,
            last_modified_by_device: None,
            is_conflict_copy: false,
            source_note_id: None,
        }
    }
}
```

- [ ] **Step 3: Add parsing helpers and asset metadata**

In `crates/domain/src/asset.rs`, keep existing types and add:

```rust
use chrono::{DateTime, Utc};

impl AssetId {
    pub fn parse(value: &str) -> Result<Self, uuid::Error> {
        Uuid::parse_str(value).map(Self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetMetadata {
    pub id: AssetId,
    pub note_id: crate::NoteId,
    pub content_type: String,
    pub byte_size: i64,
    pub sha256: String,
    pub storage_key: String,
    pub created_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}
```

- [ ] **Step 4: Add shared sync operation types**

Create `crates/domain/src/sync.rs`:

```rust
use crate::{AssetId, Note, NoteId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncOpType {
    UpsertNote,
    DeleteNote,
    AssetUpload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteChangePayload {
    pub title: String,
    pub content_md: String,
    pub pinned: bool,
    pub deleted_at: Option<DateTime<Utc>>,
}

impl NoteChangePayload {
    pub fn from_note(note: &Note) -> Self {
        Self {
            title: note.title.clone(),
            content_md: note.content_md.clone(),
            pinned: note.pinned,
            deleted_at: note.deleted_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetUploadPayload {
    pub asset_id: AssetId,
    pub note_id: NoteId,
    pub content_type: String,
    pub byte_size: i64,
    pub sha256: String,
    pub markdown_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SyncPayload {
    Note(NoteChangePayload),
    Asset(AssetUploadPayload),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictCopyRequest {
    pub source_note_id: NoteId,
    pub rejected_note: Note,
    pub server_note: Note,
}
```

- [ ] **Step 5: Export sync types**

In `crates/domain/src/lib.rs`, add:

```rust
pub mod sync;

pub use sync::{
    AssetUploadPayload, ConflictCopyRequest, NoteChangePayload, SyncOpType, SyncPayload,
};
```

- [ ] **Step 6: Add serialization tests**

Append to `crates/domain/src/sync.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::Note;
    use chrono::{TimeZone, Utc};

    #[test]
    fn note_payload_round_trips_as_json() {
        let mut note = Note::draft(Utc.with_ymd_and_hms(2026, 4, 29, 1, 0, 0).unwrap());
        note.title = "Hello".to_string();
        note.content_md = "# Hello".to_string();
        note.pinned = true;

        let payload = SyncPayload::Note(NoteChangePayload::from_note(&note));
        let json = serde_json::to_string(&payload).unwrap();
        let decoded: SyncPayload = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, payload);
        assert!(json.contains("upsert_note") == false);
        assert!(json.contains("Hello"));
    }
}
```

- [ ] **Step 7: Run domain tests**

Run:

```powershell
cargo test -p snapline-domain
```

Expected: all domain tests pass.

- [ ] **Step 8: Commit**

Run:

```powershell
git add Cargo.toml crates/domain
git commit -m "feat: add sync domain types"
```

## Task 2: SQLite Sync Schema And Queue

**Files:**
- Modify: `crates/storage/Cargo.toml`
- Modify: `crates/storage/src/lib.rs`
- Modify: `crates/storage/src/repository.rs`
- Create: `crates/storage/src/sync.rs`
- Test: `crates/storage/src/sync.rs`

- [ ] **Step 1: Add serde dependencies to storage**

Edit `crates/storage/Cargo.toml`:

```toml
[dependencies]
anyhow.workspace = true
chrono.workspace = true
rusqlite.workspace = true
serde.workspace = true
serde_json.workspace = true
snapline-domain = { path = "../domain" }
uuid.workspace = true
```

- [ ] **Step 2: Export sync storage types**

In `crates/storage/src/lib.rs`, add:

```rust
pub mod sync;

pub use sync::{ChangeQueueItem, SyncState};
```

- [ ] **Step 3: Extend notes migration**

In `crates/storage/src/repository.rs`, extend the `CREATE TABLE notes` statement with:

```sql
server_version INTEGER NOT NULL DEFAULT 0,
last_modified_by_device TEXT,
is_conflict_copy INTEGER NOT NULL DEFAULT 0,
source_note_id TEXT
```

Then update `migrate()` after the current `ensure_column("notes", "pinned", ...)` call:

```rust
self.ensure_column("notes", "server_version", "INTEGER NOT NULL DEFAULT 0")?;
self.ensure_column("notes", "last_modified_by_device", "TEXT")?;
self.ensure_column("notes", "is_conflict_copy", "INTEGER NOT NULL DEFAULT 0")?;
self.ensure_column("notes", "source_note_id", "TEXT")?;
sync::migrate_sync_tables(&self.conn)?;
```

Also add this import at the top:

```rust
use crate::sync;
```

- [ ] **Step 4: Update note row mapping**

Update every note select in `repository.rs` to include the four sync fields:

```sql
SELECT id, title, content_md, pinned, created_at, updated_at, deleted_at,
       server_version, last_modified_by_device, is_conflict_copy, source_note_id
FROM notes
```

Update `row_to_note`:

```rust
fn row_to_note(row: &rusqlite::Row<'_>) -> rusqlite::Result<Note> {
    let deleted: Option<String> = row.get(6)?;
    let source_note_id: Option<String> = row.get(10)?;
    Ok(Note {
        id: NoteId(parse_uuid(row.get::<_, String>(0)?)?),
        title: row.get(1)?,
        content_md: row.get(2)?,
        pinned: row.get::<_, i64>(3)? != 0,
        created_at: parse_time(row.get::<_, String>(4)?)?,
        updated_at: parse_time(row.get::<_, String>(5)?)?,
        deleted_at: deleted.map(parse_time).transpose()?,
        server_version: row.get(7)?,
        last_modified_by_device: row.get(8)?,
        is_conflict_copy: row.get::<_, i64>(9)? != 0,
        source_note_id: source_note_id
            .map(|value| parse_uuid(value).map(NoteId))
            .transpose()?,
    })
}
```

Update `draft_note_with_id` to set the new fields to the same defaults as `Note::draft`.

- [ ] **Step 5: Update summary mapping**

Update `list_recent` select:

```sql
SELECT id, title, pinned, updated_at, content_md, is_conflict_copy, source_note_id FROM notes
WHERE deleted_at IS NULL
ORDER BY pinned DESC, updated_at DESC
LIMIT ?1
```

Update `NoteSummary` construction:

```rust
let source_note_id: Option<String> = row.get(6)?;
Ok(NoteSummary {
    id: NoteId(parse_uuid(row.get::<_, String>(0)?)?),
    title: row.get(1)?,
    pinned: row.get::<_, i64>(2)? != 0,
    updated_at: parse_time(row.get::<_, String>(3)?)?,
    preview: derive_preview(&row.get::<_, String>(4)?),
    preview_md: derive_preview_markdown(&row.get::<_, String>(4)?),
    is_conflict_copy: row.get::<_, i64>(5)? != 0,
    source_note_id: source_note_id
        .map(|value| parse_uuid(value).map(NoteId))
        .transpose()?,
})
```

- [ ] **Step 6: Create sync storage module**

Create `crates/storage/src/sync.rs`:

```rust
use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use snapline_domain::{NoteId, SyncOpType, SyncPayload};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeQueueItem {
    pub id: String,
    pub note_id: NoteId,
    pub op_type: SyncOpType,
    pub base_version: i64,
    pub payload: SyncPayload,
    pub queued_at: DateTime<Utc>,
    pub retry_count: i64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncState {
    pub account_id: Option<String>,
    pub device_id: String,
    pub server_base_url: Option<String>,
    pub server_cursor: i64,
    pub access_token: Option<String>,
    pub last_sync_at: Option<DateTime<Utc>>,
    pub last_success_at: Option<DateTime<Utc>>,
}

pub fn migrate_sync_tables(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS change_queue (
          id TEXT PRIMARY KEY,
          note_id TEXT NOT NULL,
          op_type TEXT NOT NULL,
          base_version INTEGER NOT NULL,
          payload_json TEXT NOT NULL,
          queued_at TEXT NOT NULL,
          retry_count INTEGER NOT NULL DEFAULT 0,
          last_error TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_change_queue_queued_at
        ON change_queue (queued_at ASC);

        CREATE TABLE IF NOT EXISTS sync_state (
          id INTEGER PRIMARY KEY CHECK (id = 1),
          account_id TEXT,
          device_id TEXT NOT NULL,
          server_base_url TEXT,
          server_cursor INTEGER NOT NULL DEFAULT 0,
          access_token TEXT,
          last_sync_at TEXT,
          last_success_at TEXT
        );
        ",
    )?;
    Ok(())
}

pub fn enqueue_change(
    conn: &Connection,
    note_id: &NoteId,
    op_type: SyncOpType,
    base_version: i64,
    payload: &SyncPayload,
    queued_at: DateTime<Utc>,
) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO change_queue (id, note_id, op_type, base_version, payload_json, queued_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            id,
            note_id.to_string(),
            op_type_to_str(&op_type),
            base_version,
            serde_json::to_string(payload)?,
            queued_at.to_rfc3339()
        ],
    )?;
    Ok(id)
}

pub fn list_pending_changes(conn: &Connection, limit: usize) -> Result<Vec<ChangeQueueItem>> {
    let mut stmt = conn.prepare(
        "SELECT id, note_id, op_type, base_version, payload_json, queued_at, retry_count, last_error
         FROM change_queue ORDER BY queued_at ASC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], |row| {
        let note_id = Uuid::parse_str(&row.get::<_, String>(1)?)
            .map(NoteId)
            .map_err(to_sql_err)?;
        let payload_json: String = row.get(4)?;
        Ok(ChangeQueueItem {
            id: row.get(0)?,
            note_id,
            op_type: op_type_from_str(&row.get::<_, String>(2)?).map_err(to_sql_err)?,
            base_version: row.get(3)?,
            payload: serde_json::from_str(&payload_json).map_err(to_sql_err)?,
            queued_at: parse_time(row.get::<_, String>(5)?)?,
            retry_count: row.get(6)?,
            last_error: row.get(7)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
}

pub fn delete_change(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM change_queue WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn mark_change_failed(conn: &Connection, id: &str, error: &str) -> Result<()> {
    conn.execute(
        "UPDATE change_queue SET retry_count = retry_count + 1, last_error = ?1 WHERE id = ?2",
        params![error, id],
    )?;
    Ok(())
}

pub fn get_or_create_sync_state(conn: &Connection) -> Result<SyncState> {
    let existing = conn
        .query_row(
            "SELECT account_id, device_id, server_base_url, server_cursor, access_token, last_sync_at, last_success_at
             FROM sync_state WHERE id = 1",
            [],
            row_to_sync_state,
        )
        .optional()?;
    if let Some(state) = existing {
        return Ok(state);
    }
    let device_id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO sync_state (id, device_id) VALUES (1, ?1)",
        params![device_id],
    )?;
    Ok(SyncState {
        account_id: None,
        device_id,
        server_base_url: None,
        server_cursor: 0,
        access_token: None,
        last_sync_at: None,
        last_success_at: None,
    })
}

pub fn save_sync_state(conn: &Connection, state: &SyncState) -> Result<()> {
    conn.execute(
        "INSERT INTO sync_state
         (id, account_id, device_id, server_base_url, server_cursor, access_token, last_sync_at, last_success_at)
         VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(id) DO UPDATE SET
           account_id = excluded.account_id,
           device_id = excluded.device_id,
           server_base_url = excluded.server_base_url,
           server_cursor = excluded.server_cursor,
           access_token = excluded.access_token,
           last_sync_at = excluded.last_sync_at,
           last_success_at = excluded.last_success_at",
        params![
            state.account_id,
            state.device_id,
            state.server_base_url,
            state.server_cursor,
            state.access_token,
            state.last_sync_at.map(|time| time.to_rfc3339()),
            state.last_success_at.map(|time| time.to_rfc3339())
        ],
    )?;
    Ok(())
}

fn row_to_sync_state(row: &rusqlite::Row<'_>) -> rusqlite::Result<SyncState> {
    let last_sync_at: Option<String> = row.get(5)?;
    let last_success_at: Option<String> = row.get(6)?;
    Ok(SyncState {
        account_id: row.get(0)?,
        device_id: row.get(1)?,
        server_base_url: row.get(2)?,
        server_cursor: row.get(3)?,
        access_token: row.get(4)?,
        last_sync_at: last_sync_at.map(parse_time).transpose()?,
        last_success_at: last_success_at.map(parse_time).transpose()?,
    })
}

fn op_type_to_str(op_type: &SyncOpType) -> &'static str {
    match op_type {
        SyncOpType::UpsertNote => "upsert_note",
        SyncOpType::DeleteNote => "delete_note",
        SyncOpType::AssetUpload => "asset_upload",
    }
}

fn op_type_from_str(value: &str) -> Result<SyncOpType, String> {
    match value {
        "upsert_note" => Ok(SyncOpType::UpsertNote),
        "delete_note" => Ok(SyncOpType::DeleteNote),
        "asset_upload" => Ok(SyncOpType::AssetUpload),
        _ => Err(format!("unknown sync op type: {value}")),
    }
}

fn parse_time(value: String) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&value)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(to_sql_err)
}

fn to_sql_err<E>(err: E) -> rusqlite::Error
where
    E: std::error::Error + Send + Sync + 'static,
{
    rusqlite::Error::ToSqlConversionFailure(Box::new(err))
}
```

- [ ] **Step 7: Add sync storage tests**

Append to `crates/storage/src/sync.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use rusqlite::Connection;
    use snapline_domain::{Note, NoteChangePayload, SyncPayload};

    #[test]
    fn queues_and_lists_note_change() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_sync_tables(&conn).unwrap();
        let note = Note::draft(Utc.with_ymd_and_hms(2026, 4, 29, 1, 0, 0).unwrap());
        let payload = SyncPayload::Note(NoteChangePayload::from_note(&note));

        let id = enqueue_change(
            &conn,
            &note.id,
            SyncOpType::UpsertNote,
            0,
            &payload,
            note.created_at,
        )
        .unwrap();

        let items = list_pending_changes(&conn, 10).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, id);
        assert_eq!(items[0].note_id, note.id);
        assert_eq!(items[0].payload, payload);
    }

    #[test]
    fn sync_state_persists_device_and_cursor() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_sync_tables(&conn).unwrap();
        let mut state = get_or_create_sync_state(&conn).unwrap();
        state.account_id = Some("acct_1".to_string());
        state.server_base_url = Some("http://localhost:8080".to_string());
        state.server_cursor = 42;
        save_sync_state(&conn, &state).unwrap();

        let loaded = get_or_create_sync_state(&conn).unwrap();
        assert_eq!(loaded.device_id, state.device_id);
        assert_eq!(loaded.account_id.as_deref(), Some("acct_1"));
        assert_eq!(loaded.server_cursor, 42);
    }
}
```

- [ ] **Step 8: Run storage tests**

Run:

```powershell
cargo test -p snapline-storage
```

Expected: all storage tests pass.

- [ ] **Step 9: Commit**

Run:

```powershell
git add crates/storage crates/domain Cargo.toml
git commit -m "feat: add local sync queue storage"
```

## Task 3: App Core Queue Integration

**Files:**
- Modify: `crates/storage/src/repository.rs`
- Modify: `crates/app-core/src/lib.rs`
- Test: `crates/app-core/src/lib.rs`

- [ ] **Step 1: Add repository queue wrappers**

In `crates/storage/src/repository.rs`, add imports:

```rust
use snapline_domain::{SyncOpType, SyncPayload};
use crate::sync::{self, ChangeQueueItem, SyncState};
```

Add methods inside `impl NoteRepository`:

```rust
pub fn enqueue_change(
    &self,
    note_id: &NoteId,
    op_type: SyncOpType,
    base_version: i64,
    payload: &SyncPayload,
    queued_at: DateTime<Utc>,
) -> Result<String> {
    sync::enqueue_change(&self.conn, note_id, op_type, base_version, payload, queued_at)
}

pub fn list_pending_changes(&self, limit: usize) -> Result<Vec<ChangeQueueItem>> {
    sync::list_pending_changes(&self.conn, limit)
}

pub fn delete_change(&self, id: &str) -> Result<()> {
    sync::delete_change(&self.conn, id)
}

pub fn mark_change_failed(&self, id: &str, error: &str) -> Result<()> {
    sync::mark_change_failed(&self.conn, id, error)
}

pub fn get_or_create_sync_state(&self) -> Result<SyncState> {
    sync::get_or_create_sync_state(&self.conn)
}

pub fn save_sync_state(&self, state: &SyncState) -> Result<()> {
    sync::save_sync_state(&self.conn, state)
}
```

- [ ] **Step 2: Enqueue note saves**

In `crates/app-core/src/lib.rs`, add imports:

```rust
use snapline_domain::{AssetId, AssetRef, AssetUploadPayload, Note, NoteChangePayload, NoteId, NoteSummary, SyncOpType, SyncPayload};
use sha2::{Digest, Sha256};
```

Add `sha2.workspace = true` to `crates/app-core/Cargo.toml`, and add `sha2 = "0.10"` to workspace dependencies in `Cargo.toml`.

Update `save_note`:

```rust
pub fn save_note(
    &self,
    id: &NoteId,
    title: &str,
    content_md: &str,
    pinned: bool,
) -> Result<Note> {
    let note = self.repo.save_note(id, title, content_md, pinned, Utc::now())?;
    let payload = SyncPayload::Note(NoteChangePayload::from_note(&note));
    self.repo.enqueue_change(
        &note.id,
        SyncOpType::UpsertNote,
        note.server_version,
        &payload,
        Utc::now(),
    )?;
    Ok(note)
}
```

- [ ] **Step 3: Enqueue pin and delete operations**

Update `set_note_pinned`:

```rust
pub fn set_note_pinned(&self, id: &NoteId, pinned: bool) -> Result<Note> {
    let note = self.repo.set_pinned(id, pinned, Utc::now())?;
    let payload = SyncPayload::Note(NoteChangePayload::from_note(&note));
    self.repo.enqueue_change(
        &note.id,
        SyncOpType::UpsertNote,
        note.server_version,
        &payload,
        Utc::now(),
    )?;
    Ok(note)
}
```

Update `delete_note`:

```rust
pub fn delete_note(&self, id: &NoteId) -> Result<Vec<NoteSummary>> {
    let existing = self.repo.get_note(id)?;
    self.repo.soft_delete(id, Utc::now())?;
    let deleted = self.repo.get_note(id)?;
    let payload = SyncPayload::Note(NoteChangePayload::from_note(&deleted));
    self.repo.enqueue_change(
        id,
        SyncOpType::DeleteNote,
        existing.server_version,
        &payload,
        Utc::now(),
    )?;
    self.repo.list_recent(50)
}
```

- [ ] **Step 4: Enqueue asset uploads**

Update `save_png_asset` after writing bytes:

```rust
let sha256 = {
    let mut hasher = Sha256::new();
    hasher.update(png_bytes);
    format!("{:x}", hasher.finalize())
};
let payload = SyncPayload::Asset(AssetUploadPayload {
    asset_id: asset_id.clone(),
    note_id: note_id.clone(),
    content_type: "image/png".to_string(),
    byte_size: png_bytes.len() as i64,
    sha256,
    markdown_path: markdown_path.clone(),
});
self.repo.enqueue_change(
    note_id,
    SyncOpType::AssetUpload,
    0,
    &payload,
    Utc::now(),
)?;
```

- [ ] **Step 5: Expose pending changes for tests and future sync client**

Add to `AppCore`:

```rust
pub fn pending_sync_changes(&self) -> Result<Vec<snapline_storage::ChangeQueueItem>> {
    self.repo.list_pending_changes(100)
}

pub fn sync_state(&self) -> Result<snapline_storage::SyncState> {
    self.repo.get_or_create_sync_state()
}
```

- [ ] **Step 6: Add app-core queue tests**

Append tests to `crates/app-core/src/lib.rs`:

```rust
#[test]
fn save_note_enqueues_upsert_change() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_data_dir(dir.path());
    let repo = NoteRepository::open_in_memory().unwrap();
    let core = AppCore::with_repo(paths, repo);
    let note = core.create_note().unwrap();

    core.save_note(&note.id, "Title", "# Title", false).unwrap();

    let changes = core.pending_sync_changes().unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].op_type, snapline_domain::SyncOpType::UpsertNote);
}

#[test]
fn save_png_asset_enqueues_asset_upload() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_data_dir(dir.path());
    let repo = NoteRepository::open_in_memory().unwrap();
    let core = AppCore::with_repo(paths, repo);
    let note = core.create_note().unwrap();

    core.save_png_asset(&note.id, &[137, 80, 78, 71]).unwrap();

    let changes = core.pending_sync_changes().unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].op_type, snapline_domain::SyncOpType::AssetUpload);
}
```

- [ ] **Step 7: Run app-core tests**

Run:

```powershell
cargo test -p snapline-app-core
```

Expected: all app-core tests pass.

- [ ] **Step 8: Commit**

Run:

```powershell
git add Cargo.toml crates/app-core crates/storage
git commit -m "feat: enqueue local sync changes"
```

## Task 4: Sync Client Protocol And Mock Processor

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/sync-client/Cargo.toml`
- Create: `crates/sync-client/src/lib.rs`
- Create: `crates/sync-client/src/protocol.rs`
- Create: `crates/sync-client/src/mock.rs`
- Test: `crates/sync-client/src/mock.rs`

- [ ] **Step 1: Add workspace member and dependencies**

Edit root `Cargo.toml`:

```toml
members = [
  "crates/domain",
  "crates/platform",
  "crates/storage",
  "crates/app-core",
  "crates/sync-client",
  "apps/desktop-tauri/src-tauri"
]

[workspace.dependencies]
async-trait = "0.1"
reqwest = { version = "0.12", features = ["json", "multipart"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread", "time"] }
```

- [ ] **Step 2: Create sync-client manifest**

Create `crates/sync-client/Cargo.toml`:

```toml
[package]
name = "snapline-sync-client"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
anyhow.workspace = true
async-trait.workspace = true
chrono.workspace = true
reqwest.workspace = true
serde.workspace = true
serde_json.workspace = true
snapline-domain = { path = "../domain" }
snapline-storage = { path = "../storage" }
tokio.workspace = true
```

- [ ] **Step 3: Add protocol DTOs**

Create `crates/sync-client/src/protocol.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use snapline_domain::{AssetMetadata, Note, NoteId, SyncPayload};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
    pub device_id: String,
    pub device_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    pub account_id: String,
    pub access_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushRequest {
    pub device_id: String,
    pub changes: Vec<PushChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushChange {
    pub queue_id: String,
    pub note_id: NoteId,
    pub base_version: i64,
    pub payload: SyncPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PushChangeResult {
    Accepted {
        queue_id: String,
        note_id: NoteId,
        server_version: i64,
        cursor: i64,
    },
    Conflict {
        queue_id: String,
        note_id: NoteId,
        server_note: Note,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushResponse {
    pub results: Vec<PushChangeResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullResponse {
    pub cursor: i64,
    pub changes: Vec<RemoteChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteChange {
    pub cursor: i64,
    pub device_id: String,
    pub note: Note,
    pub changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotResponse {
    pub cursor: i64,
    pub notes: Vec<Note>,
    pub assets: Vec<AssetMetadata>,
}
```

- [ ] **Step 4: Add SyncApi trait and HTTP shell**

Create `crates/sync-client/src/lib.rs`:

```rust
pub mod mock;
pub mod protocol;

use anyhow::Result;
use async_trait::async_trait;
use protocol::{LoginRequest, LoginResponse, PullResponse, PushRequest, PushResponse, SnapshotResponse};

#[async_trait]
pub trait SyncApi {
    async fn login(&self, request: LoginRequest) -> Result<LoginResponse>;
    async fn push(&self, token: &str, request: PushRequest) -> Result<PushResponse>;
    async fn pull(&self, token: &str, cursor: i64) -> Result<PullResponse>;
    async fn snapshot(&self, token: &str) -> Result<SnapshotResponse>;
}

pub struct HttpSyncApi {
    base_url: String,
    client: reqwest::Client,
}

impl HttpSyncApi {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl SyncApi for HttpSyncApi {
    async fn login(&self, request: LoginRequest) -> Result<LoginResponse> {
        Ok(self
            .client
            .post(format!("{}/auth/login", self.base_url))
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    async fn push(&self, token: &str, request: PushRequest) -> Result<PushResponse> {
        Ok(self
            .client
            .post(format!("{}/sync/push", self.base_url))
            .bearer_auth(token)
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    async fn pull(&self, token: &str, cursor: i64) -> Result<PullResponse> {
        Ok(self
            .client
            .get(format!("{}/sync/pull", self.base_url))
            .bearer_auth(token)
            .query(&[("cursor", cursor)])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    async fn snapshot(&self, token: &str) -> Result<SnapshotResponse> {
        Ok(self
            .client
            .get(format!("{}/sync/snapshot", self.base_url))
            .bearer_auth(token)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }
}
```

- [ ] **Step 5: Add mock sync API**

Create `crates/sync-client/src/mock.rs`:

```rust
use crate::protocol::*;
use crate::SyncApi;
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use snapline_domain::{Note, SyncPayload};
use std::sync::Mutex;

#[derive(Default)]
pub struct MockSyncApi {
    notes: Mutex<Vec<Note>>,
    cursor: Mutex<i64>,
}

#[async_trait]
impl SyncApi for MockSyncApi {
    async fn login(&self, request: LoginRequest) -> Result<LoginResponse> {
        Ok(LoginResponse {
            account_id: format!("acct_{}", request.email),
            access_token: "mock-token".to_string(),
        })
    }

    async fn push(&self, _token: &str, request: PushRequest) -> Result<PushResponse> {
        let mut notes = self.notes.lock().unwrap();
        let mut cursor = self.cursor.lock().unwrap();
        let mut results = Vec::new();
        for change in request.changes {
            if let SyncPayload::Note(payload) = change.payload {
                if let Some(existing) = notes.iter_mut().find(|note| note.id == change.note_id) {
                    if existing.server_version != change.base_version {
                        results.push(PushChangeResult::Conflict {
                            queue_id: change.queue_id,
                            note_id: change.note_id,
                            server_note: existing.clone(),
                        });
                        continue;
                    }
                    existing.title = payload.title;
                    existing.content_md = payload.content_md;
                    existing.pinned = payload.pinned;
                    existing.deleted_at = payload.deleted_at;
                    existing.server_version += 1;
                    *cursor += 1;
                    results.push(PushChangeResult::Accepted {
                        queue_id: change.queue_id,
                        note_id: existing.id.clone(),
                        server_version: existing.server_version,
                        cursor: *cursor,
                    });
                } else {
                    let mut note = Note::draft(Utc::now());
                    note.id = change.note_id.clone();
                    note.title = payload.title;
                    note.content_md = payload.content_md;
                    note.pinned = payload.pinned;
                    note.deleted_at = payload.deleted_at;
                    note.server_version = 1;
                    notes.push(note);
                    *cursor += 1;
                    results.push(PushChangeResult::Accepted {
                        queue_id: change.queue_id,
                        note_id: change.note_id,
                        server_version: 1,
                        cursor: *cursor,
                    });
                }
            }
        }
        Ok(PushResponse { results })
    }

    async fn pull(&self, _token: &str, _cursor: i64) -> Result<PullResponse> {
        let notes = self.notes.lock().unwrap();
        let cursor = *self.cursor.lock().unwrap();
        Ok(PullResponse {
            cursor,
            changes: notes
                .iter()
                .cloned()
                .map(|note| RemoteChange {
                    cursor,
                    device_id: "mock-device".to_string(),
                    note,
                    changed_at: Utc::now(),
                })
                .collect(),
        })
    }

    async fn snapshot(&self, _token: &str) -> Result<SnapshotResponse> {
        Ok(SnapshotResponse {
            cursor: *self.cursor.lock().unwrap(),
            notes: self.notes.lock().unwrap().clone(),
            assets: Vec::new(),
        })
    }
}
```

- [ ] **Step 6: Add mock tests**

Append to `crates/sync-client/src/mock.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use snapline_domain::{NoteChangePayload, NoteId};

    #[tokio::test]
    async fn mock_accepts_first_note_push() {
        let api = MockSyncApi::default();
        let note = Note::draft(Utc::now());
        let response = api
            .push(
                "token",
                PushRequest {
                    device_id: "device-a".to_string(),
                    changes: vec![PushChange {
                        queue_id: "q1".to_string(),
                        note_id: note.id.clone(),
                        base_version: 0,
                        payload: SyncPayload::Note(NoteChangePayload::from_note(&note)),
                    }],
                },
            )
            .await
            .unwrap();

        assert!(matches!(response.results[0], PushChangeResult::Accepted { server_version: 1, .. }));
    }

    #[tokio::test]
    async fn mock_reports_version_conflict() {
        let api = MockSyncApi::default();
        let note = Note::draft(Utc::now());
        let note_id: NoteId = note.id.clone();
        let payload = SyncPayload::Note(NoteChangePayload::from_note(&note));
        api.push("token", PushRequest {
            device_id: "device-a".to_string(),
            changes: vec![PushChange {
                queue_id: "q1".to_string(),
                note_id: note_id.clone(),
                base_version: 0,
                payload: payload.clone(),
            }],
        }).await.unwrap();

        let conflict = api.push("token", PushRequest {
            device_id: "device-b".to_string(),
            changes: vec![PushChange {
                queue_id: "q2".to_string(),
                note_id,
                base_version: 0,
                payload,
            }],
        }).await.unwrap();

        assert!(matches!(conflict.results[0], PushChangeResult::Conflict { .. }));
    }
}
```

- [ ] **Step 7: Run sync-client tests**

Run:

```powershell
cargo test -p snapline-sync-client
```

Expected: all sync-client tests pass.

- [ ] **Step 8: Commit**

Run:

```powershell
git add Cargo.toml crates/sync-client
git commit -m "feat: add sync client protocol"
```

## Task 5: Sync Server Skeleton, Config, And Auth

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/sync-server/Cargo.toml`
- Create: `crates/sync-server/src/main.rs`
- Create: `crates/sync-server/src/config.rs`
- Create: `crates/sync-server/src/auth.rs`
- Create: `crates/sync-server/src/db.rs`
- Create: `crates/sync-server/migrations/0001_init.sql`
- Test: `crates/sync-server/src/auth.rs`

- [ ] **Step 1: Add server workspace member and dependencies**

Edit root `Cargo.toml`:

```toml
members = [
  "crates/domain",
  "crates/platform",
  "crates/storage",
  "crates/app-core",
  "crates/sync-client",
  "crates/sync-server",
  "apps/desktop-tauri/src-tauri"
]

[workspace.dependencies]
argon2 = "0.5"
axum = "0.7"
jsonwebtoken = "9"
sqlx = { version = "0.8", features = ["runtime-tokio", "tls-rustls", "postgres", "uuid", "chrono", "json"] }
tower-http = { version = "0.6", features = ["trace", "cors"] }
```

- [ ] **Step 2: Create server manifest**

Create `crates/sync-server/Cargo.toml`:

```toml
[package]
name = "snapline-sync-server"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
anyhow.workspace = true
argon2.workspace = true
axum.workspace = true
chrono.workspace = true
jsonwebtoken.workspace = true
serde.workspace = true
serde_json.workspace = true
snapline-domain = { path = "../domain" }
snapline-sync-client = { path = "../sync-client" }
sqlx.workspace = true
tokio.workspace = true
tower-http.workspace = true
uuid.workspace = true
```

- [ ] **Step 3: Add PostgreSQL migration**

Create `crates/sync-server/migrations/0001_init.sql`:

```sql
CREATE TABLE IF NOT EXISTS accounts (
  id TEXT PRIMARY KEY,
  email TEXT NOT NULL UNIQUE,
  password_hash TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  disabled_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS devices (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS notes (
  id TEXT NOT NULL,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  title TEXT NOT NULL,
  content_md TEXT NOT NULL,
  pinned BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL,
  deleted_at TIMESTAMPTZ,
  version BIGINT NOT NULL DEFAULT 1,
  last_modified_by_device TEXT NOT NULL,
  PRIMARY KEY (account_id, id)
);

CREATE TABLE IF NOT EXISTS change_log (
  cursor BIGSERIAL PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  note_id TEXT NOT NULL,
  op_type TEXT NOT NULL,
  note_version BIGINT NOT NULL,
  payload_json JSONB NOT NULL,
  device_id TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS assets (
  id TEXT NOT NULL,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  note_id TEXT NOT NULL,
  content_type TEXT NOT NULL,
  byte_size BIGINT NOT NULL,
  sha256 TEXT NOT NULL,
  storage_key TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  deleted_at TIMESTAMPTZ,
  PRIMARY KEY (account_id, id)
);
```

- [ ] **Step 4: Add config loader**

Create `crates/sync-server/src/config.rs`:

```rust
use anyhow::{Context, Result};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub jwt_secret: String,
    pub asset_data_dir: PathBuf,
    pub public_base_url: String,
    pub allow_registration: bool,
    pub bootstrap_admin_email: Option<String>,
    pub bootstrap_admin_password: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            database_url: std::env::var("DATABASE_URL").context("DATABASE_URL is required")?,
            jwt_secret: std::env::var("JWT_SECRET").context("JWT_SECRET is required")?,
            asset_data_dir: std::env::var("ASSET_DATA_DIR")
                .context("ASSET_DATA_DIR is required")?
                .into(),
            public_base_url: std::env::var("PUBLIC_BASE_URL").context("PUBLIC_BASE_URL is required")?,
            allow_registration: std::env::var("ALLOW_REGISTRATION")
                .unwrap_or_else(|_| "true".to_string())
                == "true",
            bootstrap_admin_email: std::env::var("SNAPLINE_BOOTSTRAP_ADMIN_EMAIL").ok(),
            bootstrap_admin_password: std::env::var("SNAPLINE_BOOTSTRAP_ADMIN_PASSWORD").ok(),
        })
    }
}
```

- [ ] **Step 5: Add auth helpers**

Create `crates/sync-server/src/auth.rs`:

```rust
use anyhow::Result;
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
}

pub fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(Argon2::default()
        .hash_password(password.as_bytes(), &salt)?
        .to_string())
}

pub fn verify_password(password: &str, hash: &str) -> Result<bool> {
    let parsed = PasswordHash::new(hash)?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

pub fn issue_token(account_id: &str, secret: &str) -> Result<String> {
    let claims = Claims {
        sub: account_id.to_string(),
        exp: (Utc::now() + Duration::days(30)).timestamp() as usize,
    };
    Ok(encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?)
}

pub fn verify_token(token: &str, secret: &str) -> Result<Claims> {
    Ok(decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )?
    .claims)
}
```

- [ ] **Step 6: Add auth tests**

Append to `crates/sync-server/src/auth.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_hash_verifies_original_password() {
        let hash = hash_password("secret-password").unwrap();
        assert!(verify_password("secret-password", &hash).unwrap());
        assert!(!verify_password("wrong", &hash).unwrap());
    }

    #[test]
    fn jwt_round_trips_account_id() {
        let token = issue_token("acct_1", "test-secret").unwrap();
        let claims = verify_token(&token, "test-secret").unwrap();
        assert_eq!(claims.sub, "acct_1");
    }
}
```

- [ ] **Step 7: Add DB module and main skeleton**

Create `crates/sync-server/src/db.rs`:

```rust
use anyhow::Result;
use sqlx::{postgres::PgPoolOptions, PgPool};

pub async fn connect(database_url: &str) -> Result<PgPool> {
    Ok(PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?)
}
```

Create `crates/sync-server/src/main.rs`:

```rust
mod auth;
mod config;
mod db;

use anyhow::Result;
use axum::{routing::get, Router};
use config::Config;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::from_env()?;
    let pool = db::connect(&config.database_url).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;

    let app = Router::new().route("/health", get(|| async { "ok" }));
    let addr: SocketAddr = "0.0.0.0:8080".parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
```

- [ ] **Step 8: Run server unit tests**

Run:

```powershell
cargo test -p snapline-sync-server
```

Expected: auth tests pass.

- [ ] **Step 9: Commit**

Run:

```powershell
git add Cargo.toml crates/sync-server
git commit -m "feat: add sync server skeleton"
```

## Task 6: Server Routes For Register, Login, Push, Pull, Snapshot

**Files:**
- Modify: `crates/sync-server/src/main.rs`
- Create: `crates/sync-server/src/routes.rs`
- Create: `crates/sync-server/src/sync_service.rs`
- Test: `crates/sync-server/src/sync_service.rs`

- [ ] **Step 1: Add shared app state and auth extractor**

Create `crates/sync-server/src/routes.rs`:

```rust
use crate::{auth, config::Config};
use axum::{
    extract::{FromRequestParts, State},
    http::{request::Parts, StatusCode},
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Config,
}

#[derive(Debug, Clone)]
pub struct AuthAccount {
    pub account_id: String,
}

#[axum::async_trait]
impl FromRequestParts<Arc<AppState>> for AuthAccount {
    type Rejection = (StatusCode, String);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .ok_or((StatusCode::UNAUTHORIZED, "missing authorization".to_string()))?;
        let token = header
            .strip_prefix("Bearer ")
            .ok_or((StatusCode::UNAUTHORIZED, "invalid authorization".to_string()))?;
        let claims = auth::verify_token(token, &state.config.jwt_secret)
            .map_err(|_| (StatusCode::UNAUTHORIZED, "invalid token".to_string()))?;
        Ok(Self {
            account_id: claims.sub,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub device_id: String,
    pub device_name: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub account_id: String,
    pub access_token: String,
}
```

- [ ] **Step 2: Add auth route handlers**

Append to `routes.rs`:

```rust
use axum::{extract::Json, response::IntoResponse};
use uuid::Uuid;

pub async fn register(
    State(state): State<Arc<AppState>>,
    Json(request): Json<RegisterRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    if !state.config.allow_registration {
        return Err((StatusCode::FORBIDDEN, "registration is disabled".to_string()));
    }
    create_account_and_device(state, request).await
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(request): Json<RegisterRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let row = sqlx::query!(
        "SELECT id, password_hash FROM accounts WHERE email = $1 AND disabled_at IS NULL",
        request.email
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_error)?
    .ok_or((StatusCode::UNAUTHORIZED, "invalid credentials".to_string()))?;
    if !auth::verify_password(&request.password, &row.password_hash).map_err(internal_error)? {
        return Err((StatusCode::UNAUTHORIZED, "invalid credentials".to_string()));
    }
    sqlx::query!(
        "INSERT INTO devices (id, account_id, name) VALUES ($1, $2, $3)
         ON CONFLICT(id) DO UPDATE SET last_seen_at = now(), name = excluded.name",
        request.device_id,
        row.id,
        request.device_name
    )
    .execute(&state.pool)
    .await
    .map_err(internal_error)?;
    let token = auth::issue_token(&row.id, &state.config.jwt_secret).map_err(internal_error)?;
    Ok(Json(AuthResponse {
        account_id: row.id,
        access_token: token,
    }))
}

async fn create_account_and_device(
    state: Arc<AppState>,
    request: RegisterRequest,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let account_id = Uuid::new_v4().to_string();
    let password_hash = auth::hash_password(&request.password).map_err(internal_error)?;
    let mut tx = state.pool.begin().await.map_err(internal_error)?;
    sqlx::query!(
        "INSERT INTO accounts (id, email, password_hash) VALUES ($1, $2, $3)",
        account_id,
        request.email,
        password_hash
    )
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;
    sqlx::query!(
        "INSERT INTO devices (id, account_id, name) VALUES ($1, $2, $3)",
        request.device_id,
        account_id,
        request.device_name
    )
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;
    tx.commit().await.map_err(internal_error)?;
    let token = auth::issue_token(&account_id, &state.config.jwt_secret).map_err(internal_error)?;
    Ok(Json(AuthResponse {
        account_id,
        access_token: token,
    }))
}

fn internal_error(err: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}
```

- [ ] **Step 3: Add sync service skeleton**

Create `crates/sync-server/src/sync_service.rs`:

```rust
use anyhow::Result;
use snapline_domain::{Note, NoteChangePayload, SyncPayload};
use snapline_sync_client::protocol::{PushChange, PushChangeResult, RemoteChange};
use sqlx::{PgPool, Postgres, Transaction};

pub async fn apply_push_change(
    tx: &mut Transaction<'_, Postgres>,
    account_id: &str,
    device_id: &str,
    change: PushChange,
) -> Result<PushChangeResult> {
    let existing = sqlx::query!(
        "SELECT title, content_md, pinned, created_at, updated_at, deleted_at, version, last_modified_by_device
         FROM notes WHERE account_id = $1 AND id = $2",
        account_id,
        change.note_id.to_string()
    )
    .fetch_optional(&mut **tx)
    .await?;

    if let Some(existing) = existing {
        if existing.version != change.base_version {
            let server_note = Note {
                id: change.note_id.clone(),
                title: existing.title,
                content_md: existing.content_md,
                pinned: existing.pinned,
                created_at: existing.created_at,
                updated_at: existing.updated_at,
                deleted_at: existing.deleted_at,
                server_version: existing.version,
                last_modified_by_device: Some(existing.last_modified_by_device),
                is_conflict_copy: false,
                source_note_id: None,
            };
            return Ok(PushChangeResult::Conflict {
                queue_id: change.queue_id,
                note_id: change.note_id,
                server_note,
            });
        }
    }

    let payload = match change.payload {
        SyncPayload::Note(payload) => payload,
        SyncPayload::Asset(_) => {
            return Ok(PushChangeResult::Accepted {
                queue_id: change.queue_id,
                note_id: change.note_id,
                server_version: change.base_version,
                cursor: 0,
            })
        }
    };

    let next_version = existing.as_ref().map(|row| row.version + 1).unwrap_or(1);
    upsert_note(tx, account_id, device_id, &change.note_id.to_string(), next_version, &payload).await?;
    let cursor = append_change_log(tx, account_id, device_id, &change.note_id.to_string(), next_version, &payload).await?;
    Ok(PushChangeResult::Accepted {
        queue_id: change.queue_id,
        note_id: change.note_id,
        server_version: next_version,
        cursor,
    })
}

async fn upsert_note(
    tx: &mut Transaction<'_, Postgres>,
    account_id: &str,
    device_id: &str,
    note_id: &str,
    version: i64,
    payload: &NoteChangePayload,
) -> Result<()> {
    sqlx::query!(
        "INSERT INTO notes
         (id, account_id, title, content_md, pinned, created_at, updated_at, deleted_at, version, last_modified_by_device)
         VALUES ($1, $2, $3, $4, $5, now(), now(), $6, $7, $8)
         ON CONFLICT(account_id, id) DO UPDATE SET
           title = excluded.title,
           content_md = excluded.content_md,
           pinned = excluded.pinned,
           updated_at = now(),
           deleted_at = excluded.deleted_at,
           version = excluded.version,
           last_modified_by_device = excluded.last_modified_by_device",
        note_id,
        account_id,
        payload.title,
        payload.content_md,
        payload.pinned,
        payload.deleted_at,
        version,
        device_id
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn append_change_log(
    tx: &mut Transaction<'_, Postgres>,
    account_id: &str,
    device_id: &str,
    note_id: &str,
    version: i64,
    payload: &NoteChangePayload,
) -> Result<i64> {
    let payload_json = serde_json::to_value(payload)?;
    let row = sqlx::query!(
        "INSERT INTO change_log (account_id, note_id, op_type, note_version, payload_json, device_id)
         VALUES ($1, $2, 'upsert_note', $3, $4, $5)
         RETURNING cursor",
        account_id,
        note_id,
        version,
        payload_json,
        device_id
    )
    .fetch_one(&mut **tx)
    .await?;
    Ok(row.cursor)
}
```

- [ ] **Step 4: Add push, pull, and snapshot routes**

Append to `routes.rs`:

```rust
use crate::sync_service;
use axum::extract::Query;
use snapline_sync_client::protocol::{PullResponse, PushRequest, PushResponse, RemoteChange, SnapshotResponse};

#[derive(Debug, Deserialize)]
pub struct PullQuery {
    pub cursor: i64,
}

pub async fn push(
    State(state): State<Arc<AppState>>,
    account: AuthAccount,
    Json(request): Json<PushRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let mut tx = state.pool.begin().await.map_err(internal_error)?;
    let mut results = Vec::new();
    for change in request.changes {
        results.push(
            sync_service::apply_push_change(&mut tx, &account.account_id, &request.device_id, change)
                .await
                .map_err(internal_error)?,
        );
    }
    tx.commit().await.map_err(internal_error)?;
    Ok(Json(PushResponse { results }))
}

pub async fn pull(
    State(state): State<Arc<AppState>>,
    account: AuthAccount,
    Query(query): Query<PullQuery>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let rows = sqlx::query!(
        "SELECT c.cursor, c.device_id, c.created_at, n.id, n.title, n.content_md, n.pinned,
                n.created_at AS note_created_at, n.updated_at, n.deleted_at, n.version, n.last_modified_by_device
         FROM change_log c
         JOIN notes n ON n.account_id = c.account_id AND n.id = c.note_id
         WHERE c.account_id = $1 AND c.cursor > $2
         ORDER BY c.cursor ASC",
        account.account_id,
        query.cursor
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;
    let latest_cursor = rows.last().map(|row| row.cursor).unwrap_or(query.cursor);
    let changes = rows
        .into_iter()
        .map(|row| RemoteChange {
            cursor: row.cursor,
            device_id: row.device_id,
            changed_at: row.created_at,
            note: snapline_domain::Note {
                id: snapline_domain::NoteId(uuid::Uuid::parse_str(&row.id).unwrap()),
                title: row.title,
                content_md: row.content_md,
                pinned: row.pinned,
                created_at: row.note_created_at,
                updated_at: row.updated_at,
                deleted_at: row.deleted_at,
                server_version: row.version,
                last_modified_by_device: Some(row.last_modified_by_device),
                is_conflict_copy: false,
                source_note_id: None,
            },
        })
        .collect();
    Ok(Json(PullResponse {
        cursor: latest_cursor,
        changes,
    }))
}

pub async fn snapshot(
    State(state): State<Arc<AppState>>,
    account: AuthAccount,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let note_rows = sqlx::query!(
        "SELECT id, title, content_md, pinned, created_at, updated_at, deleted_at, version, last_modified_by_device
         FROM notes WHERE account_id = $1",
        account.account_id
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;
    let cursor = sqlx::query_scalar!(
        "SELECT COALESCE(MAX(cursor), 0) FROM change_log WHERE account_id = $1",
        account.account_id
    )
    .fetch_one(&state.pool)
    .await
    .map_err(internal_error)?
    .unwrap_or(0);
    let notes = note_rows
        .into_iter()
        .map(|row| snapline_domain::Note {
            id: snapline_domain::NoteId(uuid::Uuid::parse_str(&row.id).unwrap()),
            title: row.title,
            content_md: row.content_md,
            pinned: row.pinned,
            created_at: row.created_at,
            updated_at: row.updated_at,
            deleted_at: row.deleted_at,
            server_version: row.version,
            last_modified_by_device: Some(row.last_modified_by_device),
            is_conflict_copy: false,
            source_note_id: None,
        })
        .collect();
    Ok(Json(SnapshotResponse {
        cursor,
        notes,
        assets: Vec::new(),
    }))
}
```

- [ ] **Step 5: Wire routes in main**

Update `crates/sync-server/src/main.rs`:

```rust
mod auth;
mod config;
mod db;
mod routes;
mod sync_service;

use anyhow::Result;
use axum::{routing::{get, post}, Router};
use config::Config;
use routes::AppState;
use std::{net::SocketAddr, sync::Arc};

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::from_env()?;
    let pool = db::connect(&config.database_url).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    let state = Arc::new(AppState { pool, config });

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/auth/register", post(routes::register))
        .route("/auth/login", post(routes::login))
        .route("/sync/push", post(routes::push))
        .route("/sync/pull", get(routes::pull))
        .route("/sync/snapshot", get(routes::snapshot))
        .with_state(state);

    let addr: SocketAddr = "0.0.0.0:8080".parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
```

- [ ] **Step 6: Compile server**

Run:

```powershell
cargo check -p snapline-sync-server
```

Expected: server compiles. If SQLx requires a live database for macros, set up `DATABASE_URL` and run `cargo sqlx prepare`, or replace `query!` with runtime-checked `sqlx::query` in this task.

- [ ] **Step 7: Commit**

Run:

```powershell
git add crates/sync-server
git commit -m "feat: add sync server routes"
```

## Task 7: Server Asset Upload And Download

**Files:**
- Modify: `crates/sync-server/src/main.rs`
- Modify: `crates/sync-server/src/routes.rs`
- Create: `crates/sync-server/src/assets.rs`
- Test: `crates/sync-server/src/assets.rs`

- [ ] **Step 1: Add bytes dependency**

Add to root `Cargo.toml`:

```toml
[workspace.dependencies]
bytes = "1"
```

In `crates/sync-server/Cargo.toml`, replace `axum.workspace = true` with:

```toml
bytes.workspace = true
axum = { workspace = true, features = ["multipart"] }
```

- [ ] **Step 2: Add asset store**

Create `crates/sync-server/src/assets.rs`:

```rust
use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use std::path::{Path, PathBuf};

#[async_trait]
pub trait AssetStore: Send + Sync {
    async fn put(&self, key: &str, bytes: Bytes) -> Result<()>;
    async fn get(&self, key: &str) -> Result<Bytes>;
    async fn delete(&self, key: &str) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct LocalFsAssetStore {
    root: PathBuf,
}

impl LocalFsAssetStore {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    fn resolve(&self, key: &str) -> PathBuf {
        self.root.join(key.replace('\\', "/"))
    }
}

#[async_trait]
impl AssetStore for LocalFsAssetStore {
    async fn put(&self, key: &str, bytes: Bytes) -> Result<()> {
        let path = self.resolve(key);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(path, bytes).await?;
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Bytes> {
        Ok(Bytes::from(tokio::fs::read(self.resolve(key)).await?))
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let path = self.resolve(key);
        if tokio::fs::try_exists(&path).await? {
            tokio::fs::remove_file(path).await?;
        }
        Ok(())
    }
}
```

- [ ] **Step 3: Add asset store tests**

Append to `assets.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_store_puts_and_gets_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let store = LocalFsAssetStore::new(dir.path());
        store
            .put("accounts/a/notes/n/image.png", Bytes::from_static(b"png"))
            .await
            .unwrap();

        let loaded = store.get("accounts/a/notes/n/image.png").await.unwrap();
        assert_eq!(loaded, Bytes::from_static(b"png"));
    }
}
```

Add `tempfile.workspace = true` to `crates/sync-server` dev-dependencies.

- [ ] **Step 4: Extend AppState with asset store**

In `routes.rs`, update `AppState`:

```rust
use crate::assets::LocalFsAssetStore;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Config,
    pub asset_store: LocalFsAssetStore,
}
```

In `main.rs`, add `mod assets;` and construct:

```rust
let asset_store = assets::LocalFsAssetStore::new(&config.asset_data_dir);
let state = Arc::new(AppState { pool, config, asset_store });
```

- [ ] **Step 5: Add upload and download route DTOs**

Append to `routes.rs`:

```rust
use axum::extract::{Multipart, Path};
use snapline_domain::AssetUploadPayload;

pub async fn upload_asset(
    State(state): State<Arc<AppState>>,
    account: AuthAccount,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let mut payload: Option<AssetUploadPayload> = None;
    let mut file_bytes: Option<bytes::Bytes> = None;

    while let Some(field) = multipart.next_field().await.map_err(internal_error)? {
        match field.name() {
            Some("metadata") => {
                let text = field.text().await.map_err(internal_error)?;
                payload = Some(serde_json::from_str(&text).map_err(internal_error)?);
            }
            Some("file") => {
                file_bytes = Some(field.bytes().await.map_err(internal_error)?);
            }
            _ => {}
        }
    }

    let payload = payload.ok_or((StatusCode::BAD_REQUEST, "missing metadata".to_string()))?;
    let file_bytes = file_bytes.ok_or((StatusCode::BAD_REQUEST, "missing file".to_string()))?;
    if file_bytes.len() as i64 != payload.byte_size {
        return Err((StatusCode::BAD_REQUEST, "asset byte size mismatch".to_string()));
    }
    let storage_key = format!(
        "accounts/{}/notes/{}/{}.png",
        account.account_id, payload.note_id, payload.asset_id
    );
    state.asset_store.put(&storage_key, file_bytes).await.map_err(internal_error)?;
    sqlx::query!(
        "INSERT INTO assets (id, account_id, note_id, content_type, byte_size, sha256, storage_key)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT(account_id, id) DO UPDATE SET
           note_id = excluded.note_id,
           content_type = excluded.content_type,
           byte_size = excluded.byte_size,
           sha256 = excluded.sha256,
           storage_key = excluded.storage_key",
        payload.asset_id.to_string(),
        account.account_id,
        payload.note_id.to_string(),
        payload.content_type,
        payload.byte_size,
        payload.sha256,
        storage_key
    )
    .execute(&state.pool)
    .await
    .map_err(internal_error)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn download_asset(
    State(state): State<Arc<AppState>>,
    account: AuthAccount,
    Path(asset_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let row = sqlx::query!(
        "SELECT content_type, storage_key FROM assets WHERE account_id = $1 AND id = $2 AND deleted_at IS NULL",
        account.account_id,
        asset_id
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_error)?
    .ok_or((StatusCode::NOT_FOUND, "asset not found".to_string()))?;
    let bytes = state.asset_store.get(&row.storage_key).await.map_err(internal_error)?;
    Ok(([(axum::http::header::CONTENT_TYPE, row.content_type)], bytes))
}
```

- [ ] **Step 6: Wire asset routes**

In `main.rs`, add:

```rust
.route("/sync/assets/upload", post(routes::upload_asset))
.route("/sync/assets/:asset_id/download", get(routes::download_asset))
```

- [ ] **Step 7: Run server checks**

Run:

```powershell
cargo test -p snapline-sync-server
```

Expected: asset store and auth tests pass.

- [ ] **Step 8: Commit**

Run:

```powershell
git add Cargo.toml crates/sync-server
git commit -m "feat: add sync asset storage"
```

## Task 8: Desktop Sync Commands And Settings UI

**Files:**
- Modify: `apps/desktop-tauri/src-tauri/Cargo.toml`
- Modify: `apps/desktop-tauri/src-tauri/src/main.rs`
- Modify: `apps/desktop-tauri/src/api.ts`
- Modify: `apps/desktop-tauri/src/types.ts`
- Create: `apps/desktop-tauri/src/SyncSettings.tsx`
- Modify: `apps/desktop-tauri/src/App.tsx`
- Modify: `apps/desktop-tauri/src/styles.css`
- Test: `apps/desktop-tauri/src/session.test.ts`

- [ ] **Step 1: Add sync-client dependency to Tauri**

Edit `apps/desktop-tauri/src-tauri/Cargo.toml`:

```toml
snapline-sync-client = { path = "../../../crates/sync-client" }
```

- [ ] **Step 2: Add app-core sync account methods**

In `crates/app-core/src/lib.rs`, add:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncAccountState {
    pub account_id: Option<String>,
    pub device_id: String,
    pub server_base_url: Option<String>,
    pub is_logged_in: bool,
}

pub fn sync_account_state(&self) -> Result<SyncAccountState> {
    let state = self.repo.get_or_create_sync_state()?;
    Ok(SyncAccountState {
        account_id: state.account_id,
        device_id: state.device_id,
        server_base_url: state.server_base_url,
        is_logged_in: state.access_token.is_some(),
    })
}

pub fn save_sync_login(&self, server_base_url: &str, account_id: &str, access_token: &str) -> Result<SyncAccountState> {
    let mut state = self.repo.get_or_create_sync_state()?;
    state.server_base_url = Some(server_base_url.to_string());
    state.account_id = Some(account_id.to_string());
    state.access_token = Some(access_token.to_string());
    self.repo.save_sync_state(&state)?;
    self.sync_account_state()
}
```

- [ ] **Step 3: Add Tauri sync commands**

In `main.rs`, import:

```rust
use snapline_app_core::SyncAccountState;
use snapline_sync_client::{protocol::LoginRequest, HttpSyncApi, SyncApi};
```

Add commands:

```rust
#[tauri::command]
fn get_sync_account_state(state: State<'_, AppState>) -> Result<SyncAccountState, String> {
    state.core.lock().map_err(|_| "app state lock poisoned".to_string())?
        .sync_account_state()
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn login_sync(
    state: State<'_, AppState>,
    server_base_url: String,
    email: String,
    password: String,
) -> Result<SyncAccountState, String> {
    let device_id = state.core.lock().map_err(|_| "app state lock poisoned".to_string())?
        .sync_state()
        .map_err(|err| err.to_string())?
        .device_id;
    let api = HttpSyncApi::new(&server_base_url);
    let response = api.login(LoginRequest {
        email,
        password,
        device_id,
        device_name: "Snapline Desktop".to_string(),
    }).await.map_err(|err| err.to_string())?;
    state.core.lock().map_err(|_| "app state lock poisoned".to_string())?
        .save_sync_login(&server_base_url, &response.account_id, &response.access_token)
        .map_err(|err| err.to_string())
}
```

Add both commands to `generate_handler!`.

- [ ] **Step 4: Add frontend types and API calls**

In `apps/desktop-tauri/src/types.ts`, add:

```ts
export interface SyncAccountState {
  account_id: string | null;
  device_id: string;
  server_base_url: string | null;
  is_logged_in: boolean;
}
```

In `api.ts`, add:

```ts
getSyncAccountState: () => invoke<SyncAccountState>("get_sync_account_state"),
loginSync: (serverBaseUrl: string, email: string, password: string) =>
  invoke<SyncAccountState>("login_sync", { serverBaseUrl, email, password }),
```

- [ ] **Step 5: Create SyncSettings component**

Create `apps/desktop-tauri/src/SyncSettings.tsx`:

```tsx
import { FormEvent, useState } from "react";
import { api } from "./api";
import type { SyncAccountState } from "./types";

interface SyncSettingsProps {
  initial: SyncAccountState | null;
  onSaved: (state: SyncAccountState) => void;
}

export function SyncSettings({ initial, onSaved }: SyncSettingsProps) {
  const [serverUrl, setServerUrl] = useState(initial?.server_base_url ?? "http://localhost:8080");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [status, setStatus] = useState(initial?.is_logged_in ? "Connected" : "Not connected");

  async function submit(event: FormEvent) {
    event.preventDefault();
    setStatus("Connecting");
    try {
      const next = await api.loginSync(serverUrl, email, password);
      onSaved(next);
      setStatus("Connected");
    } catch (err) {
      setStatus(String(err));
    }
  }

  return (
    <form className="syncSettings" onSubmit={submit}>
      <input value={serverUrl} onChange={(event) => setServerUrl(event.target.value)} aria-label="Sync server URL" />
      <input value={email} onChange={(event) => setEmail(event.target.value)} aria-label="Email" />
      <input value={password} onChange={(event) => setPassword(event.target.value)} aria-label="Password" type="password" />
      <button type="submit">Connect</button>
      <span>{status}</span>
    </form>
  );
}
```

- [ ] **Step 6: Wire settings into App**

In `App.tsx`, load account state with `api.getSyncAccountState()` on startup. Add state:

```ts
const [syncAccount, setSyncAccount] = useState<SyncAccountState | null>(null);
const [showSyncSettings, setShowSyncSettings] = useState(false);
```

Add to the bootstrap effect:

```ts
api.getSyncAccountState().then(setSyncAccount).catch((err) => console.error(err));
```

Add a topbar button:

```tsx
<button onClick={() => setShowSyncSettings((value) => !value)}>
  {syncAccount?.is_logged_in ? "Synced" : "Sync"}
</button>
```

Render:

```tsx
{showSyncSettings ? (
  <SyncSettings initial={syncAccount} onSaved={setSyncAccount} />
) : null}
```

- [ ] **Step 7: Add focused CSS**

Append to `styles.css`:

```css
.syncSettings {
  display: grid;
  grid-template-columns: minmax(180px, 1fr) minmax(140px, 180px) minmax(140px, 180px) auto auto;
  gap: 8px;
  align-items: center;
  padding: 10px 14px;
  border-bottom: 1px solid #d9dde3;
}

.syncSettings input {
  min-width: 0;
  height: 32px;
  border: 1px solid #c9ced6;
  border-radius: 6px;
  padding: 0 8px;
  font: inherit;
}
```

- [ ] **Step 8: Run frontend and Tauri checks**

Run:

```powershell
cargo check -p snapline-desktop
cd apps\desktop-tauri
npm test
npm run build
```

Expected: Rust checks, Vitest, and frontend build pass.

- [ ] **Step 9: Commit**

Run:

```powershell
git add crates/app-core apps/desktop-tauri
git commit -m "feat: add desktop sync login settings"
```

## Task 9: Background Sync Processor

**Files:**
- Modify: `crates/sync-client/src/lib.rs`
- Create: `crates/sync-client/src/processor.rs`
- Modify: `crates/app-core/src/lib.rs`
- Modify: `apps/desktop-tauri/src-tauri/src/main.rs`
- Test: `crates/sync-client/src/processor.rs`

- [ ] **Step 1: Add processor module**

In `crates/sync-client/src/lib.rs`, add:

```rust
pub mod processor;
```

- [ ] **Step 2: Create queue processor types**

Create `crates/sync-client/src/processor.rs`:

```rust
use crate::protocol::{PushChange, PushRequest, PushChangeResult};
use crate::SyncApi;
use anyhow::Result;
use snapline_storage::NoteRepository;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessReport {
    pub accepted: usize,
    pub conflicts: usize,
    pub failed: usize,
}

pub async fn push_pending_changes<A: SyncApi + Sync>(
    repo: &NoteRepository,
    api: &A,
    token: &str,
    device_id: &str,
) -> Result<ProcessReport> {
    let pending = repo.list_pending_changes(25)?;
    if pending.is_empty() {
        return Ok(ProcessReport {
            accepted: 0,
            conflicts: 0,
            failed: 0,
        });
    }
    let request = PushRequest {
        device_id: device_id.to_string(),
        changes: pending
            .iter()
            .map(|item| PushChange {
                queue_id: item.id.clone(),
                note_id: item.note_id.clone(),
                base_version: item.base_version,
                payload: item.payload.clone(),
            })
            .collect(),
    };
    let response = api.push(token, request).await?;
    let mut report = ProcessReport {
        accepted: 0,
        conflicts: 0,
        failed: 0,
    };
    for result in response.results {
        match result {
            PushChangeResult::Accepted { queue_id, .. } => {
                repo.delete_change(&queue_id)?;
                report.accepted += 1;
            }
            PushChangeResult::Conflict { queue_id, .. } => {
                repo.mark_change_failed(&queue_id, "version conflict")?;
                report.conflicts += 1;
            }
        }
    }
    Ok(report)
}
```

- [ ] **Step 3: Add processor test with mock API**

Append to `processor.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockSyncApi;
    use chrono::Utc;
    use snapline_domain::{Note, NoteChangePayload, SyncOpType, SyncPayload};

    #[tokio::test]
    async fn processor_deletes_accepted_queue_items() {
        let repo = NoteRepository::open_in_memory().unwrap();
        let note = Note::draft(Utc::now());
        let payload = SyncPayload::Note(NoteChangePayload::from_note(&note));
        repo.enqueue_change(&note.id, SyncOpType::UpsertNote, 0, &payload, Utc::now())
            .unwrap();

        let api = MockSyncApi::default();
        let report = push_pending_changes(&repo, &api, "token", "device-a").await.unwrap();

        assert_eq!(report.accepted, 1);
        assert!(repo.list_pending_changes(10).unwrap().is_empty());
    }
}
```

- [ ] **Step 4: Add app-core sync run method**

In `AppCore`, expose repository-backed sync state fields needed by Tauri:

```rust
pub fn sync_credentials(&self) -> Result<Option<(String, String, String)>> {
    let state = self.repo.get_or_create_sync_state()?;
    match (state.server_base_url, state.access_token) {
        (Some(base_url), Some(token)) => Ok(Some((base_url, token, state.device_id))),
        _ => Ok(None),
    }
}
```

- [ ] **Step 5: Add manual sync command**

In Tauri `main.rs`, add:

```rust
#[tauri::command]
async fn sync_now(state: State<'_, AppState>) -> Result<String, String> {
    let (base_url, token, device_id, pending) = {
        let core = state.core.lock().map_err(|_| "app state lock poisoned".to_string())?;
        let (base_url, token, device_id) = core
            .sync_credentials()
            .map_err(|err| err.to_string())?
            .ok_or_else(|| "not logged in".to_string())?;
        let pending = core.pending_sync_changes().map_err(|err| err.to_string())?;
        (base_url, token, device_id, pending)
    };
    if pending.is_empty() {
        return Ok("accepted=0, conflicts=0, failed=0".to_string());
    }
    let api = HttpSyncApi::new(base_url);
    let response = api
        .push(
            &token,
            snapline_sync_client::protocol::PushRequest {
                device_id,
                changes: pending
                    .iter()
                    .map(|item| snapline_sync_client::protocol::PushChange {
                        queue_id: item.id.clone(),
                        note_id: item.note_id.clone(),
                        base_version: item.base_version,
                        payload: item.payload.clone(),
                    })
                    .collect(),
            },
        )
        .await
        .map_err(|err| err.to_string())?;
    let mut accepted = 0;
    let mut conflicts = 0;
    {
        let core = state.core.lock().map_err(|_| "app state lock poisoned".to_string())?;
        for result in response.results {
            match result {
                snapline_sync_client::protocol::PushChangeResult::Accepted { queue_id, .. } => {
                    core.delete_sync_change(&queue_id).map_err(|err| err.to_string())?;
                    accepted += 1;
                }
                snapline_sync_client::protocol::PushChangeResult::Conflict { queue_id, .. } => {
                    core.mark_sync_change_failed(&queue_id, "version conflict")
                        .map_err(|err| err.to_string())?;
                    conflicts += 1;
                }
            }
        }
    }
    Ok(format!("accepted={accepted}, conflicts={conflicts}, failed=0"))
}
```

Add the two AppCore helpers used above:

```rust
pub fn delete_sync_change(&self, queue_id: &str) -> Result<()> {
    self.repo.delete_change(queue_id)
}

pub fn mark_sync_change_failed(&self, queue_id: &str, error: &str) -> Result<()> {
    self.repo.mark_change_failed(queue_id, error)
}
```

- [ ] **Step 6: Run checks**

Run:

```powershell
cargo test -p snapline-sync-client
cargo check -p snapline-desktop
```

Expected: sync processor test passes and desktop compiles.

- [ ] **Step 7: Commit**

Run:

```powershell
git add crates/sync-client crates/app-core apps/desktop-tauri/src-tauri
git commit -m "feat: process pending sync changes"
```

## Task 10: Docker Compose And Self-Hosting Docs

**Files:**
- Create: `docker-compose.sync.yml`
- Create: `docs/self-hosting.md`
- Modify: `.gitignore`

- [ ] **Step 1: Add Docker Compose file**

Create `docker-compose.sync.yml`:

```yaml
services:
  postgres:
    image: postgres:16
    environment:
      POSTGRES_DB: snapline
      POSTGRES_USER: snapline
      POSTGRES_PASSWORD: snapline-dev-password
    volumes:
      - snapline-postgres:/var/lib/postgresql/data
    ports:
      - "5432:5432"

  sync-server:
    build:
      context: .
      dockerfile: crates/sync-server/Dockerfile
    environment:
      DATABASE_URL: postgres://snapline:snapline-dev-password@postgres:5432/snapline
      JWT_SECRET: change-this-secret-before-deploying
      ASSET_DATA_DIR: /data/assets
      PUBLIC_BASE_URL: http://localhost:8080
      ALLOW_REGISTRATION: "true"
      SNAPLINE_BOOTSTRAP_ADMIN_EMAIL: admin@example.com
      SNAPLINE_BOOTSTRAP_ADMIN_PASSWORD: change-me
    volumes:
      - snapline-assets:/data/assets
    ports:
      - "8080:8080"
    depends_on:
      - postgres

volumes:
  snapline-postgres:
  snapline-assets:
```

- [ ] **Step 2: Add sync-server Dockerfile**

Create `crates/sync-server/Dockerfile`:

```dockerfile
FROM rust:1-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p snapline-sync-server

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=builder /app/target/release/snapline-sync-server /usr/local/bin/snapline-sync-server
EXPOSE 8080
CMD ["snapline-sync-server"]
```

- [ ] **Step 3: Add self-hosting guide**

Create `docs/self-hosting.md`:

```markdown
# Snapline Self-Hosting

Snapline sync server is an open source Axum service backed by PostgreSQL. M2 stores image assets on the server filesystem.

## Start Locally

```powershell
docker compose -f docker-compose.sync.yml up --build
```

The server listens on `http://localhost:8080`.

## Required Configuration

- `DATABASE_URL`: PostgreSQL connection string.
- `JWT_SECRET`: long random secret for access tokens.
- `ASSET_DATA_DIR`: directory for image assets.
- `PUBLIC_BASE_URL`: external URL clients use.
- `ALLOW_REGISTRATION`: `true` or `false`.
- `SNAPLINE_BOOTSTRAP_ADMIN_EMAIL`: first account email when registration is disabled.
- `SNAPLINE_BOOTSTRAP_ADMIN_PASSWORD`: first account password when registration is disabled.

## Backup

Back up both PostgreSQL and the asset directory.

For Docker Compose deployments, preserve:

- `snapline-postgres`
- `snapline-assets`

## Disable Public Registration

Set:

```env
ALLOW_REGISTRATION=false
SNAPLINE_BOOTSTRAP_ADMIN_EMAIL=you@example.com
SNAPLINE_BOOTSTRAP_ADMIN_PASSWORD=a-long-password
```

The bootstrap account is created only when no account exists.
```

- [ ] **Step 4: Update gitignore**

Append to `.gitignore`:

```gitignore
.env
server-data/
```

- [ ] **Step 5: Validate compose syntax**

Run:

```powershell
docker compose -f docker-compose.sync.yml config
```

Expected: Docker prints the normalized config with no validation errors.

- [ ] **Step 6: Commit**

Run:

```powershell
git add docker-compose.sync.yml crates/sync-server/Dockerfile docs/self-hosting.md .gitignore
git commit -m "docs: add sync self-hosting guide"
```

## Task 11: End-To-End Verification

**Files:**
- Modify only if verification exposes defects.

- [ ] **Step 1: Run full Rust tests**

Run:

```powershell
cargo test
```

Expected: all Rust tests pass.

- [ ] **Step 2: Run frontend tests and build**

Run:

```powershell
cd apps\desktop-tauri
npm test
npm run build
```

Expected: Vitest and Vite build pass.

- [ ] **Step 3: Start self-hosted sync server**

Run:

```powershell
docker compose -f docker-compose.sync.yml up --build
```

Expected: PostgreSQL starts and sync server responds at `http://localhost:8080/health` with `ok`.

- [ ] **Step 4: Verify local queue behavior**

Manual flow:

1. Start Snapline.
2. Create or edit a note.
3. Pin the note.
4. Paste a PNG image.
5. Delete a different note.

Expected:

- `change_queue` contains `upsert_note`, `asset_upload`, and `delete_note` entries before sync.
- Editing and saving still work with the sync server stopped.

- [ ] **Step 5: Verify login and push**

Manual flow:

1. Open sync settings.
2. Use server URL `http://localhost:8080`.
3. Register or login with the bootstrap account.
4. Trigger sync.

Expected:

- UI shows connected state.
- Accepted queue items are removed.
- Server PostgreSQL contains synced notes.

- [ ] **Step 6: Verify image asset persistence**

Manual flow:

1. Paste a PNG image into a note.
2. Sync.
3. Inspect the asset volume.

Expected:

- PostgreSQL `assets` row exists for the image.
- Asset file exists under `accounts/<account_id>/notes/<note_id>/<asset_id>.png`.

- [ ] **Step 7: Verify conflict with two local profiles**

Manual flow:

1. Run two Snapline profiles by temporarily changing `AppPaths::from_data_dir` in a local test build or by using two OS users.
2. Login both to the same server.
3. Pull the same note to both profiles.
4. Stop server.
5. Edit the same note differently in both profiles.
6. Restart server and sync both.

Expected:

- First upload is accepted.
- Second upload receives conflict.
- Local conflict copy is preserved with Markdown image references unchanged.

- [ ] **Step 8: Commit fixes from verification**

If code changed:

```powershell
git add .
git commit -m "fix: stabilize cloud sync verification"
```

If no code changed, do not create an empty commit.

## Self-Review

- Spec coverage: local queue, sync state, open source Axum server, PostgreSQL schema, auth, push/pull/snapshot, image asset metadata, local filesystem asset store, self-hosting, UI login, and verification are covered.
- Deferred by design: S3/MinIO, password reset, admin dashboard, E2EE, CRDT, teams, and mobile clients remain out of M2.
- Plan scan: no marker text or unspecified implementation steps remain.
- Asset sync check: Task 7 uploads metadata and file bytes together as multipart data, then stores bytes through `LocalFsAssetStore`.
