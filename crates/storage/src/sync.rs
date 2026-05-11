use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use snapline_domain::{NoteId, SyncOpType, SyncPayload};
use uuid::Uuid;

/// 待同步的变更队列条目。
///
/// 同一笔记的多次 upsert/delete 操作会被合并为一条记录（coalesced），
/// 避免向服务端推送冗余的中间状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeQueueItem {
    /// 队列条目 UUID，提交给服务端后用于对账。
    pub id: String,
    /// 所属账户 ID；None 表示匿名本地队列。
    pub account_id: Option<String>,
    pub note_id: NoteId,
    pub op_type: SyncOpType,
    /// 推送时携带的基准版本号，服务端据此检测冲突。
    pub base_version: i64,
    pub payload: SyncPayload,
    pub queued_at: DateTime<Utc>,
    /// 已重试次数（每次失败加 1）。
    pub retry_count: i64,
    pub last_error: Option<String>,
}

/// 本地持久化的同步状态，整个数据库只有一行（id = 1）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncState {
    pub account_id: Option<String>,
    /// 本设备的唯一标识，首次打开时生成，用于拉取时过滤自己推送的变更。
    pub device_id: String,
    pub server_base_url: Option<String>,
    /// 上次成功拉取后服务端返回的游标，下次拉取时作为起点。
    pub server_cursor: i64,
    pub access_token: Option<String>,
    pub last_sync_at: Option<DateTime<Utc>>,
    pub last_success_at: Option<DateTime<Utc>>,
    /// base64 编码的 KEK 盐，用于重新派生 KEK 以解包 DEK。
    pub kek_salt: Option<String>,
    /// KEK 包裹后的 DEK，base64(nonce || ciphertext)，持久化以便下次启动时恢复。
    pub encrypted_dek: Option<String>,
}

/// 初始化同步相关的数据库表（change_queue、sync_state）。
///
/// 通过 `ensure_column` 支持增量迁移：对已存在但缺少新列的旧数据库也能升级。
pub fn migrate_sync_tables(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS change_queue (
          id TEXT PRIMARY KEY,
          account_id TEXT,
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
    ensure_column(conn, "change_queue", "account_id", "TEXT")?;
    ensure_column(conn, "sync_state", "kek_salt", "TEXT")?;
    ensure_column(conn, "sync_state", "encrypted_dek", "TEXT")?;
    conn.execute_batch(
        "
        CREATE INDEX IF NOT EXISTS idx_change_queue_account_queued_at
        ON change_queue (account_id, queued_at ASC);
        ",
    )?;
    Ok(())
}

/// 向变更队列中写入一条记录。
///
/// 若队列中已存在同一笔记的 upsert/delete 条目，则原地更新（coalesce），
/// 避免多次编辑产生多条推送请求；资源上传条目不合并，每次独立入队。
pub fn enqueue_change(
    conn: &Connection,
    account_id: Option<&str>,
    note_id: &NoteId,
    op_type: SyncOpType,
    base_version: i64,
    payload: &SyncPayload,
    queued_at: DateTime<Utc>,
) -> Result<String> {
    if matches!(op_type, SyncOpType::UpsertNote | SyncOpType::DeleteNote) {
        if let Some(existing_id) = find_existing_note_change(conn, account_id, note_id)? {
            conn.execute(
                "UPDATE change_queue
                 SET op_type = ?1, payload_json = ?2, queued_at = ?3, retry_count = 0, last_error = NULL
                 WHERE id = ?4",
                params![
                    op_type.as_str(),
                    serde_json::to_string(payload)?,
                    queued_at.to_rfc3339(),
                    existing_id,
                ],
            )?;
            return Ok(existing_id);
        }
    }
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO change_queue (id, account_id, note_id, op_type, base_version, payload_json, queued_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            id,
            account_id,
            note_id.to_string(),
            op_type.as_str(),
            base_version,
            serde_json::to_string(payload)?,
            queued_at.to_rfc3339()
        ],
    )?;
    Ok(id)
}

