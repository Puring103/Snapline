use crate::sync;
use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use snapline_domain::{
    derive_preview, derive_preview_markdown, derive_title, Note, NoteId, NoteSummary, SyncOpType,
    SyncPayload,
};
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
              pinned INTEGER NOT NULL DEFAULT 0,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              deleted_at TEXT,
              server_version INTEGER NOT NULL DEFAULT 0,
              last_modified_by_device TEXT,
              is_conflict_copy INTEGER NOT NULL DEFAULT 0,
              source_note_id TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_notes_deleted_pinned_updated
            ON notes (deleted_at, pinned DESC, updated_at DESC);
            CREATE TABLE IF NOT EXISTS settings (
              key TEXT PRIMARY KEY,
              value TEXT NOT NULL
            );
            ",
        )?;
        self.ensure_column("notes", "pinned", "INTEGER NOT NULL DEFAULT 0")?;
        self.ensure_column("notes", "server_version", "INTEGER NOT NULL DEFAULT 0")?;
        self.ensure_column("notes", "last_modified_by_device", "TEXT")?;
        self.ensure_column("notes", "is_conflict_copy", "INTEGER NOT NULL DEFAULT 0")?;
        self.ensure_column("notes", "source_note_id", "TEXT")?;
        sync::migrate_sync_tables(&self.conn)?;
        Ok(())
    }

    pub fn create_note(&self, now: DateTime<Utc>) -> Result<Note> {
        let note = Note::draft(now);
        self.conn.execute(
            "INSERT INTO notes (id, title, content_md, pinned, created_at, updated_at, deleted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL)",
            params![
                note.id.to_string(),
                note.title,
                note.content_md,
                note.pinned as i64,
                note.created_at.to_rfc3339(),
                note.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(note)
    }

    pub fn save_note(
        &self,
        id: &NoteId,
        title: &str,
        content_md: &str,
        pinned: bool,
        now: DateTime<Utc>,
    ) -> Result<Note> {
        let resolved_title = resolve_note_title(title, content_md);
        self.conn.execute(
            "
            INSERT INTO notes (id, title, content_md, pinned, created_at, updated_at, deleted_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL)
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
            ],
        )?;
        self.get_note(id)
    }

    pub fn apply_remote_note(&self, note: &Note) -> Result<()> {
        self.conn.execute(
            "
            INSERT INTO notes
              (id, title, content_md, pinned, created_at, updated_at, deleted_at,
               server_version, last_modified_by_device, is_conflict_copy, source_note_id)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ON CONFLICT(id) DO UPDATE SET
              title = excluded.title,
              content_md = excluded.content_md,
              pinned = excluded.pinned,
              updated_at = excluded.updated_at,
              deleted_at = excluded.deleted_at,
              server_version = excluded.server_version,
              last_modified_by_device = excluded.last_modified_by_device,
              is_conflict_copy = excluded.is_conflict_copy,
              source_note_id = excluded.source_note_id
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
            ],
        )?;
        Ok(())
    }

    pub fn create_conflict_copy(&self, rejected_note: &Note, now: DateTime<Utc>) -> Result<Note> {
        let mut copy = rejected_note.clone();
        copy.id = NoteId::new();
        copy.title = format!("{} (Conflict copy)", rejected_note.title);
        copy.created_at = now;
        copy.updated_at = now;
        copy.deleted_at = None;
        copy.server_version = 0;
        copy.last_modified_by_device = None;
        copy.is_conflict_copy = true;
        copy.source_note_id = Some(rejected_note.id.clone());
        self.conn.execute(
            "
            INSERT INTO notes
              (id, title, content_md, pinned, created_at, updated_at, deleted_at,
               server_version, last_modified_by_device, is_conflict_copy, source_note_id)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, 0, NULL, 1, ?7)
            ",
            params![
                copy.id.to_string(),
                copy.title,
                copy.content_md,
                copy.pinned as i64,
                copy.created_at.to_rfc3339(),
                copy.updated_at.to_rfc3339(),
                rejected_note.id.to_string(),
            ],
        )?;
        self.get_note(&copy.id)
    }

    pub fn update_note_server_version(&self, id: &NoteId, server_version: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE notes SET server_version = ?1 WHERE id = ?2",
            params![server_version, id.to_string()],
        )?;
        Ok(())
    }

    pub fn set_pinned(&self, id: &NoteId, pinned: bool, now: DateTime<Utc>) -> Result<Note> {
        let note = self
            .find_note(id)?
            .unwrap_or_else(|| draft_note_with_id(id, now));
        self.save_note(id, &note.title, &note.content_md, pinned, now)
    }

    pub fn update_note_title(&self, id: &NoteId, title: &str, now: DateTime<Utc>) -> Result<Note> {
        let note = self
            .find_note(id)?
            .unwrap_or_else(|| draft_note_with_id(id, now));
        self.save_note(id, title, &note.content_md, note.pinned, now)
    }

    pub fn update_note_content(
        &self,
        id: &NoteId,
        content_md: &str,
        now: DateTime<Utc>,
    ) -> Result<Note> {
        let note = self
            .find_note(id)?
            .unwrap_or_else(|| draft_note_with_id(id, now));
        self.save_note(id, &note.title, content_md, note.pinned, now)
    }

    pub fn get_note(&self, id: &NoteId) -> Result<Note> {
        self.find_note(id)?
            .ok_or_else(|| anyhow::anyhow!("note not found"))
    }

    fn find_note(&self, id: &NoteId) -> Result<Option<Note>> {
        self.conn
            .query_row(
                "
                SELECT id, title, content_md, pinned, created_at, updated_at, deleted_at,
                       server_version, last_modified_by_device, is_conflict_copy, source_note_id
                FROM notes WHERE id = ?1
                ",
                params![id.to_string()],
                row_to_note,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn list_recent(&self, limit: usize) -> Result<Vec<NoteSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, pinned, updated_at, content_md, is_conflict_copy, source_note_id FROM notes
             WHERE deleted_at IS NULL
             ORDER BY pinned DESC, updated_at DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
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
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn soft_delete(&self, id: &NoteId, now: DateTime<Utc>) -> Result<()> {
        self.conn.execute(
            "UPDATE notes SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2",
            params![now.to_rfc3339(), id.to_string()],
        )?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT value FROM settings WHERE key = ?1")?;
        let value = stmt.query_row(params![key], |row| row.get::<_, String>(0));
        match value {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    pub fn set_setting(&self, key: &str, value: Option<&str>) -> Result<()> {
        match value {
            Some(value) => {
                self.conn.execute(
                    "INSERT INTO settings (key, value) VALUES (?1, ?2)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    params![key, value],
                )?;
            }
            None => {
                self.conn
                    .execute("DELETE FROM settings WHERE key = ?1", params![key])?;
            }
        }
        Ok(())
    }

    pub fn enqueue_change(
        &self,
        note_id: &NoteId,
        op_type: SyncOpType,
        base_version: i64,
        payload: &SyncPayload,
        queued_at: DateTime<Utc>,
    ) -> Result<String> {
        sync::enqueue_change(
            &self.conn,
            note_id,
            op_type,
            base_version,
            payload,
            queued_at,
        )
    }

    pub fn list_pending_changes(&self, limit: usize) -> Result<Vec<sync::ChangeQueueItem>> {
        sync::list_pending_changes(&self.conn, limit)
    }

    pub fn has_pending_note_change(&self, note_id: &NoteId) -> Result<bool> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM change_queue WHERE note_id = ?1",
            params![note_id.to_string()],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn delete_change(&self, id: &str) -> Result<()> {
        sync::delete_change(&self.conn, id)
    }

    pub fn delete_changes_for_note(&self, note_id: &NoteId) -> Result<()> {
        self.conn.execute(
            "DELETE FROM change_queue WHERE note_id = ?1",
            params![note_id.to_string()],
        )?;
        Ok(())
    }

    pub fn mark_change_failed(&self, id: &str, error: &str) -> Result<()> {
        sync::mark_change_failed(&self.conn, id, error)
    }

    pub fn get_or_create_sync_state(&self) -> Result<sync::SyncState> {
        sync::get_or_create_sync_state(&self.conn)
    }

    pub fn save_sync_state(&self, state: &sync::SyncState) -> Result<()> {
        sync::save_sync_state(&self.conn, state)
    }

    pub fn update_sync_cursor_success(&self, cursor: i64, now: DateTime<Utc>) -> Result<()> {
        let mut state = self.get_or_create_sync_state()?;
        state.server_cursor = cursor;
        state.last_sync_at = Some(now);
        state.last_success_at = Some(now);
        self.save_sync_state(&state)
    }

    fn ensure_column(&self, table: &str, column: &str, definition: &str) -> Result<()> {
        let mut stmt = self.conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let has_column = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .any(|name| name == column);
        if !has_column {
            self.conn.execute(
                &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
                [],
            )?;
        }
        Ok(())
    }
}

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

fn parse_uuid(value: String) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(&value).map_err(|err| rusqlite::Error::ToSqlConversionFailure(Box::new(err)))
}

fn parse_time(value: String) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&value)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|err| rusqlite::Error::ToSqlConversionFailure(Box::new(err)))
}

