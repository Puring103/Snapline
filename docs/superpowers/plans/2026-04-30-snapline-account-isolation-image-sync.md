# Snapline Account Isolation And Image Sync Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make logged-in account views isolated from local anonymous drafts, prompt users before importing local drafts into an account, and complete end-to-end pasted image sync.

**Architecture:** Add a local ownership boundary to SQLite notes and sync queue rows. AppCore becomes the single place that interprets current identity, filters note access, imports anonymous drafts, and scopes queued sync work to the current account. Extend the sync client/server asset path so queued image uploads send file bytes to the server and snapshot/pull flows can hydrate missing local images.

**Tech Stack:** Rust workspace, rusqlite, reqwest multipart, Axum multipart, SQLx/PostgreSQL, Tauri commands, React, TypeScript, Vitest.

---

## File Structure

- `crates/domain/src/note.rs`: add `owner_account_id` to `Note` and `NoteSummary`.
- `crates/storage/src/repository.rs`: migrate note ownership, filter/list/get by owner, import anonymous drafts, and preserve ownership on save.
- `crates/storage/src/sync.rs`: add `account_id` to `change_queue`, scope pending queue reads, and keep asset upload queue rows account-aware.
- `crates/app-core/src/lib.rs`: expose anonymous draft count/import APIs, filter bootstrap by current identity, create account-owned notes when logged in, and enqueue only account-owned sync changes.
- `apps/desktop-tauri/src-tauri/src/main.rs`: add Tauri commands for local draft count/import and use scoped sync queues.
- `apps/desktop-tauri/src/types.ts`: add `owner_account_id` and import prompt state types.
- `apps/desktop-tauri/src/api.ts`: add wrappers for anonymous draft count/import and image sync helpers as needed.
- `apps/desktop-tauri/src/SyncSettings.tsx`: trigger an import decision after login.
- `apps/desktop-tauri/src/App.tsx`: refresh visible notes after login/import choice and keep anonymous drafts hidden in account views.
- `apps/desktop-tauri/src/session.ts`: preserve note ownership in summaries.
- `crates/sync-client/src/protocol.rs`: add asset upload/download DTO helpers if needed.
- `crates/sync-client/src/lib.rs`: add `upload_asset` and `download_asset`.
- `crates/sync-client/src/mock.rs`: track asset bytes in mock tests.
- `crates/sync-client/src/processor.rs`: either implement or keep as the focused place for queue processing if the sync logic is moved out of Tauri main.
- `crates/sync-server/src/routes.rs`: return asset metadata in snapshot and keep upload/download compatible.
- `crates/sync-server/src/sync_service.rs`: add asset metadata query for snapshot.
- `crates/sync-server/migrations/0001_init.sql`: verify `assets` schema supports snapshot metadata; add indexes if missing.
- `crates/platform/src/lib.rs`: add helpers for parsing asset ids from markdown paths if this is cleaner than duplicating path parsing.
- Tests: Rust unit tests in touched crates, Tauri command tests in `apps/desktop-tauri/src-tauri/src/main.rs`, Vitest coverage in `apps/desktop-tauri/src`.

## Task 1: Add Note Ownership And Account-Scoped Listing

**Files:**
- Modify: `crates/domain/src/note.rs`
- Modify: `crates/storage/src/repository.rs`
- Test: `crates/storage/src/repository.rs`

- [ ] **Step 1: Write failing repository tests for ownership filtering**

Add tests to `crates/storage/src/repository.rs`:

```rust
#[test]
fn list_recent_filters_by_owner_account() {
    let repo = NoteRepository::open_in_memory().unwrap();
    let t1 = Utc.with_ymd_and_hms(2026, 4, 30, 1, 0, 0).unwrap();
    let local = repo.create_note(t1, None).unwrap();
    let account = repo.create_note(t1, Some("acct_a")).unwrap();

    assert_eq!(repo.list_recent_for_owner(10, None).unwrap()[0].id, local.id);
    assert_eq!(
        repo.list_recent_for_owner(10, Some("acct_a")).unwrap()[0].id,
        account.id
    );
    assert!(repo.list_recent_for_owner(10, Some("acct_b")).unwrap().is_empty());
}

#[test]
fn get_note_requires_matching_owner() {
    let repo = NoteRepository::open_in_memory().unwrap();
    let t1 = Utc.with_ymd_and_hms(2026, 4, 30, 1, 1, 0).unwrap();
    let note = repo.create_note(t1, Some("acct_a")).unwrap();

    assert!(repo.get_note_for_owner(&note.id, Some("acct_a")).is_ok());
    assert!(repo.get_note_for_owner(&note.id, Some("acct_b")).is_err());
    assert!(repo.get_note_for_owner(&note.id, None).is_err());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```powershell
cargo test -p snapline-storage owner
```

Expected: FAIL because `owner_account_id`, `create_note(now, owner)`, `list_recent_for_owner`, and `get_note_for_owner` do not exist.

- [ ] **Step 3: Add ownership fields to domain note types**

Update `crates/domain/src/note.rs`:

```rust
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
    pub owner_account_id: Option<String>,
}