/// 查找同一账户 + 笔记的已存在 upsert/delete 队列条目 ID。
fn find_existing_note_change(
    conn: &Connection,
    account_id: Option<&str>,
    note_id: &NoteId,
) -> Result<Option<String>> {
    match account_id {
        Some(account_id) => conn
            .query_row(
                "SELECT id FROM change_queue
                 WHERE account_id = ?1 AND note_id = ?2 AND op_type IN ('upsert_note', 'delete_note')
                 ORDER BY queued_at ASC LIMIT 1",
                params![account_id, note_id.to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into),
        None => conn
            .query_row(
                "SELECT id FROM change_queue
                 WHERE account_id IS NULL AND note_id = ?1 AND op_type IN ('upsert_note', 'delete_note')
                 ORDER BY queued_at ASC LIMIT 1",
                params![note_id.to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into),
    }
}

/// 列出待处理的变更条目，按入队时间升序排列。
pub fn list_pending_changes(
    conn: &Connection,
    account_id: Option<&str>,
    limit: usize,
) -> Result<Vec<ChangeQueueItem>> {
    let sql = match account_id {
        Some(account_id) => {
            let mut stmt = conn.prepare(
                "SELECT id, account_id, note_id, op_type, base_version, payload_json, queued_at, retry_count, last_error
                 FROM change_queue WHERE account_id = ?1 ORDER BY queued_at ASC LIMIT ?2",
            )?;
            let rows = stmt
                .query_map(params![account_id, limit as i64], row_to_change_queue_item)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            return Ok(rows);
        }
        None =>
            "SELECT id, account_id, note_id, op_type, base_version, payload_json, queued_at, retry_count, last_error
             FROM change_queue WHERE account_id IS NULL ORDER BY queued_at ASC LIMIT ?1",
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params![limit as i64], row_to_change_queue_item)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

/// 将数据库行映射为 `ChangeQueueItem`。
fn row_to_change_queue_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChangeQueueItem> {
    let note_id = Uuid::parse_str(&row.get::<_, String>(2)?)
        .map(NoteId)
        .map_err(to_sql_err)?;
    let op_str: String = row.get(3)?;
    let op_type = op_str
        .parse::<SyncOpType>()
        .map_err(|_| rusqlite::Error::InvalidParameterName(format!("unknown op_type: {op_str}")))?;
    let payload_json: String = row.get(5)?;
    Ok(ChangeQueueItem {
        id: row.get(0)?,
        account_id: row.get(1)?,
        note_id,
        op_type,
        base_version: row.get(4)?,
        payload: serde_json::from_str(&payload_json).map_err(to_sql_err)?,
        queued_at: parse_time(row.get::<_, String>(6)?)?,
        retry_count: row.get(7)?,
        last_error: row.get(8)?,
    })
}

/// 从队列中删除已处理的条目（接受或冲突解决后调用）。
pub fn delete_change(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM change_queue WHERE id = ?1", params![id])?;
    Ok(())
}

/// 标记条目推送失败，增加重试计数并记录错误信息。
pub fn mark_change_failed(conn: &Connection, id: &str, error: &str) -> Result<()> {
    conn.execute(
        "UPDATE change_queue SET retry_count = retry_count + 1, last_error = ?1 WHERE id = ?2",
        params![error, id],
    )?;
    Ok(())
}

/// 读取（或首次创建）本地同步状态行。
///
/// 首次调用时生成随机 `device_id` 并写入数据库；后续复用同一行。
pub fn get_or_create_sync_state(conn: &Connection) -> Result<SyncState> {
    let existing = conn
        .query_row(
            "SELECT account_id, device_id, server_base_url, server_cursor, access_token,
                    last_sync_at, last_success_at, kek_salt, encrypted_dek
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
        kek_salt: None,
        encrypted_dek: None,
    })
}

/// 将同步状态写回数据库（upsert）。
pub fn save_sync_state(conn: &Connection, state: &SyncState) -> Result<()> {
    conn.execute(
        "INSERT INTO sync_state
         (id, account_id, device_id, server_base_url, server_cursor, access_token,
          last_sync_at, last_success_at, kek_salt, encrypted_dek)
         VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(id) DO UPDATE SET
           account_id = excluded.account_id,
           device_id = excluded.device_id,
           server_base_url = excluded.server_base_url,
           server_cursor = excluded.server_cursor,
           access_token = excluded.access_token,
           last_sync_at = excluded.last_sync_at,
           last_success_at = excluded.last_success_at,
           kek_salt = excluded.kek_salt,
           encrypted_dek = excluded.encrypted_dek",
        params![
            state.account_id,
            state.device_id,
            state.server_base_url,
            state.server_cursor,
            state.access_token,
            state.last_sync_at.map(|time| time.to_rfc3339()),
            state.last_success_at.map(|time| time.to_rfc3339()),
            state.kek_salt,
            state.encrypted_dek,
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
        kek_salt: row.get(7)?,
        encrypted_dek: row.get(8)?,
    })
}

/// 检查表中是否存在指定列，不存在则用 ALTER TABLE 添加（用于增量迁移）。
pub(crate) fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let has_column = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .any(|name| name == column);
    if !has_column {
        conn.execute(
            &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
            [],
        )?;
    }
    Ok(())
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
            None,
            &note.id,
            SyncOpType::UpsertNote,
            0,
            &payload,
            note.created_at,
        )
        .unwrap();

        let items = list_pending_changes(&conn, None, 10).unwrap();
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

    #[test]
    fn migration_adds_account_id_before_index_for_existing_queue_table() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "
            CREATE TABLE change_queue (
              id TEXT PRIMARY KEY,
              note_id TEXT NOT NULL,
              op_type TEXT NOT NULL,
              base_version INTEGER NOT NULL,
              payload_json TEXT NOT NULL,
              queued_at TEXT NOT NULL,
              retry_count INTEGER NOT NULL DEFAULT 0,
              last_error TEXT
            );
            ",
        )
        .unwrap();

        migrate_sync_tables(&conn).unwrap();

        let account_column_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('change_queue') WHERE name = 'account_id'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let account_index_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_change_queue_account_queued_at'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(account_column_count, 1);
        assert_eq!(account_index_count, 1);
    }

    #[test]
    fn pending_changes_are_scoped_to_account() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_sync_tables(&conn).unwrap();
        let note = Note::draft(Utc.with_ymd_and_hms(2026, 4, 30, 3, 0, 0).unwrap());
        let payload = SyncPayload::Note(NoteChangePayload::from_note(&note));

        enqueue_change(
            &conn,
            Some("acct_a"),
            &note.id,
            SyncOpType::UpsertNote,
            0,
            &payload,
            note.created_at,
        )
        .unwrap();
        enqueue_change(
            &conn,
            Some("acct_b"),
            &note.id,
            SyncOpType::UpsertNote,
            0,
            &payload,
            note.created_at,
        )
        .unwrap();

        let acct_a = list_pending_changes(&conn, Some("acct_a"), 10).unwrap();
        assert_eq!(acct_a.len(), 1);
        assert_eq!(acct_a[0].account_id.as_deref(), Some("acct_a"));
    }

    #[test]
    fn upsert_changes_for_same_note_are_coalesced() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_sync_tables(&conn).unwrap();
        let mut note = Note::draft(Utc.with_ymd_and_hms(2026, 4, 30, 4, 0, 0).unwrap());
        let first_payload = SyncPayload::Note(NoteChangePayload::from_note(&note));
        let first_id = enqueue_change(
            &conn,
            Some("acct_a"),
            &note.id,
            SyncOpType::UpsertNote,
            7,
            &first_payload,
            note.created_at,
        )
        .unwrap();
        note.title = "Latest".to_string();
        let latest_payload = SyncPayload::Note(NoteChangePayload::from_note(&note));
        let second_id = enqueue_change(
            &conn,
            Some("acct_a"),
            &note.id,
            SyncOpType::UpsertNote,
            7,
            &latest_payload,
            note.created_at,
        )
        .unwrap();

        let items = list_pending_changes(&conn, Some("acct_a"), 10).unwrap();
        assert_eq!(first_id, second_id);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].base_version, 7);
        assert_eq!(items[0].payload, latest_payload);
    }
}
