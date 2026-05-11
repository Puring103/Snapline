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
            CREATE VIRTUAL TABLE IF NOT EXISTS note_search
            USING fts5(title, content_md, content='notes', content_rowid='rowid');
            CREATE TRIGGER IF NOT EXISTS notes_ai AFTER INSERT ON notes BEGIN
              INSERT INTO note_search(rowid, title, content_md)
              VALUES (new.rowid, new.title, new.content_md);
            END;
            CREATE TRIGGER IF NOT EXISTS notes_ad AFTER DELETE ON notes BEGIN
              INSERT INTO note_search(note_search, rowid, title, content_md)
              VALUES('delete', old.rowid, old.title, old.content_md);
            END;
            CREATE TRIGGER IF NOT EXISTS notes_au AFTER UPDATE ON notes BEGIN
              INSERT INTO note_search(note_search, rowid, title, content_md)
              VALUES('delete', old.rowid, old.title, old.content_md);
              INSERT INTO note_search(rowid, title, content_md)
              VALUES (new.rowid, new.title, new.content_md);
            END;
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
        self.conn
            .execute("INSERT INTO note_search(note_search) VALUES('rebuild')", [])?;
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
mod tests;