pub struct NoteSummary {
    pub id: NoteId,
    pub title: String,
    pub preview: String,
    pub preview_md: String,
    pub pinned: bool,
    pub updated_at: DateTime<Utc>,
    pub is_conflict_copy: bool,
    pub source_note_id: Option<NoteId>,
    pub owner_account_id: Option<String>,
}
```

In `Note::draft`, initialize:

```rust
owner_account_id: None,
```

- [ ] **Step 4: Migrate and preserve `owner_account_id`**

In `crates/storage/src/repository.rs`, add `owner_account_id TEXT` to the `notes` table and `ensure_column` calls. Update selects and row mapping so `row_to_note` reads the new column.

Use this schema fragment:

```sql
owner_account_id TEXT
```

Update `create_note` to accept an owner:

```rust
pub fn create_note(&self, now: DateTime<Utc>, owner_account_id: Option<&str>) -> Result<Note> {
    let mut note = Note::draft(now);
    note.owner_account_id = owner_account_id.map(str::to_string);
    self.conn.execute(
        "INSERT INTO notes (id, title, content_md, pinned, created_at, updated_at, deleted_at, owner_account_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7)",
        params![
            note.id.to_string(),
            note.title,
            note.content_md,
            note.pinned as i64,
            note.created_at.to_rfc3339(),
            note.updated_at.to_rfc3339(),
            note.owner_account_id,
        ],
    )?;
    Ok(note)
}
```

Update `save_note` to preserve existing ownership and only set ownership on insert:

```rust
pub fn save_note(
    &self,
    id: &NoteId,
    title: &str,
    content_md: &str,
    pinned: bool,
    now: DateTime<Utc>,
    owner_account_id: Option<&str>,
) -> Result<Note> {
    let resolved_title = resolve_note_title(title, content_md);
    self.conn.execute(
        "
        INSERT INTO notes (id, title, content_md, pinned, created_at, updated_at, deleted_at, owner_account_id)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7)
        ON CONFLICT(id) DO UPDATE SET
          title = excluded.title,
          content_md = excluded.content_md,
          pinned = excluded.pinned,
          updated_at = excluded.updated_at,
          deleted_at = NULL
        ",
        params![
            id.to_string(),
            resolved_title,
            content_md,
            pinned as i64,
            now.to_rfc3339(),
            now.to_rfc3339(),
            owner_account_id,
        ],
    )?;
    self.get_note(id)
}
```

Add scoped APIs:

```rust
pub fn get_note_for_owner(&self, id: &NoteId, owner_account_id: Option<&str>) -> Result<Note> {
    let note = self.get_note(id)?;
    if note.owner_account_id.as_deref() == owner_account_id {
        Ok(note)
    } else {
        anyhow::bail!("note not found for current owner")
    }
}

pub fn list_recent_for_owner(
    &self,
    limit: usize,
    owner_account_id: Option<&str>,
) -> Result<Vec<NoteSummary>> {
    let (sql, owner_param): (&str, Option<&str>) = if owner_account_id.is_some() {
        (
            "SELECT id, title, pinned, updated_at, content_md, is_conflict_copy, source_note_id, owner_account_id
             FROM notes
             WHERE deleted_at IS NULL AND owner_account_id = ?2
             ORDER BY pinned DESC, updated_at DESC
             LIMIT ?1",
            owner_account_id,
        )
    } else {
        (
            "SELECT id, title, pinned, updated_at, content_md, is_conflict_copy, source_note_id, owner_account_id
             FROM notes
             WHERE deleted_at IS NULL AND owner_account_id IS NULL
             ORDER BY pinned DESC, updated_at DESC
             LIMIT ?1",
            None,
        )
    };
    let mut stmt = self.conn.prepare(sql)?;
    let rows = match owner_param {
        Some(owner) => stmt.query_map(params![limit as i64, owner], note_summary_from_row)?,
        None => stmt.query_map(params![limit as i64], note_summary_from_row)?,
    };
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
}
```

- [ ] **Step 5: Update existing repository tests and call sites**

Change old test calls:

```rust
repo.create_note(t1).unwrap()
repo.save_note(&note.id, "Hello", "# Hello\nBody", true, t2).unwrap()
repo.list_recent(10).unwrap()
```

to:

```rust
repo.create_note(t1, None).unwrap()
repo.save_note(&note.id, "Hello", "# Hello\nBody", true, t2, None).unwrap()
repo.list_recent_for_owner(10, None).unwrap()
```

Keep compatibility helpers only if they reduce churn:

```rust
pub fn list_recent(&self, limit: usize) -> Result<Vec<NoteSummary>> {
    self.list_recent_for_owner(limit, None)
}
```

- [ ] **Step 6: Run tests**

Run:

```powershell
cargo test -p snapline-domain -p snapline-storage
```

Expected: PASS.

- [ ] **Step 7: Commit**

```powershell
git add crates/domain/src/note.rs crates/storage/src/repository.rs
git commit -m "feat: add local note ownership"
```

## Task 2: Import Anonymous Drafts Into The Current Account

**Files:**
- Modify: `crates/storage/src/repository.rs`
- Modify: `crates/app-core/src/lib.rs`
- Test: `crates/storage/src/repository.rs`
- Test: `crates/app-core/src/lib.rs`

- [ ] **Step 1: Write failing repository tests for import**

Add to `crates/storage/src/repository.rs` tests:

```rust
#[test]
fn imports_anonymous_notes_into_account() {
    let repo = NoteRepository::open_in_memory().unwrap();
    let t1 = Utc.with_ymd_and_hms(2026, 4, 30, 2, 0, 0).unwrap();
    let local = repo.create_note(t1, None).unwrap();
    repo.save_note(&local.id, "Local", "Local body", false, t1, None).unwrap();

    let imported = repo.import_anonymous_notes("acct_a").unwrap();

    assert_eq!(imported, vec![local.id.clone()]);
    assert!(repo.list_recent_for_owner(10, None).unwrap().is_empty());
    assert_eq!(repo.list_recent_for_owner(10, Some("acct_a")).unwrap()[0].id, local.id);
}

