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

/// 本地 SQLite 笔记仓库。
///
/// 封装所有笔记 CRUD、设置键值对，以及同步队列的代理调用。
/// 连接在打开时自动执行增量 schema 迁移，无需手动管理版本号。
pub struct NoteRepository {
    conn: Connection,
}

impl NoteRepository {
    /// 打开（或创建）指定路径的 SQLite 数据库。
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        let repo = Self { conn };
        repo.migrate()?;
        Ok(repo)
    }

    /// 打开内存数据库，仅用于测试。
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let repo = Self { conn };
        repo.migrate()?;
        Ok(repo)
    }

    /// 执行增量 schema 迁移：建表、补列、建索引。
    ///
    /// 所有操作使用 `IF NOT EXISTS` 或 `ensure_column` 保证幂等，
    /// 可在任意旧版本数据库上安全运行。
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
        // 对可能存在的旧版本数据库补齐新增列
        sync::ensure_column(&self.conn, "notes", "pinned", "INTEGER NOT NULL DEFAULT 0")?;
        sync::ensure_column(&self.conn, "notes", "server_version", "INTEGER NOT NULL DEFAULT 0")?;
        sync::ensure_column(&self.conn, "notes", "last_modified_by_device", "TEXT")?;
        sync::ensure_column(&self.conn, "notes", "is_conflict_copy", "INTEGER NOT NULL DEFAULT 0")?;
        sync::ensure_column(&self.conn, "notes", "source_note_id", "TEXT")?;
        sync::ensure_column(&self.conn, "notes", "owner_account_id", "TEXT")?;
        sync::migrate_sync_tables(&self.conn)?;
        Ok(())
    }

    /// 创建一条空白草稿笔记并持久化，返回新笔记。
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

    /// 保存笔记内容（upsert），标题为空或 "Untitled" 时自动从正文推导。
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

    /// 检查指定 ID 的笔记是否存在（不区分 owner）。
    pub fn note_exists(&self, id: &NoteId) -> Result<bool> {
        Ok(self.find_note(id)?.is_some())
    }

    /// 将远端同步来的笔记写入本地，覆盖同 ID 的已有记录（包括 server_version）。
    ///
    /// 此方法不入队同步变更，仅更新本地镜像。
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

    /// 为同步冲突时被拒绝的本地版本创建副本，保留用户的编辑内容。
    ///
    /// 副本标题自动加 `(Conflict copy)` 后缀，`server_version` 归零，
    /// `source_note_id` 指向原笔记以便 UI 展示来源关系。
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

    /// 更新笔记的服务端版本号（推送被服务端接受后调用）。
    pub fn update_note_server_version(&self, id: &NoteId, server_version: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE notes SET server_version = ?1 WHERE id = ?2",
            params![server_version, id.to_string()],
        )?;
        Ok(())
    }

    /// 设置笔记的置顶状态。
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

    /// 更新笔记标题（保持正文不变）。
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

    /// 更新笔记正文 Markdown（标题由调用方单独管理）。
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

    /// 按 ID 获取笔记，不存在则返回错误。
    pub fn get_note(&self, id: &NoteId) -> Result<Note> {
        self.find_note(id)?
            .ok_or_else(|| anyhow::anyhow!("note not found"))
    }

    /// 按 ID 获取笔记，并校验 owner；owner 不匹配时返回错误。
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

    /// 列出最近编辑的笔记（不区分 owner，用于未登录状态）。
    pub fn list_recent(&self, limit: usize) -> Result<Vec<NoteSummary>> {
        self.list_recent_for_owner(limit, None)
    }

    /// 列出指定 owner 最近编辑的笔记，按置顶优先、更新时间降序排列。
    ///
    /// `owner_account_id = None` 表示只列匿名本地笔记。
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

    /// 统计匿名（本地）笔记数量，用于登录前提示是否迁移。
    pub fn count_anonymous_notes(&self) -> Result<usize> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM notes WHERE owner_account_id IS NULL AND deleted_at IS NULL",
            [],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    /// 将所有匿名笔记归属到指定账户，返回迁移的笔记 ID 列表。
    ///
    /// 迁移完成后调用方需为每条笔记入队 `UpsertNote` 同步变更。
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

    /// 软删除笔记：设置 `deleted_at` 和 `updated_at`，不物理删除行。
    pub fn soft_delete(&self, id: &NoteId, now: DateTime<Utc>) -> Result<()> {
        self.conn.execute(
            "UPDATE notes SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2",
            params![now.to_rfc3339(), id.to_string()],
        )?;
        Ok(())
    }

    /// 读取设置项，不存在时返回 None。
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

    /// 写入或删除设置项（`value = None` 时删除该 key）。
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

    // ── 同步队列代理方法 ─────────────────────────────────────────────────────

    /// 向变更队列写入一条同步记录。
    pub fn enqueue_change(
        &self,
        account_id: Option<&str>,
        note_id: &NoteId,
        op_type: SyncOpType,
        base_version: i64,
        payload: &SyncPayload,
        queued_at: DateTime<Utc>,
    ) -> Result<String> {
        sync::enqueue_change(
            &self.conn,
            account_id,
            note_id,
            op_type,
            base_version,
            payload,
            queued_at,
        )
    }

    /// 列出待处理的变更队列条目。
    pub fn list_pending_changes(
        &self,
        account_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<sync::ChangeQueueItem>> {
        sync::list_pending_changes(&self.conn, account_id, limit)
    }

    /// 检查指定笔记是否有待处理的变更（用于冲突检测）。
    pub fn has_pending_note_change(
        &self,
        account_id: Option<&str>,
        note_id: &NoteId,
    ) -> Result<bool> {
        let count: i64 = match account_id {
            Some(account_id) => self.conn.query_row(
                "SELECT COUNT(*) FROM change_queue WHERE account_id = ?1 AND note_id = ?2",
                params![account_id, note_id.to_string()],
                |row| row.get(0),
            )?,
            None => self.conn.query_row(
                "SELECT COUNT(*) FROM change_queue WHERE account_id IS NULL AND note_id = ?1",
                params![note_id.to_string()],
                |row| row.get(0),
            )?,
        };
        Ok(count > 0)
    }

    /// 删除单条变更队列条目（推送被接受后调用）。
    pub fn delete_change(&self, id: &str) -> Result<()> {
        sync::delete_change(&self.conn, id)
    }

    /// 删除某笔记的所有待处理变更（冲突解决后清空队列）。
    pub fn delete_changes_for_note(
        &self,
        account_id: Option<&str>,
        note_id: &NoteId,
    ) -> Result<()> {
        match account_id {
            Some(account_id) => {
                self.conn.execute(
                    "DELETE FROM change_queue WHERE account_id = ?1 AND note_id = ?2",
                    params![account_id, note_id.to_string()],
                )?;
            }
            None => {
                self.conn.execute(
                    "DELETE FROM change_queue WHERE account_id IS NULL AND note_id = ?1",
                    params![note_id.to_string()],
                )?;
            }
        }
        Ok(())
    }

    /// 标记变更队列条目推送失败，增加重试计数。
    pub fn mark_change_failed(&self, id: &str, error: &str) -> Result<()> {
        sync::mark_change_failed(&self.conn, id, error)
    }

    /// 读取（或初始化）本地同步状态。
    pub fn get_or_create_sync_state(&self) -> Result<sync::SyncState> {
        sync::get_or_create_sync_state(&self.conn)
    }

    /// 将同步状态写回数据库。
    pub fn save_sync_state(&self, state: &sync::SyncState) -> Result<()> {
        sync::save_sync_state(&self.conn, state)
    }

    /// 推送成功后更新服务端游标和最后同步时间。
    pub fn update_sync_cursor_success(&self, cursor: i64, now: DateTime<Utc>) -> Result<()> {
        let mut state = self.get_or_create_sync_state()?;
        state.server_cursor = cursor;
        state.last_sync_at = Some(now);
        state.last_success_at = Some(now);
        self.save_sync_state(&state)
    }
}

// ── 行映射辅助函数 ────────────────────────────────────────────────────────────

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
    let content_md: String = row.get(4)?;
    Ok(NoteSummary {
        id: NoteId(parse_uuid(row.get::<_, String>(0)?)?),
        title: row.get(1)?,
        pinned: row.get::<_, i64>(2)? != 0,
        updated_at: parse_time(row.get::<_, String>(3)?)?,
        preview: derive_preview(&content_md),
        preview_md: derive_preview_markdown(&content_md),
        is_conflict_copy: row.get::<_, i64>(5)? != 0,
        source_note_id: source_note_id
            .map(|value| parse_uuid(value).map(NoteId))
            .transpose()?,
        owner_account_id: row.get(7)?,
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

/// 生成一个带指定 ID 的临时内存草稿，用于局部更新时的 fallback。
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

/// 标题解析：空白或 "Untitled" 时从正文 Markdown 自动推导。
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
