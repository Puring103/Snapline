use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};
use snapline_domain::{
    derive_preview, derive_preview_markdown, derive_title, Note, NoteId, NoteSummary,
};
use uuid::Uuid;

use super::NoteRepository;

impl NoteRepository {
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

    pub fn note_exists(&self, id: &NoteId) -> Result<bool> {
        Ok(self.find_note(id)?.is_some())
    }

    pub fn apply_remote_note(&self, note: &Note) -> Result<()> {
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
              is_conflict_copy = excluded.is_conflict_copy,
              source_note_id = excluded.source_note_id,
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
                note.owner_account_id,
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
        copy.owner_account_id = rejected_note.owner_account_id.clone();
        self.conn.execute(
            "
            INSERT INTO notes
              (id, title, content_md, pinned, created_at, updated_at, deleted_at,
               server_version, last_modified_by_device, is_conflict_copy, source_note_id, owner_account_id)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, 0, NULL, 1, ?7, ?8)
            ",
            params![
                copy.id.to_string(),
                copy.title,
                copy.content_md,
                copy.pinned as i64,
                copy.created_at.to_rfc3339(),
                copy.updated_at.to_rfc3339(),
                rejected_note.id.to_string(),
                copy.owner_account_id,
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
        self.save_note(
            id,
            &note.title,
            &note.content_md,
            pinned,
            now,
            note.owner_account_id.as_deref(),
        )
    }

    pub fn update_note_title(&self, id: &NoteId, title: &str, now: DateTime<Utc>) -> Result<Note> {
        let note = self
            .find_note(id)?
            .unwrap_or_else(|| draft_note_with_id(id, now));
        self.save_note(
            id,
            title,
            &note.content_md,
            note.pinned,
            now,
            note.owner_account_id.as_deref(),
        )
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
        self.save_note(
            id,
            &note.title,
            content_md,
            note.pinned,
            now,
            note.owner_account_id.as_deref(),
        )
    }

    pub fn get_note(&self, id: &NoteId) -> Result<Note> {
        self.find_note(id)?
            .ok_or_else(|| anyhow::anyhow!("note not found"))
    }

    pub fn get_note_for_owner(&self, id: &NoteId, owner_account_id: Option<&str>) -> Result<Note> {
        let note = self.get_note(id)?;
        if note.owner_account_id.as_deref() == owner_account_id {
            Ok(note)
        } else {
            anyhow::bail!("note not found for current owner")
        }
    }

    fn find_note(&self, id: &NoteId) -> Result<Option<Note>> {
        self.conn
            .query_row(
                "
                SELECT id, title, content_md, pinned, created_at, updated_at, deleted_at,
                       server_version, last_modified_by_device, is_conflict_copy, source_note_id, owner_account_id
                FROM notes WHERE id = ?1
                ",
                params![id.to_string()],
                row_to_note,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn list_recent(&self, limit: usize) -> Result<Vec<NoteSummary>> {
        self.list_recent_for_owner(limit, None)
    }

    pub fn list_recent_for_owner(
        &self,
        limit: usize,
        owner_account_id: Option<&str>,
    ) -> Result<Vec<NoteSummary>> {
        match owner_account_id {
            Some(owner) => {
                let mut stmt = self.conn.prepare(
                    "SELECT id, title, pinned, updated_at, content_md, is_conflict_copy, source_note_id, owner_account_id FROM notes
                     WHERE deleted_at IS NULL AND owner_account_id = ?2
                     ORDER BY pinned DESC, updated_at DESC
                     LIMIT ?1",
                )?;
                let rows = stmt.query_map(params![limit as i64, owner], note_summary_from_row)?;
                rows.collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(Into::into)
            }
            None => {
                let mut stmt = self.conn.prepare(
                    "SELECT id, title, pinned, updated_at, content_md, is_conflict_copy, source_note_id, owner_account_id FROM notes
                     WHERE deleted_at IS NULL AND owner_account_id IS NULL
                     ORDER BY pinned DESC, updated_at DESC
                     LIMIT ?1",
                )?;
                let rows = stmt.query_map(params![limit as i64], note_summary_from_row)?;
                rows.collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(Into::into)
            }
        }
    }

    pub fn search_notes_for_owner(
        &self,
        query: &str,
        limit: usize,
        owner_account_id: Option<&str>,
    ) -> Result<Vec<NoteSummary>> {
        let Some(fts_query) = fts5_query(query) else {
            return self.list_recent_for_owner(limit, owner_account_id);
        };

        let results = match owner_account_id {
            Some(owner) => {
                let mut stmt = self.conn.prepare(
                    "SELECT notes.id, notes.title, notes.pinned, notes.updated_at, notes.content_md,
                            notes.is_conflict_copy, notes.source_note_id, notes.owner_account_id,
                            bm25(note_search) AS rank
                     FROM note_search
                     JOIN notes ON notes.rowid = note_search.rowid
                     WHERE note_search MATCH ?1
                       AND notes.deleted_at IS NULL
                       AND notes.owner_account_id = ?2
                     ORDER BY notes.pinned DESC, rank ASC, notes.updated_at DESC
                     LIMIT ?3",
                )?;
                let rows = stmt.query_map(
                    params![fts_query, owner, limit as i64],
                    note_summary_from_search_row,
                )?;
                rows.collect::<rusqlite::Result<Vec<_>>>()
            }
            None => {
                let mut stmt = self.conn.prepare(
                    "SELECT notes.id, notes.title, notes.pinned, notes.updated_at, notes.content_md,
                            notes.is_conflict_copy, notes.source_note_id, notes.owner_account_id,
                            bm25(note_search) AS rank
                     FROM note_search
                     JOIN notes ON notes.rowid = note_search.rowid
                     WHERE note_search MATCH ?1
                       AND notes.deleted_at IS NULL
                       AND notes.owner_account_id IS NULL
                     ORDER BY notes.pinned DESC, rank ASC, notes.updated_at DESC
                     LIMIT ?2",
                )?;
                let rows = stmt.query_map(
                    params![fts_query, limit as i64],
                    note_summary_from_search_row,
                )?;
                rows.collect::<rusqlite::Result<Vec<_>>>()
            }
        }?;

        if results.is_empty() {
            return self.search_notes_like_for_owner(query, limit, owner_account_id);
        }

        Ok(results)
    }

    fn search_notes_like_for_owner(
        &self,
        query: &str,
        limit: usize,
        owner_account_id: Option<&str>,
    ) -> Result<Vec<NoteSummary>> {
        let escaped = query
            .trim()
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        if escaped.is_empty() {
            return self.list_recent_for_owner(limit, owner_account_id);
        }
        let pattern = format!("%{escaped}%");

        match owner_account_id {
            Some(owner) => {
                let mut stmt = self.conn.prepare(
                    "SELECT id, title, pinned, updated_at, content_md, is_conflict_copy, source_note_id, owner_account_id
                     FROM notes
                     WHERE deleted_at IS NULL
                       AND owner_account_id = ?2
                       AND (title LIKE ?1 ESCAPE '\\' OR content_md LIKE ?1 ESCAPE '\\')
                     ORDER BY pinned DESC, updated_at DESC
                     LIMIT ?3",
                )?;
                let rows =
                    stmt.query_map(params![pattern, owner, limit as i64], note_summary_from_row)?;
                rows.collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(Into::into)
            }
            None => {
                let mut stmt = self.conn.prepare(
                    "SELECT id, title, pinned, updated_at, content_md, is_conflict_copy, source_note_id, owner_account_id
                     FROM notes
                     WHERE deleted_at IS NULL
                       AND owner_account_id IS NULL
                       AND (title LIKE ?1 ESCAPE '\\' OR content_md LIKE ?1 ESCAPE '\\')
                     ORDER BY pinned DESC, updated_at DESC
                     LIMIT ?2",
                )?;
                let rows = stmt.query_map(params![pattern, limit as i64], note_summary_from_row)?;
                rows.collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(Into::into)
            }
        }
    }

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

    pub fn soft_delete(&self, id: &NoteId, now: DateTime<Utc>) -> Result<()> {
        self.conn.execute(
            "UPDATE notes SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2",
            params![now.to_rfc3339(), id.to_string()],
        )?;
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
        owner_account_id: row.get(11)?,
    })
}

fn note_summary_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<NoteSummary> {
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
        owner_account_id: row.get(7)?,
    })
}

fn note_summary_from_search_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<NoteSummary> {
    note_summary_from_row(row)
}

fn fts5_query(query: &str) -> Option<String> {
    let terms = query
        .split_whitespace()
        .map(str::trim)
        .filter(|term| !term.is_empty())
        .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
        .collect::<Vec<_>>();

    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" AND "))
    }
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
        owner_account_id: None,
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