#[test]
fn counts_only_visible_anonymous_drafts() {
    let repo = NoteRepository::open_in_memory().unwrap();
    let t1 = Utc.with_ymd_and_hms(2026, 4, 30, 2, 1, 0).unwrap();
    let local = repo.create_note(t1, None).unwrap();
    let account = repo.create_note(t1, Some("acct_a")).unwrap();
    repo.soft_delete(&local.id, t1).unwrap();

    assert_eq!(repo.count_anonymous_notes().unwrap(), 0);
    assert!(repo.get_note_for_owner(&account.id, Some("acct_a")).is_ok());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```powershell
cargo test -p snapline-storage anonymous
```

Expected: FAIL because import/count APIs do not exist.

- [ ] **Step 3: Implement repository import and count**

Add to `NoteRepository`:

```rust
pub fn count_anonymous_notes(&self) -> Result<usize> {
    let count: i64 = self.conn.query_row(
        "SELECT COUNT(*) FROM notes WHERE owner_account_id IS NULL AND deleted_at IS NULL",
        [],
        |row| row.get(0),
    )?;
    Ok(count as usize)
}

pub fn import_anonymous_notes(&self, account_id: &str) -> Result<Vec<NoteId>> {
    let mut stmt = self.conn.prepare(
        "SELECT id FROM notes WHERE owner_account_id IS NULL AND deleted_at IS NULL ORDER BY updated_at ASC",
    )?;
    let ids = stmt
        .query_map([], |row| parse_uuid(row.get::<_, String>(0)?).map(NoteId))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    self.conn.execute(
        "UPDATE notes SET owner_account_id = ?1 WHERE owner_account_id IS NULL AND deleted_at IS NULL",
        params![account_id],
    )?;
    Ok(ids)
}
```

- [ ] **Step 4: Write failing AppCore tests**

Add to `crates/app-core/src/lib.rs` tests:

```rust
#[test]
fn bootstrap_shows_local_notes_when_logged_out_and_account_notes_when_logged_in() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_data_dir(dir.path());
    let repo = NoteRepository::open_in_memory().unwrap();
    let core = AppCore::with_repo(paths, repo);

    let local = core.create_note().unwrap();
    core.save_note(&local.id, "Local", "Local", false).unwrap();
    assert_eq!(core.bootstrap().unwrap().notes.len(), 1);

    core.save_sync_login("http://localhost:8080", "acct_a", "token").unwrap();
    assert!(core.bootstrap().unwrap().notes.is_empty());

    core.import_anonymous_notes_to_current_account().unwrap();
    assert_eq!(core.bootstrap().unwrap().notes.len(), 1);
}

#[test]
fn importing_anonymous_notes_enqueues_upserts_for_current_account() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_data_dir(dir.path());
    let repo = NoteRepository::open_in_memory().unwrap();
    let core = AppCore::with_repo(paths, repo);

    let local = core.create_note().unwrap();
    core.save_note(&local.id, "Local", "Local", false).unwrap();
    core.save_sync_login("http://localhost:8080", "acct_a", "token").unwrap();
    core.import_anonymous_notes_to_current_account().unwrap();

    let changes = core.pending_sync_changes().unwrap();
    assert!(changes.iter().all(|item| item.account_id.as_deref() == Some("acct_a")));
    assert!(changes.iter().any(|item| item.note_id == local.id));
}
```

- [ ] **Step 5: Implement AppCore identity-aware bootstrap/create/import**

Add helpers in `AppCore`:

```rust
fn current_account_id(&self) -> Result<Option<String>> {
    Ok(self.repo.get_or_create_sync_state()?.account_id)
}

pub fn anonymous_note_count(&self) -> Result<usize> {
    self.repo.count_anonymous_notes()
}
```

Update `bootstrap`:

```rust
let owner = self.current_account_id()?;
let notes = self.repo.list_recent_for_owner(50, owner.as_deref())?;
```

Update `create_note`:

```rust
pub fn create_note(&self) -> Result<Note> {
    let owner = self.current_account_id()?;
    self.repo.create_note(Utc::now(), owner.as_deref())
}
```

Add import:

```rust
pub fn import_anonymous_notes_to_current_account(&self) -> Result<Vec<NoteSummary>> {
    let account_id = self
        .current_account_id()?
        .ok_or_else(|| anyhow::anyhow!("not logged in"))?;
    let imported_ids = self.repo.import_anonymous_notes(&account_id)?;
    for note_id in imported_ids {
        let note = self.repo.get_note(&note_id)?;
        self.enqueue_note_change(&note, SyncOpType::UpsertNote, 0)?;
    }
    self.repo.list_recent_for_owner(50, Some(&account_id))
}
```

- [ ] **Step 6: Run tests**

Run:

```powershell
cargo test -p snapline-storage -p snapline-app-core
```

Expected: PASS.

- [ ] **Step 7: Commit**

```powershell
git add crates/storage/src/repository.rs crates/app-core/src/lib.rs
git commit -m "feat: import local drafts into accounts"
```

## Task 3: Scope Sync Queue By Account

**Files:**
- Modify: `crates/storage/src/sync.rs`
- Modify: `crates/storage/src/repository.rs`
- Modify: `crates/app-core/src/lib.rs`
- Test: `crates/storage/src/sync.rs`
- Test: `crates/app-core/src/lib.rs`

- [ ] **Step 1: Write failing sync queue tests**

Add to `crates/storage/src/sync.rs` tests:

```rust
#[test]
fn pending_changes_are_scoped_to_account() {
    let conn = Connection::open_in_memory().unwrap();
    migrate_sync_tables(&conn).unwrap();
    let note = Note::draft(Utc.with_ymd_and_hms(2026, 4, 30, 3, 0, 0).unwrap());
    let payload = SyncPayload::Note(NoteChangePayload::from_note(&note));

    enqueue_change(&conn, Some("acct_a"), &note.id, SyncOpType::UpsertNote, 0, &payload, note.created_at).unwrap();
    enqueue_change(&conn, Some("acct_b"), &note.id, SyncOpType::UpsertNote, 0, &payload, note.created_at).unwrap();

    let acct_a = list_pending_changes(&conn, Some("acct_a"), 10).unwrap();
    assert_eq!(acct_a.len(), 1);
    assert_eq!(acct_a[0].account_id.as_deref(), Some("acct_a"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```powershell
cargo test -p snapline-storage pending_changes_are_scoped_to_account
```

Expected: FAIL because queue APIs are not account-aware.

- [ ] **Step 3: Add `account_id` to queue model and migration**

In `ChangeQueueItem` add:

```rust
pub account_id: Option<String>,
```

Update migration:

```sql
account_id TEXT,
CREATE INDEX IF NOT EXISTS idx_change_queue_account_queued_at
ON change_queue (account_id, queued_at ASC);
```

Add `ensure` logic for existing databases. Since `sync.rs` does not currently have `ensure_column`, add a small helper:

```rust
fn ensure_column(conn: &Connection, table: &str, column: &str, definition: &str) -> Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let has_column = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .any(|name| name == column);
    if !has_column {
        conn.execute(&format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"), [])?;
    }
    Ok(())
}
```

- [ ] **Step 4: Update queue APIs**

Change signatures:

```rust
pub fn enqueue_change(
    conn: &Connection,
    account_id: Option<&str>,
    note_id: &NoteId,
    op_type: SyncOpType,
    base_version: i64,
    payload: &SyncPayload,
    queued_at: DateTime<Utc>,
) -> Result<String>

pub fn list_pending_changes(
    conn: &Connection,
    account_id: Option<&str>,
    limit: usize,
) -> Result<Vec<ChangeQueueItem>>
```

For `list_pending_changes`, use:

```rust
let (sql, account_param): (&str, Option<&str>) = if account_id.is_some() {
    (
        "SELECT id, account_id, note_id, op_type, base_version, payload_json, queued_at, retry_count, last_error
         FROM change_queue WHERE account_id = ?1 ORDER BY queued_at ASC LIMIT ?2",
        account_id,
    )
} else {
    (
        "SELECT id, account_id, note_id, op_type, base_version, payload_json, queued_at, retry_count, last_error
         FROM change_queue WHERE account_id IS NULL ORDER BY queued_at ASC LIMIT ?1",
        None,
    )
};
```

- [ ] **Step 5: Update repository and AppCore queue use**

In `NoteRepository::enqueue_change`, pass `account_id`.

In `AppCore::enqueue_note_change`, skip enqueue for anonymous local drafts:

```rust
let Some(account_id) = note.owner_account_id.as_deref() else {
    return Ok(());
};
self.repo.enqueue_change(Some(account_id), &note.id, op_type, base_version, &payload, Utc::now())?;
```

Update `pending_sync_changes`:

```rust
pub fn pending_sync_changes(&self) -> Result<Vec<snapline_storage::ChangeQueueItem>> {
    let account_id = self
        .current_account_id()?
        .ok_or_else(|| anyhow::anyhow!("not logged in"))?;
    self.repo.list_pending_changes(Some(&account_id), 100)
}
```

For asset uploads in `save_png_asset`, enqueue only if the note belongs to an account:

```rust
let note = self.repo.get_note(note_id)?;
if let Some(account_id) = note.owner_account_id.as_deref() {
    self.repo.enqueue_change(Some(account_id), note_id, SyncOpType::AssetUpload, 0, &payload, Utc::now())?;
}
```

- [ ] **Step 6: Update tests that expected anonymous saves to enqueue**

Change anonymous save expectations in `crates/app-core/src/lib.rs`:

```rust
#[test]
fn anonymous_save_does_not_enqueue_sync_change() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_data_dir(dir.path());
    let repo = NoteRepository::open_in_memory().unwrap();
    let core = AppCore::with_repo(paths, repo);
    let note = core.create_note().unwrap();

    core.save_note(&note.id, "Title", "# Title", false).unwrap();

    assert!(core.pending_sync_changes().is_err());
}
```

Add account-owned save test:

```rust
#[test]
fn account_save_enqueues_scoped_upsert_change() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_data_dir(dir.path());
    let repo = NoteRepository::open_in_memory().unwrap();
    let core = AppCore::with_repo(paths, repo);
    core.save_sync_login("http://localhost:8080", "acct_a", "token").unwrap();
    let note = core.create_note().unwrap();

    core.save_note(&note.id, "Title", "# Title", false).unwrap();

    let changes = core.pending_sync_changes().unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].account_id.as_deref(), Some("acct_a"));
}
```

- [ ] **Step 7: Run tests**

Run:

```powershell
cargo test -p snapline-storage -p snapline-app-core
```

Expected: PASS.

- [ ] **Step 8: Commit**

```powershell
git add crates/storage/src/sync.rs crates/storage/src/repository.rs crates/app-core/src/lib.rs
git commit -m "feat: scope sync queue by account"
```

## Task 4: Add Tauri Commands And UI Import Prompt

**Files:**
- Modify: `apps/desktop-tauri/src-tauri/src/main.rs`
- Modify: `apps/desktop-tauri/src/types.ts`
- Modify: `apps/desktop-tauri/src/api.ts`
- Modify: `apps/desktop-tauri/src/SyncSettings.tsx`
- Modify: `apps/desktop-tauri/src/App.tsx`
- Test: `apps/desktop-tauri/src/syncSettings.test.ts`

- [ ] **Step 1: Add API and type definitions**

In `apps/desktop-tauri/src/types.ts`, update `Note` and `NoteSummary`:

```ts
owner_account_id?: string | null;
```

Add:

```ts
export interface LoginSyncResult {
  account: SyncAccountState;
  anonymous_note_count: number;
}
```

In `apps/desktop-tauri/src/api.ts`, change login and add import:

```ts
loginSync: (serverBaseUrl: string, email: string, password: string) =>
  invoke<LoginSyncResult>("login_sync", { serverBaseUrl, email, password }),