fn draft_note_with_id(id: &NoteId, now: DateTime<Utc>) -> Note {
    Note {
        id: id.clone(),
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

fn resolve_note_title(title: &str, content_md: &str) -> String {
    let trimmed = title.trim();
    if trimmed.is_empty() || trimmed == "Untitled" {
        derive_title(content_md)
    } else {
        trimmed.to_string()
    }
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
        let updated = repo
            .save_note(&note.id, "Hello", "# Hello\nBody", true, t2)
            .unwrap();

        assert_eq!(updated.title, "Hello");
        assert!(updated.pinned);
        assert_eq!(repo.list_recent(10).unwrap().len(), 1);

        repo.soft_delete(&note.id, t3).unwrap();
        assert!(repo.list_recent(10).unwrap().is_empty());
        assert!(repo.get_note(&note.id).unwrap().deleted_at.is_some());
    }

    #[test]
    fn pinned_notes_sort_before_unpinned_notes() {
        let repo = NoteRepository::open_in_memory().unwrap();
        let t1 = Utc.with_ymd_and_hms(2026, 4, 29, 3, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2026, 4, 29, 3, 1, 0).unwrap();

        let first = repo.create_note(t1).unwrap();
        let second = repo.create_note(t2).unwrap();

        repo.set_pinned(&first.id, true, t2).unwrap();

        let notes = repo.list_recent(10).unwrap();
        assert_eq!(notes[0].id, first.id);
        assert_eq!(notes[0].pinned, true);
        assert_eq!(notes[1].id, second.id);
        assert_eq!(notes[1].pinned, false);
    }

    #[test]
    fn updating_content_keeps_custom_title() {
        let repo = NoteRepository::open_in_memory().unwrap();
        let t1 = Utc.with_ymd_and_hms(2026, 4, 29, 4, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2026, 4, 29, 4, 1, 0).unwrap();

        let note = repo.create_note(t1).unwrap();
        repo.update_note_title(&note.id, "Daily note", t1).unwrap();
        let updated = repo
            .update_note_content(&note.id, "# Heading\nBody", t2)
            .unwrap();

        assert_eq!(updated.title, "Daily note");
        assert_eq!(updated.content_md, "# Heading\nBody");
    }

    #[test]
    fn derives_title_from_first_h1_when_title_is_blank() {
        let repo = NoteRepository::open_in_memory().unwrap();
        let t1 = Utc.with_ymd_and_hms(2026, 4, 29, 4, 2, 0).unwrap();

        let note = repo.create_note(t1).unwrap();
        let updated = repo
            .save_note(&note.id, "", "## Secondary\n# Primary\nBody", false, t1)
            .unwrap();

        assert_eq!(updated.title, "Primary");
    }

    #[test]
    fn list_recent_includes_a_preview() {
        let repo = NoteRepository::open_in_memory().unwrap();
        let t1 = Utc.with_ymd_and_hms(2026, 4, 29, 4, 3, 0).unwrap();

        let note = repo.create_note(t1).unwrap();
        repo.save_note(
            &note.id,
            "Title",
            "# Title\n\nPreview line\nMore",
            false,
            t1,
        )
        .unwrap();

        let notes = repo.list_recent(10).unwrap();
        assert_eq!(notes[0].preview, "Preview line\nMore");
    }

    #[test]
    fn list_recent_includes_markdown_preview() {
        let repo = NoteRepository::open_in_memory().unwrap();
        let t1 = Utc.with_ymd_and_hms(2026, 4, 29, 4, 4, 0).unwrap();

        let note = repo.create_note(t1).unwrap();
        repo.save_note(
            &note.id,
            "Title",
            "# Title\n\n- **Preview** line",
            false,
            t1,
        )
        .unwrap();

        let notes = repo.list_recent(10).unwrap();
        assert_eq!(notes[0].preview, "**Preview** line");
        assert_eq!(notes[0].preview_md, "- **Preview** line");
    }

    #[test]
    fn persists_to_disk() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("snapline.db");
        let t1 = Utc.with_ymd_and_hms(2026, 4, 29, 2, 0, 0).unwrap();
        let note_id = {
            let repo = NoteRepository::open(&db_path).unwrap();
            let note = repo.create_note(t1).unwrap();
            repo.save_note(&note.id, "Persistent", "Persistent", false, t1)
                .unwrap();
            note.id
        };

        let repo = NoteRepository::open(&db_path).unwrap();
        assert_eq!(repo.get_note(&note_id).unwrap().content_md, "Persistent");
    }

    #[test]
    fn persists_settings() {
        let repo = NoteRepository::open_in_memory().unwrap();

        repo.set_setting("shortcut", Some("Ctrl+Alt+S")).unwrap();
        assert_eq!(
            repo.get_setting("shortcut").unwrap().as_deref(),
            Some("Ctrl+Alt+S")
        );

        repo.set_setting("shortcut", None).unwrap();
        assert!(repo.get_setting("shortcut").unwrap().is_none());
    }

    #[test]
    fn applies_remote_note_and_updates_server_version() {
        let repo = NoteRepository::open_in_memory().unwrap();
        let mut note =
            snapline_domain::Note::draft(Utc.with_ymd_and_hms(2026, 4, 29, 8, 0, 0).unwrap());
        note.title = "Remote".to_string();
        note.content_md = "# Remote".to_string();
        note.server_version = 7;
        note.last_modified_by_device = Some("device-b".to_string());

        repo.apply_remote_note(&note).unwrap();

        let loaded = repo.get_note(&note.id).unwrap();
        assert_eq!(loaded.title, "Remote");
        assert_eq!(loaded.server_version, 7);
        assert_eq!(loaded.last_modified_by_device.as_deref(), Some("device-b"));

        repo.update_note_server_version(&note.id, 8).unwrap();
        assert_eq!(repo.get_note(&note.id).unwrap().server_version, 8);
    }

    #[test]
    fn detects_pending_changes_for_note() {
        let repo = NoteRepository::open_in_memory().unwrap();
        let note =
            snapline_domain::Note::draft(Utc.with_ymd_and_hms(2026, 4, 29, 9, 0, 0).unwrap());
        let payload = snapline_domain::SyncPayload::Note(
            snapline_domain::NoteChangePayload::from_note(&note),
        );

        assert!(!repo.has_pending_note_change(&note.id).unwrap());

        repo.enqueue_change(
            &note.id,
            snapline_domain::SyncOpType::UpsertNote,
            0,
            &payload,
            Utc.with_ymd_and_hms(2026, 4, 29, 9, 1, 0).unwrap(),
        )
        .unwrap();

        assert!(repo.has_pending_note_change(&note.id).unwrap());

        repo.delete_changes_for_note(&note.id).unwrap();
        assert!(!repo.has_pending_note_change(&note.id).unwrap());
    }

    #[test]
    fn creates_conflict_copy_for_note_payload() {
        let repo = NoteRepository::open_in_memory().unwrap();
        let mut rejected =
            snapline_domain::Note::draft(Utc.with_ymd_and_hms(2026, 4, 29, 10, 0, 0).unwrap());
        rejected.title = "Local edit".to_string();
        rejected.content_md = "# Local\n![img](assets/notes/local/image.png)".to_string();

        let copy = repo.create_conflict_copy(&rejected, Utc.with_ymd_and_hms(2026, 4, 29, 10, 1, 0).unwrap()).unwrap();

        assert_ne!(copy.id, rejected.id);
        assert!(copy.is_conflict_copy);
        assert_eq!(copy.source_note_id.as_ref(), Some(&rejected.id));
        assert!(copy.title.contains("Conflict"));
        assert_eq!(copy.content_md, rejected.content_md);
        assert_eq!(copy.server_version, 0);
    }
}
