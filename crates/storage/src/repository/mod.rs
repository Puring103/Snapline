mod notes;
mod settings;
mod sync_queue;

use crate::sync;
use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;

pub struct NoteRepository {
    pub(crate) conn: Connection,
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
              source_note_id TEXT,
              owner_account_id TEXT
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
        self.ensure_column("notes", "owner_account_id", "TEXT")?;
        sync::migrate_sync_tables(&self.conn)?;
        Ok(())
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

        let note = repo.create_note(t1, None).unwrap();
        let updated = repo
            .save_note(&note.id, "Hello", "# Hello\nBody", true, t2, None)
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

        let first = repo.create_note(t1, None).unwrap();
        let second = repo.create_note(t2, None).unwrap();

        repo.set_pinned(&first.id, true, t2).unwrap();

        let notes = repo.list_recent(10).unwrap();
        assert_eq!(notes[0].id, first.id);
        assert!(notes[0].pinned);
        assert_eq!(notes[1].id, second.id);
        assert!(!notes[1].pinned);
    }

    #[test]
    fn updating_content_keeps_custom_title() {
        let repo = NoteRepository::open_in_memory().unwrap();
        let t1 = Utc.with_ymd_and_hms(2026, 4, 29, 4, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2026, 4, 29, 4, 1, 0).unwrap();

        let note = repo.create_note(t1, None).unwrap();
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

        let note = repo.create_note(t1, None).unwrap();
        let updated = repo
            .save_note(
                &note.id,
                "",
                "## Secondary\n# Primary\nBody",
                false,
                t1,
                None,
            )
            .unwrap();

        assert_eq!(updated.title, "Primary");
    }

    #[test]
    fn list_recent_includes_a_preview() {
        let repo = NoteRepository::open_in_memory().unwrap();
        let t1 = Utc.with_ymd_and_hms(2026, 4, 29, 4, 3, 0).unwrap();

        let note = repo.create_note(t1, None).unwrap();
        repo.save_note(
            &note.id,
            "Title",
            "# Title\n\nPreview line\nMore",
            false,
            t1,
            None,
        )
        .unwrap();

        let notes = repo.list_recent(10).unwrap();
        assert_eq!(notes[0].preview, "Preview line\nMore");
    }

    #[test]
    fn list_recent_includes_markdown_preview() {
        let repo = NoteRepository::open_in_memory().unwrap();
        let t1 = Utc.with_ymd_and_hms(2026, 4, 29, 4, 4, 0).unwrap();

        let note = repo.create_note(t1, None).unwrap();
        repo.save_note(
            &note.id,
            "Title",
            "# Title\n\n- **Preview** line",
            false,
            t1,
            None,
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
            let note = repo.create_note(t1, None).unwrap();
            repo.save_note(&note.id, "Persistent", "Persistent", false, t1, None)
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

        assert!(!repo.has_pending_note_change(None, &note.id).unwrap());

        repo.enqueue_change(
            None,
            &note.id,
            snapline_domain::SyncOpType::UpsertNote,
            0,
            &payload,
            Utc.with_ymd_and_hms(2026, 4, 29, 9, 1, 0).unwrap(),
        )
        .unwrap();

        assert!(repo.has_pending_note_change(None, &note.id).unwrap());

        repo.delete_changes_for_note(None, &note.id).unwrap();
        assert!(!repo.has_pending_note_change(None, &note.id).unwrap());
    }

    #[test]
    fn creates_conflict_copy_for_note_payload() {
        let repo = NoteRepository::open_in_memory().unwrap();
        let mut rejected =
            snapline_domain::Note::draft(Utc.with_ymd_and_hms(2026, 4, 29, 10, 0, 0).unwrap());
        rejected.title = "Local edit".to_string();
        rejected.content_md = "# Local\n![img](assets/notes/local/image.png)".to_string();

        let copy = repo
            .create_conflict_copy(
                &rejected,
                Utc.with_ymd_and_hms(2026, 4, 29, 10, 1, 0).unwrap(),
            )
            .unwrap();

        assert_ne!(copy.id, rejected.id);
        assert!(copy.is_conflict_copy);
        assert_eq!(copy.source_note_id.as_ref(), Some(&rejected.id));
        assert!(copy.title.contains("Conflict"));
        assert_eq!(copy.content_md, rejected.content_md);
        assert_eq!(copy.server_version, 0);
    }

    #[test]
    fn list_recent_filters_by_owner_account() {
        let repo = NoteRepository::open_in_memory().unwrap();
        let t1 = Utc.with_ymd_and_hms(2026, 4, 30, 1, 0, 0).unwrap();
        let local = repo.create_note(t1, None).unwrap();
        let account = repo.create_note(t1, Some("acct_a")).unwrap();

        assert_eq!(
            repo.list_recent_for_owner(10, None).unwrap()[0].id,
            local.id
        );
        assert_eq!(
            repo.list_recent_for_owner(10, Some("acct_a")).unwrap()[0].id,
            account.id
        );
        assert!(repo
            .list_recent_for_owner(10, Some("acct_b"))
            .unwrap()
            .is_empty());
    }

    #[test]
    fn imports_anonymous_notes_into_account() {
        let repo = NoteRepository::open_in_memory().unwrap();
        let t1 = Utc.with_ymd_and_hms(2026, 4, 30, 2, 0, 0).unwrap();
        let local = repo.create_note(t1, None).unwrap();
        repo.save_note(&local.id, "Local", "Local body", false, t1, None)
            .unwrap();

        let imported = repo.import_anonymous_notes("acct_a").unwrap();

        assert_eq!(imported, vec![local.id.clone()]);
        assert!(repo.list_recent_for_owner(10, None).unwrap().is_empty());
        assert_eq!(
            repo.list_recent_for_owner(10, Some("acct_a")).unwrap()[0].id,
            local.id
        );
    }
}
