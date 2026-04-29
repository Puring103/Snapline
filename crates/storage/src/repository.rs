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