importAnonymousNotes: () => invoke<NoteSummary[]>("import_anonymous_notes"),
anonymousNoteCount: () => invoke<number>("anonymous_note_count"),
```

- [ ] **Step 2: Update Tauri command response**

In `apps/desktop-tauri/src-tauri/src/main.rs`, add:

```rust
#[derive(serde::Serialize)]
struct LoginSyncResult {
    account: SyncAccountState,
    anonymous_note_count: usize,
}
```

Change `login_sync` return type:

```rust
) -> Result<LoginSyncResult, String> {
```

After saving login:

```rust
let core = state.core.lock().map_err(|_| "app state lock poisoned".to_string())?;
let account = core
    .save_sync_login(&server_base_url, &response.account_id, &response.access_token)
    .map_err(|err| err.to_string())?;
let anonymous_note_count = core.anonymous_note_count().map_err(|err| err.to_string())?;
Ok(LoginSyncResult { account, anonymous_note_count })
```

Add commands:

```rust
#[tauri::command]
fn anonymous_note_count(state: State<'_, AppState>) -> Result<usize, String> {
    state
        .core
        .lock()
        .map_err(|_| "app state lock poisoned".to_string())?
        .anonymous_note_count()
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn import_anonymous_notes(state: State<'_, AppState>) -> Result<Vec<NoteSummary>, String> {
    state
        .core
        .lock()
        .map_err(|_| "app state lock poisoned".to_string())?
        .import_anonymous_notes_to_current_account()
        .map_err(|err| err.to_string())
}
```

Register both in `tauri::generate_handler!`.

- [ ] **Step 3: Write failing SyncSettings behavior tests**

In `apps/desktop-tauri/src/syncSettings.test.ts`, add a pure helper test. First export this helper from `SyncSettings.tsx`:

```ts
export function importPromptText(count: number): string {
  return `Detected ${count} local ${count === 1 ? "note" : "notes"}. Import into this account?`;
}
```

Test:

```ts
it("describes local draft import after login", () => {
  expect(importPromptText(1)).toBe("Detected 1 local note. Import into this account?");
  expect(importPromptText(3)).toBe("Detected 3 local notes. Import into this account?");
});
```

- [ ] **Step 4: Update SyncSettings to report login result**

Change props:

```ts
interface SyncSettingsProps {
  initial: SyncAccountState | null;
  onSaved: (result: LoginSyncResult) => void;
}
```

In submit:

```ts
const result = await api.loginSync(serverUrl.trim(), email.trim(), password);
onSaved(result);
setStatus("Connected");
```

- [ ] **Step 5: Add import prompt state in App**

In `apps/desktop-tauri/src/App.tsx`, add state:

```ts
const [pendingImportCount, setPendingImportCount] = useState(0);
```

When handling sync saved:

```ts
function handleSyncSaved(result: LoginSyncResult) {
  setSyncAccount(result.account);
  setPendingImportCount(result.anonymous_note_count);
  void api.bootstrap().then((state) => {
    setNotes(state.notes);
    setCurrentNote(state.current);
  });
}
```

Render a modal/dialog when `pendingImportCount > 0`:

```tsx
{pendingImportCount > 0 ? (
  <div className="connectionDialogBackdrop">
    <div className="connectionDialog" role="dialog" aria-modal="true">
      <div className="connectionDialogTitle">{importPromptText(pendingImportCount)}</div>
      <div className="connectionDialogActions">
        <button type="button" onClick={() => setPendingImportCount(0)}>Do not import</button>
        <button
          type="button"
          onClick={async () => {
            const imported = await api.importAnonymousNotes();
            setNotes(imported);
            setPendingImportCount(0);
          }}
        >
          Import
        </button>
      </div>
    </div>
  </div>
) : null}
```

Use existing dialog classes to avoid new visual language. Keep copy short.

- [ ] **Step 6: Run frontend tests**

Run:

```powershell
cd apps\desktop-tauri
npm test -- syncSettings.test.ts
```

Expected: PASS.

- [ ] **Step 7: Run Rust command tests**

Run:

```powershell
cargo test -p snapline-desktop
```

Expected: PASS.

- [ ] **Step 8: Commit**

```powershell
git add apps/desktop-tauri/src-tauri/src/main.rs apps/desktop-tauri/src/types.ts apps/desktop-tauri/src/api.ts apps/desktop-tauri/src/SyncSettings.tsx apps/desktop-tauri/src/App.tsx apps/desktop-tauri/src/syncSettings.test.ts
git commit -m "feat: prompt before importing local drafts"
```

## Task 5: Add HTTP Asset Upload And Download To Sync Client

**Files:**
- Modify: `crates/sync-client/src/lib.rs`
- Modify: `crates/sync-client/src/mock.rs`
- Test: `crates/sync-client/src/mock.rs`

- [ ] **Step 1: Write failing mock asset tests**

Add to `crates/sync-client/src/mock.rs`:

```rust
#[tokio::test]
async fn mock_uploads_and_downloads_asset_bytes() {
    let api = MockSyncApi::default();
    let note = Note::draft(Utc::now());
    let asset_id = snapline_domain::AssetId::new();
    let payload = snapline_domain::AssetUploadPayload {
        asset_id: asset_id.clone(),
        note_id: note.id.clone(),
        content_type: "image/png".to_string(),
        byte_size: 3,
        sha256: "sha".to_string(),
        markdown_path: format!("assets/notes/{}/{}.png", note.id, asset_id),
    };

    api.upload_asset("token", payload, bytes::Bytes::from_static(b"png")).await.unwrap();
    let downloaded = api.download_asset("token", &asset_id).await.unwrap();

    assert_eq!(downloaded.bytes, bytes::Bytes::from_static(b"png"));
    assert_eq!(downloaded.content_type, "image/png");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```powershell
cargo test -p snapline-sync-client mock_uploads_and_downloads_asset_bytes
```

Expected: FAIL because trait methods and response types do not exist.

- [ ] **Step 3: Add asset methods to trait and HTTP implementation**

In `crates/sync-client/src/lib.rs`, import:

```rust
use bytes::Bytes;
use snapline_domain::{AssetId, AssetUploadPayload};
```

Add:

```rust
pub struct DownloadedAsset {
    pub content_type: String,
    pub bytes: Bytes,
}
```

Extend `SyncApi`:

```rust
async fn upload_asset(&self, token: &str, metadata: AssetUploadPayload, bytes: Bytes) -> Result<()>;
async fn download_asset(&self, token: &str, asset_id: &AssetId) -> Result<DownloadedAsset>;
```

Implement upload:

```rust
async fn upload_asset(&self, token: &str, metadata: AssetUploadPayload, bytes: Bytes) -> Result<()> {
    let metadata_json = serde_json::to_string(&metadata)?;
    let form = reqwest::multipart::Form::new()
        .text("metadata", metadata_json)
        .part(
            "file",
            reqwest::multipart::Part::bytes(bytes.to_vec()).file_name("asset.png"),
        );
    self.client
        .post(format!("{}/sync/assets/upload", self.base_url))
        .bearer_auth(token)
        .multipart(form)
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}
```

Implement download:

```rust
async fn download_asset(&self, token: &str, asset_id: &AssetId) -> Result<DownloadedAsset> {
    let response = self
        .client
        .get(format!("{}/sync/assets/{}/download", self.base_url, asset_id))
        .bearer_auth(token)
        .send()
        .await?
        .error_for_status()?;
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();
    let bytes = response.bytes().await?;
    Ok(DownloadedAsset { content_type, bytes })
}
```

- [ ] **Step 4: Update mock implementation**

In `MockSyncApi`, add:

```rust
assets: Mutex<std::collections::HashMap<AssetId, DownloadedAsset>>,
```

Implement the new methods by inserting and reading from the map.

- [ ] **Step 5: Run tests**

Run:

```powershell
cargo test -p snapline-sync-client
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add crates/sync-client/src/lib.rs crates/sync-client/src/mock.rs
git commit -m "feat: add sync client asset transfer"
```

## Task 6: Upload Queued Image Assets Before Note Push

**Files:**
- Modify: `crates/app-core/src/lib.rs`
- Modify: `apps/desktop-tauri/src-tauri/src/main.rs`
- Test: `crates/app-core/src/lib.rs`

- [ ] **Step 1: Add AppCore helpers for asset queue payload paths**

In `crates/app-core/src/lib.rs`, add:

```rust
pub fn asset_upload_bytes(
    &self,
    item: &snapline_storage::ChangeQueueItem,
) -> Result<Option<(AssetUploadPayload, Vec<u8>)>> {
    match &item.payload {
        SyncPayload::Asset(payload) => {
            let path = self.paths.resolve_markdown_asset_path(&payload.markdown_path);
            Ok(Some((payload.clone(), fs::read(path)?)))
        }
        SyncPayload::Note(_) => Ok(None),
    }
}
```

Add a unit test:

```rust
#[test]
fn asset_upload_bytes_reads_queued_file() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_data_dir(dir.path());
    let repo = NoteRepository::open_in_memory().unwrap();
    let core = AppCore::with_repo(paths, repo);
    core.save_sync_login("http://localhost:8080", "acct_a", "token").unwrap();
    let note = core.create_note().unwrap();

    core.save_png_asset(&note.id, &[137, 80, 78, 71]).unwrap();
    let changes = core.pending_sync_changes().unwrap();
    let (_payload, bytes) = core.asset_upload_bytes(&changes[0]).unwrap().unwrap();

    assert_eq!(bytes, vec![137, 80, 78, 71]);
}
```

- [ ] **Step 2: Run test**

Run:

```powershell
cargo test -p snapline-app-core asset_upload_bytes_reads_queued_file
```

Expected: PASS after helper is implemented.

- [ ] **Step 3: Modify `sync_now` to upload assets first**

In `apps/desktop-tauri/src-tauri/src/main.rs`, split pending changes:

```rust
let api = HttpSyncApi::new(base_url);
let mut note_changes = Vec::new();
let mut uploaded_assets = 0;

for item in pending {
    if let Some((payload, bytes)) = {
        let core = state.core.lock().map_err(|_| "app state lock poisoned".to_string())?;
        core.asset_upload_bytes(&item).map_err(|err| err.to_string())?
    } {
        api.upload_asset(&token, payload, bytes::Bytes::from(bytes))
            .await
            .map_err(|err| err.to_string())?;
        let core = state.core.lock().map_err(|_| "app state lock poisoned".to_string())?;
        core.delete_sync_change(&item.id).map_err(|err| err.to_string())?;
        uploaded_assets += 1;
    } else {
        note_changes.push(item);
    }
}
```

Only call `push` if `note_changes` is not empty.

Return:

```rust
Ok(format!("accepted={accepted}, conflicts={conflicts}, uploaded_assets={uploaded_assets}, failed=0"))
```

- [ ] **Step 4: Run Rust tests**

Run:

```powershell
cargo test -p snapline-app-core -p snapline-desktop
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/app-core/src/lib.rs apps/desktop-tauri/src-tauri/src/main.rs
git commit -m "feat: upload queued image assets"
```

## Task 7: Return Asset Metadata From Server Snapshot

**Files:**
- Modify: `crates/sync-server/src/sync_service.rs`
- Modify: `crates/sync-server/src/routes.rs`
- Test: `crates/sync-server/src/sync_service.rs`

- [ ] **Step 1: Add asset metadata query**

In `crates/sync-server/src/sync_service.rs`, add:

```rust
pub async fn snapshot_assets(pool: &PgPool, account_id: &str) -> Result<Vec<snapline_domain::AssetMetadata>> {
    let rows = sqlx::query(
        "SELECT id, note_id, content_type, byte_size, sha256, storage_key, created_at, deleted_at
         FROM assets WHERE account_id = $1",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|row| {
            Ok(snapline_domain::AssetMetadata {
                id: snapline_domain::AssetId::parse(row.get::<&str, _>("id"))?,
                note_id: NoteId(Uuid::parse_str(row.get::<&str, _>("note_id"))?),
                content_type: row.get("content_type"),
                byte_size: row.get("byte_size"),
                sha256: row.get("sha256"),
                storage_key: row.get("storage_key"),
                created_at: row.get("created_at"),
                deleted_at: row.get("deleted_at"),
            })
        })
        .collect()
}
```

- [ ] **Step 2: Use asset metadata in snapshot route**

In `crates/sync-server/src/routes.rs`, change:

```rust
assets: Vec::new(),
```

to:

```rust
assets: sync_service::snapshot_assets(&state.pool, &account_id)
    .await
    .map_err(internal_error)?,
```

- [ ] **Step 3: Run server tests**

Run:

```powershell
cargo test -p snapline-sync-server
```

Expected: PASS.

- [ ] **Step 4: Commit**

```powershell
git add crates/sync-server/src/sync_service.rs crates/sync-server/src/routes.rs
git commit -m "feat: include assets in sync snapshot"
```

## Task 8: Download Missing Snapshot Assets

**Files:**
- Modify: `crates/app-core/src/lib.rs`
- Modify: `apps/desktop-tauri/src-tauri/src/main.rs`
- Test: `crates/app-core/src/lib.rs`

- [ ] **Step 1: Add local asset existence and write helpers**

In `crates/app-core/src/lib.rs`, add:

```rust
pub fn has_local_asset(&self, markdown_path: &str) -> bool {
    self.paths.resolve_markdown_asset_path(markdown_path).exists()
}

pub fn write_downloaded_asset(
    &self,
    note_id: &NoteId,
    asset_id: &AssetId,
    extension: &str,
    bytes: &[u8],
) -> Result<String> {
    let dir = self.paths.note_asset_dir(note_id);
    fs::create_dir_all(&dir)?;
    let path = self.paths.note_asset_path(note_id, asset_id, extension);
    fs::write(path, bytes)?;
    Ok(self.paths.markdown_asset_path(note_id, asset_id, extension))
}
```

Add test:

```rust
#[test]
fn writes_downloaded_asset_to_note_asset_directory() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_data_dir(dir.path());
    let repo = NoteRepository::open_in_memory().unwrap();
    let core = AppCore::with_repo(paths, repo);
    let note_id = NoteId::new();
    let asset_id = AssetId::new();

    let markdown_path = core.write_downloaded_asset(&note_id, &asset_id, "png", b"png").unwrap();

    assert!(markdown_path.starts_with(&format!("assets/notes/{}/", note_id)));
    assert!(core.has_local_asset(&markdown_path));
}
```

- [ ] **Step 2: Run helper test**

Run:

```powershell
cargo test -p snapline-app-core writes_downloaded_asset_to_note_asset_directory
```

Expected: PASS.

- [ ] **Step 3: Add snapshot asset download to `sync_now`**

After pushing notes in `sync_now`, call:

```rust
let snapshot = api.snapshot(&token).await.map_err(|err| err.to_string())?;
let mut downloaded_assets = 0;
for asset in snapshot.assets {
    let markdown_path = format!("assets/notes/{}/{}.png", asset.note_id, asset.id);
    let should_download = {
        let core = state.core.lock().map_err(|_| "app state lock poisoned".to_string())?;
        !core.has_local_asset(&markdown_path)
    };
    if should_download {
        let downloaded = api
            .download_asset(&token, &asset.id)
            .await
            .map_err(|err| err.to_string())?;
        let core = state.core.lock().map_err(|_| "app state lock poisoned".to_string())?;
        core.write_downloaded_asset(&asset.note_id, &asset.id, "png", &downloaded.bytes)
            .map_err(|err| err.to_string())?;
        downloaded_assets += 1;
    }
}
```

Return string:

```rust
Ok(format!(
    "accepted={accepted}, conflicts={conflicts}, uploaded_assets={uploaded_assets}, downloaded_assets={downloaded_assets}, failed=0"
))
```

- [ ] **Step 4: Run tests**

Run:

```powershell
cargo test -p snapline-app-core -p snapline-desktop -p snapline-sync-client -p snapline-sync-server
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/app-core/src/lib.rs apps/desktop-tauri/src-tauri/src/main.rs
git commit -m "feat: download missing synced images"
```

## Task 9: Apply Remote Notes From Snapshot

**Files:**
- Modify: `crates/storage/src/repository.rs`
- Modify: `crates/app-core/src/lib.rs`
- Modify: `apps/desktop-tauri/src-tauri/src/main.rs`
- Test: `crates/storage/src/repository.rs`
- Test: `crates/app-core/src/lib.rs`

- [ ] **Step 1: Add repository remote upsert test**

Add to `crates/storage/src/repository.rs` tests:

```rust
#[test]
fn upserts_remote_note_into_account_owner() {
    let repo = NoteRepository::open_in_memory().unwrap();
    let mut note = snapline_domain::Note::draft(Utc.with_ymd_and_hms(2026, 4, 30, 4, 0, 0).unwrap());
    note.title = "Remote".to_string();
    note.content_md = "Remote body".to_string();
    note.server_version = 2;

    repo.upsert_remote_note(&note, "acct_a").unwrap();

    let loaded = repo.get_note_for_owner(&note.id, Some("acct_a")).unwrap();
    assert_eq!(loaded.title, "Remote");
    assert_eq!(loaded.owner_account_id.as_deref(), Some("acct_a"));
    assert_eq!(loaded.server_version, 2);
}
```

- [ ] **Step 2: Implement remote upsert**

Add:

```rust
pub fn upsert_remote_note(&self, note: &Note, account_id: &str) -> Result<()> {
    self.conn.execute(
        "
        INSERT INTO notes
          (id, title, content_md, pinned, created_at, updated_at, deleted_at,
           server_version, last_modified_by_device, is_conflict_copy, source_note_id, owner_account_id)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
        ON CONFLICT(id) DO UPDATE SET
          title = excluded.title,
          content_md = excluded.content_md,
          pinned = excluded.pinned,
          updated_at = excluded.updated_at,
          deleted_at = excluded.deleted_at,
          server_version = excluded.server_version,
          last_modified_by_device = excluded.last_modified_by_device,
          owner_account_id = excluded.owner_account_id
        ",
        params![
            note.id.to_string(),
            note.title,
            note.content_md,
            note.pinned as i64,
            note.created_at.to_rfc3339(),
            note.updated_at.to_rfc3339(),
            note.deleted_at.map(|time| time.to_rfc3339()),
            note.server_version,
            note.last_modified_by_device,
            note.is_conflict_copy as i64,
            note.source_note_id.as_ref().map(ToString::to_string),
            account_id,
        ],
    )?;
    Ok(())
}
```

- [ ] **Step 3: Add AppCore snapshot apply**

In `crates/app-core/src/lib.rs`, add:

```rust
pub fn apply_snapshot_notes(&self, notes: &[Note]) -> Result<Vec<NoteSummary>> {
    let account_id = self
        .current_account_id()?
        .ok_or_else(|| anyhow::anyhow!("not logged in"))?;
    for note in notes {
        self.repo.upsert_remote_note(note, &account_id)?;
    }
    self.repo.list_recent_for_owner(50, Some(&account_id))
}
```

- [ ] **Step 4: Use snapshot notes in `sync_now`**

In `apps/desktop-tauri/src-tauri/src/main.rs`, after `let snapshot = api.snapshot(...)`, apply:

```rust
let remote_note_count = snapshot.notes.len();
{
    let core = state.core.lock().map_err(|_| "app state lock poisoned".to_string())?;
    core.apply_snapshot_notes(&snapshot.notes).map_err(|err| err.to_string())?;
}
```

Include `remote_notes={remote_note_count}` in the returned summary.

- [ ] **Step 5: Run tests**

Run:

```powershell
cargo test -p snapline-storage -p snapline-app-core -p snapline-desktop
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add crates/storage/src/repository.rs crates/app-core/src/lib.rs apps/desktop-tauri/src-tauri/src/main.rs
git commit -m "feat: apply remote snapshot notes"
```

## Task 10: Final Verification And Manual Scenario Check

**Files:**
- No code files unless verification exposes fixes.

- [ ] **Step 1: Run all Rust tests**

Run:

```powershell
cargo test
```

Expected: PASS.

- [ ] **Step 2: Run frontend tests**

Run:

```powershell
cd apps\desktop-tauri
npm test
```

Expected: PASS.

- [ ] **Step 3: Build frontend**

Run:

```powershell
cd apps\desktop-tauri
npm run build
```

Expected: PASS.

- [ ] **Step 4: Manual scenario: anonymous draft not imported**

Run app, create one note while logged out, log in, choose `Do not import`.

Expected:
- Account view does not show the anonymous note.
- Logging out or clearing account state later shows the local note again if a logout command exists; if logout does not exist yet, verify through repository test coverage.
- `sync_now` does not upload the anonymous note.

- [ ] **Step 5: Manual scenario: anonymous draft imported**

Create a local anonymous note, log in, choose `Import`, run `sync_now`.

Expected:
- Imported note appears in account view.
- Queue rows for imported notes use the current account id.
- Server snapshot contains the imported note.

- [ ] **Step 6: Manual scenario: image sync**

Log in, paste an image into a note, save, run `sync_now`, then inspect server data or use a second local data directory with the same account.

Expected:
- Server has an `assets` row and a file under `accounts/<account_id>/notes/<note_id>/<asset_id>.png`.
- Snapshot returns asset metadata.
- A client missing the file downloads it and the note preview/editor renders the image.

- [ ] **Step 7: Commit final fixes**

If verification required fixes:

```powershell
git status --short
git add path\to\each\fixed\file
git commit -m "fix: verify account isolation and image sync"
```

If no fixes were needed, do not create an empty commit.

## Self-Review

- Spec coverage: The plan covers B behavior: current-account-only visibility, local draft prompt after login, import as account ownership assignment, no import means hidden from account view, switch-account safety through `owner_account_id`, and account-scoped queue processing. It also covers image upload, server asset metadata, and client download.
- Placeholder scan: No unresolved placeholders remain. Commands and expected outcomes are explicit.
- Type consistency: `owner_account_id`, `account_id`, `LoginSyncResult`, `anonymous_note_count`, `import_anonymous_notes`, `upload_asset`, `download_asset`, and `DownloadedAsset` are defined before use.
