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
            op_type: op_type_from_str(&row.get::<_, String>(2)?)?,
            base_version: row.get(3)?,
            payload: serde_json::from_str(&payload_json).map_err(to_sql_err)?,
            queued_at: parse_time(row.get::<_, String>(5)?)?,
            retry_count: row.get(6)?,
            last_error: row.get(7)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
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

fn op_type_from_str(value: &str) -> rusqlite::Result<SyncOpType> {
    match value {
        "upsert_note" => Ok(SyncOpType::UpsertNote),
        "delete_note" => Ok(SyncOpType::DeleteNote),
        "asset_upload" => Ok(SyncOpType::AssetUpload),
        _ => Err(rusqlite::Error::InvalidParameterName(format!(
            "unknown sync op type: {value}"
        ))),
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
