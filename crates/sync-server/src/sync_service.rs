/// 同步业务逻辑：处理推送变更、拉取变更和快照查询。
use anyhow::Result;
use chrono::{DateTime, Utc};
use snapline_domain::{
    AssetId, AssetMetadata, Note, NoteChangePayload, NoteId, SyncOpType, SyncPayload,
};
use snapline_sync_client::protocol::{PushChange, PushChangeResult, RemoteChange};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

/// 处理单条推送变更，在事务内执行乐观锁检查并写入 notes 和 change_log。
///
/// 若服务端当前版本与 `base_version` 不符，返回 `Conflict` 而不写入。
pub async fn apply_push_change(
    tx: &mut Transaction<'_, Postgres>,
    account_id: &str,
    device_id: &str,
    change: PushChange,
) -> Result<PushChangeResult> {
    let existing = sqlx::query(
        "SELECT account_id, title, content_md, pinned, created_at, updated_at, deleted_at, version, last_modified_by_device
         FROM notes WHERE account_id = $1 AND id = $2",
    )
    .bind(account_id)
    .bind(change.note_id.to_string())
    .fetch_optional(&mut **tx)
    .await?;

    if let Some(existing) = existing.as_ref() {
        let version: i64 = existing.get("version");
        if version != change.base_version {
            return Ok(PushChangeResult::Conflict {
                queue_id: change.queue_id,
                note_id: change.note_id.clone(),
                server_note: row_to_note(existing, &change.note_id)?,
            });
        }
    }

    let payload = match change.payload {
        SyncPayload::Note(payload) => payload,
        // 资源上传通过专用接口处理，push 接口只处理笔记变更
        SyncPayload::Asset(_) => {
            return Ok(PushChangeResult::Accepted {
                queue_id: change.queue_id,
                note_id: change.note_id,
                server_version: change.base_version,
                cursor: 0,
            })
        }
    };
    let next_version = existing
        .as_ref()
        .map(|row| row.get::<i64, _>("version") + 1)
        .unwrap_or(1);
    upsert_note(
        tx,
        account_id,
        device_id,
        &change.note_id.to_string(),
        next_version,
        &payload,
    )
    .await?;
    let cursor = append_change_log(
        tx,
        account_id,
        device_id,
        &change.note_id.to_string(),
        next_version,
        &payload,
        note_payload_op_type(&payload),
    )
    .await?;
    Ok(PushChangeResult::Accepted {
        queue_id: change.queue_id,
        note_id: change.note_id,
        server_version: next_version,
        cursor,
    })
}

/// 拉取指定游标之后的变更列表，通过 JOIN notes 获取完整笔记数据。
pub async fn pull_changes(
    pool: &PgPool,
    account_id: &str,
    cursor: i64,
) -> Result<Vec<RemoteChange>> {
    let rows = sqlx::query(
        "SELECT c.cursor, c.device_id, c.created_at, n.account_id, n.id, n.title, n.content_md, n.pinned,
                n.created_at AS note_created_at, n.updated_at, n.deleted_at, n.version, n.last_modified_by_device
         FROM change_log c
         JOIN notes n ON n.account_id = c.account_id AND n.id = c.note_id
         WHERE c.account_id = $1 AND c.cursor > $2
         ORDER BY c.cursor ASC",
    )
    .bind(account_id)
    .bind(cursor)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            let note_id = NoteId(Uuid::parse_str(row.get::<&str, _>("id"))?);
            Ok(RemoteChange {
                cursor: row.get("cursor"),
                device_id: row.get("device_id"),
                changed_at: row.get("created_at"),
                note: joined_row_to_note(&row, note_id)?,
            })
        })
        .collect()
}

/// 获取账户全量快照：所有笔记 + 所有资源元数据 + 当前游标。
pub async fn snapshot(
    pool: &PgPool,
    account_id: &str,
) -> Result<(i64, Vec<Note>, Vec<AssetMetadata>)> {
    let rows = sqlx::query(
        "SELECT account_id, id, title, content_md, pinned, created_at, updated_at, deleted_at, version, last_modified_by_device
         FROM notes WHERE account_id = $1",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await?;
    let cursor = sqlx::query(
        "SELECT COALESCE(MAX(cursor), 0) AS cursor FROM change_log WHERE account_id = $1",
    )
    .bind(account_id)
    .fetch_one(pool)
    .await?
    .get("cursor");
    let notes = rows
        .iter()
        .map(|row| {
            let note_id = NoteId(Uuid::parse_str(row.get::<&str, _>("id"))?);
            row_to_note(row, &note_id)
        })
        .collect::<Result<Vec<_>>>()?;
    let asset_rows = sqlx::query(
        "SELECT id, note_id, content_type, byte_size, sha256, storage_key, created_at, deleted_at
         FROM assets WHERE account_id = $1 AND deleted_at IS NULL",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await?;
    let assets = asset_rows
        .iter()
        .map(row_to_asset_metadata)
        .collect::<Result<Vec<_>>>()?;
    Ok((cursor, notes, assets))
}

/// 将笔记写入（或更新）notes 表，所有字段使用 upsert 语义。
async fn upsert_note(
    tx: &mut Transaction<'_, Postgres>,
    account_id: &str,
    device_id: &str,
    note_id: &str,
    version: i64,
    payload: &NoteChangePayload,
) -> Result<()> {
    sqlx::query(
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
    )
    .bind(note_id)
    .bind(account_id)
    .bind(&payload.title)
    .bind(&payload.content_md)
    .bind(payload.pinned)
    .bind(payload.deleted_at)
    .bind(version)
    .bind(device_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// 向 change_log 追加一条变更记录，返回自增游标值。
async fn append_change_log(
    tx: &mut Transaction<'_, Postgres>,
    account_id: &str,
    device_id: &str,
    note_id: &str,
    version: i64,
    payload: &NoteChangePayload,
    op_type: SyncOpType,
) -> Result<i64> {
    let row = sqlx::query(
        "INSERT INTO change_log (account_id, note_id, op_type, note_version, payload_json, device_id)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING cursor",
    )
    .bind(account_id)
    .bind(note_id)
    .bind(op_type.as_str()) // 使用 domain 层统一的字符串表示，避免本地重复定义转换逻辑
    .bind(version)
    .bind(serde_json::to_value(payload)?)
    .bind(device_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(row.get("cursor"))
}

/// 根据笔记载荷判断操作类型（是删除还是更新）。
fn note_payload_op_type(payload: &NoteChangePayload) -> SyncOpType {
    if payload.deleted_at.is_some() {
        SyncOpType::DeleteNote
    } else {
        SyncOpType::UpsertNote
    }
}

fn row_to_note(row: &sqlx::postgres::PgRow, note_id: &NoteId) -> Result<Note> {
    Ok(Note {
        id: note_id.clone(),
        title: row.get("title"),
        content_md: row.get("content_md"),
        pinned: row.get("pinned"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        deleted_at: row.get("deleted_at"),
        server_version: row.get("version"),
        last_modified_by_device: Some(row.get("last_modified_by_device")),
        is_conflict_copy: false,
        source_note_id: None,
        owner_account_id: Some(row.get("account_id")),
    })
}

/// 从包含 JOIN 别名（`note_created_at`）的行构建 Note。
fn joined_row_to_note(row: &sqlx::postgres::PgRow, note_id: NoteId) -> Result<Note> {
    Ok(Note {
        id: note_id,
        title: row.get("title"),
        content_md: row.get("content_md"),
        pinned: row.get("pinned"),
        created_at: row.get::<DateTime<Utc>, _>("note_created_at"),
        updated_at: row.get("updated_at"),
        deleted_at: row.get("deleted_at"),
        server_version: row.get("version"),
        last_modified_by_device: Some(row.get("last_modified_by_device")),
        is_conflict_copy: false,
        source_note_id: None,
        owner_account_id: Some(row.get("account_id")),
    })
}

fn row_to_asset_metadata(row: &sqlx::postgres::PgRow) -> Result<AssetMetadata> {
    Ok(AssetMetadata {
        id: AssetId::parse(row.get::<&str, _>("id"))?,
        note_id: NoteId(Uuid::parse_str(row.get::<&str, _>("note_id"))?),
        content_type: row.get("content_type"),
        byte_size: row.get("byte_size"),
        sha256: row.get("sha256"),
        storage_key: row.get("storage_key"),
        created_at: row.get("created_at"),
        deleted_at: row.get("deleted_at"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use chrono::{TimeZone, Utc};
    use snapline_sync_client::protocol::PushChange;
    use sqlx::{
        postgres::{PgConnectOptions, PgPoolOptions},
        Row,
    };

    struct PgFixture {
        pool: PgPool,
        schema_name: String,
    }

    impl PgFixture {
        fn schema_name(&self) -> &str {
            &self.schema_name
        }
    }

    async fn pg_fixture() -> Option<PgFixture> {
        let database_url = match std::env::var("SNAPLINE_SYNC_SERVER_TEST_DATABASE_URL") {
            Ok(url) => url,
            Err(_) => {
                eprintln!(
                    "skipping PostgreSQL integration test; set SNAPLINE_SYNC_SERVER_TEST_DATABASE_URL"
                );
                return None;
            }
        };
        let schema_name = format!("snapline_test_{}", Uuid::new_v4().simple());
        let admin_pool = db::connect(&database_url)
            .await
            .expect("connect postgres admin database");
        sqlx::query(&format!(r#"CREATE SCHEMA "{}""#, schema_name))
            .execute(&admin_pool)
            .await
            .expect("create isolated test schema");
        admin_pool.close().await;
        let options: PgConnectOptions = database_url.parse().expect("parse postgres database url");
        let options = options.options([("search_path", schema_name.as_str())]);
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .expect("connect isolated postgres test schema");
        db::migrate(&pool).await.expect("migrate postgres schema");
        Some(PgFixture { pool, schema_name })
    }

    async fn create_account(pool: &PgPool, account_id: &str) {
        sqlx::query(
            "INSERT INTO accounts (id, email, password_hash)
             VALUES ($1, $2, 'hash')",
        )
        .bind(account_id)
        .bind(format!("{account_id}@example.test"))
        .execute(pool)
        .await
        .expect("insert account");
    }

    fn test_account(prefix: &str) -> String {
        format!("{prefix}_{}", Uuid::new_v4().simple())
    }

    fn note_payload(title: &str) -> NoteChangePayload {
        NoteChangePayload {
            title: title.to_string(),
            content_md: format!("# {title}\nBody"),
            pinned: false,
            deleted_at: None,
        }
    }

    fn deleted_note_payload(title: &str) -> NoteChangePayload {
        NoteChangePayload {
            title: title.to_string(),
            content_md: format!("# {title}\nDeleted body"),
            pinned: false,
            deleted_at: Some(Utc.with_ymd_and_hms(2026, 5, 12, 10, 0, 0).unwrap()),
        }
    }

    fn push_change(queue_id: &str, note_id: &NoteId, base_version: i64, title: &str) -> PushChange {
        PushChange {
            queue_id: queue_id.to_string(),
            note_id: note_id.clone(),
            base_version,
            payload: SyncPayload::Note(note_payload(title)),
        }
    }

    #[tokio::test]
    async fn migrated_schema_accepts_first_push_and_logs_cursor() {
        let Some(fixture) = pg_fixture().await else {
            return;
        };
        let pool = &fixture.pool;
        let account_id = test_account("acct_push");
        create_account(pool, &account_id).await;
        let note_id = NoteId::new();
        let mut tx = pool.begin().await.unwrap();

        let result = apply_push_change(
            &mut tx,
            &account_id,
            "device-a",
            push_change("q1", &note_id, 0, "First"),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        assert!(matches!(
            result,
            PushChangeResult::Accepted {
                server_version: 1,
                ..
            }
        ));
        let note = sqlx::query(
            "SELECT title, version, last_modified_by_device FROM notes
             WHERE account_id = $1 AND id = $2",
        )
        .bind(&account_id)
        .bind(note_id.to_string())
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(note.get::<String, _>("title"), "First");
        assert_eq!(note.get::<i64, _>("version"), 1);
        assert_eq!(note.get::<String, _>("last_modified_by_device"), "device-a");
        let log_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM change_log WHERE account_id = $1")
                .bind(&account_id)
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(log_count, 1, "schema {}", fixture.schema_name());
    }

    #[tokio::test]
    async fn push_conflict_returns_server_note_without_mutating_current_version() {
        let Some(fixture) = pg_fixture().await else {
            return;
        };
        let pool = &fixture.pool;
        let account_id = test_account("acct_conflict");
        create_account(pool, &account_id).await;
        let note_id = NoteId::new();
        let mut tx = pool.begin().await.unwrap();
        apply_push_change(
            &mut tx,
            &account_id,
            "device-a",
            push_change("q1", &note_id, 0, "Server"),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let mut tx = pool.begin().await.unwrap();

        let result = apply_push_change(
            &mut tx,
            &account_id,
            "device-b",
            push_change("q2", &note_id, 0, "Local"),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        match result {
            PushChangeResult::Conflict { server_note, .. } => {
                assert_eq!(server_note.title, "Server");
                assert_eq!(server_note.server_version, 1);
            }
            other => panic!("expected conflict, got {other:?}"),
        }
        let title: String =
            sqlx::query_scalar("SELECT title FROM notes WHERE account_id = $1 AND id = $2")
                .bind(&account_id)
                .bind(note_id.to_string())
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(title, "Server");
        let log_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM change_log WHERE account_id = $1")
                .bind(&account_id)
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(log_count, 1, "schema {}", fixture.schema_name());
    }

    #[tokio::test]
    async fn soft_delete_push_updates_note_and_logs_delete_operation() {
        let Some(fixture) = pg_fixture().await else {
            return;
        };
        let pool = &fixture.pool;
        let account_id = test_account("acct_delete");
        create_account(pool, &account_id).await;
        let note_id = NoteId::new();
        let mut tx = pool.begin().await.unwrap();
        let created = apply_push_change(
            &mut tx,
            &account_id,
            "device-a",
            push_change("q1", &note_id, 0, "Live"),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let PushChangeResult::Accepted {
            server_version: base_version,
            ..
        } = created
        else {
            panic!("expected first push to be accepted");
        };
        let mut tx = pool.begin().await.unwrap();

        let deleted = apply_push_change(
            &mut tx,
            &account_id,
            "device-a",
            PushChange {
                queue_id: "delete".to_string(),
                note_id: note_id.clone(),
                base_version,
                payload: SyncPayload::Note(deleted_note_payload("Deleted")),
            },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        assert!(matches!(
            deleted,
            PushChangeResult::Accepted {
                server_version: 2,
                ..
            }
        ));
        let note =
            sqlx::query("SELECT deleted_at, version FROM notes WHERE account_id = $1 AND id = $2")
                .bind(&account_id)
                .bind(note_id.to_string())
                .fetch_one(pool)
                .await
                .unwrap();
        assert!(note.get::<Option<DateTime<Utc>>, _>("deleted_at").is_some());
        assert_eq!(note.get::<i64, _>("version"), 2);
        let op_types = sqlx::query_scalar::<_, String>(
            "SELECT op_type FROM change_log
             WHERE account_id = $1 AND note_id = $2
             ORDER BY cursor ASC",
        )
        .bind(&account_id)
        .bind(note_id.to_string())
        .fetch_all(pool)
        .await
        .unwrap();
        assert_eq!(
            op_types,
            vec![
                SyncOpType::UpsertNote.as_str().to_string(),
                SyncOpType::DeleteNote.as_str().to_string()
            ],
            "schema {}",
            fixture.schema_name()
        );
        let pulled = pull_changes(pool, &account_id, 0).await.unwrap();
        assert_eq!(pulled.len(), 2);
        assert!(pulled[1].note.deleted_at.is_some());
        assert_eq!(pulled[1].note.server_version, 2);
    }

    #[tokio::test]
    async fn pull_changes_filters_by_account_and_cursor() {
        let Some(fixture) = pg_fixture().await else {
            return;
        };
        let pool = &fixture.pool;
        let acct_a = test_account("acct_pull_a");
        let acct_b = test_account("acct_pull_b");
        create_account(pool, &acct_a).await;
        create_account(pool, &acct_b).await;
        let acct_a_first = NoteId::new();
        let acct_a_second = NoteId::new();
        let acct_b_note = NoteId::new();
        for (account_id, note_id, title) in [
            (acct_a.as_str(), &acct_a_first, "A1"),
            (acct_b.as_str(), &acct_b_note, "B1"),
            (acct_a.as_str(), &acct_a_second, "A2"),
        ] {
            let mut tx = pool.begin().await.unwrap();
            apply_push_change(
                &mut tx,
                account_id,
                "device-remote",
                push_change(title, note_id, 0, title),
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
        }

        let all_acct_a = pull_changes(&pool, &acct_a, 0).await.unwrap();
        let after_first = pull_changes(&pool, &acct_a, all_acct_a[0].cursor)
            .await
            .unwrap();

        assert_eq!(all_acct_a.len(), 2);
        assert_eq!(all_acct_a[0].note.title, "A1");
        assert_eq!(all_acct_a[1].note.title, "A2");
        assert_eq!(after_first.len(), 1);
        assert_eq!(after_first[0].note.id, acct_a_second);
    }

    #[tokio::test]
    async fn snapshot_is_account_scoped_and_includes_assets_and_cursor() {
        let Some(fixture) = pg_fixture().await else {
            return;
        };
        let pool = &fixture.pool;
        let acct_a = test_account("acct_snapshot_a");
        let acct_b = test_account("acct_snapshot_b");
        create_account(pool, &acct_a).await;
        create_account(pool, &acct_b).await;
        let acct_a_note = NoteId::new();
        let acct_b_note = NoteId::new();
        for (account_id, note_id, title) in [
            (acct_a.as_str(), &acct_a_note, "A snapshot"),
            (acct_b.as_str(), &acct_b_note, "B snapshot"),
        ] {
            let mut tx = pool.begin().await.unwrap();
            apply_push_change(
                &mut tx,
                account_id,
                "device-remote",
                push_change(title, note_id, 0, title),
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
        }
        let asset_id = AssetId::new();
        sqlx::query(
            "INSERT INTO assets
             (id, account_id, note_id, content_type, byte_size, sha256, storage_key, created_at)
             VALUES ($1, $2, $3, 'image/png', 4, 'sha', 'assets/notes/a/image.png', $4)",
        )
        .bind(asset_id.to_string())
        .bind(&acct_a)
        .bind(acct_a_note.to_string())
        .bind(Utc.with_ymd_and_hms(2026, 5, 12, 8, 0, 0).unwrap())
        .execute(pool)
        .await
        .unwrap();

        let (cursor, notes, assets) = snapshot(&pool, &acct_a).await.unwrap();

        assert!(cursor > 0);
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].title, "A snapshot");
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].id, asset_id);
        assert_eq!(assets[0].note_id, acct_a_note);
    }

    #[tokio::test]
    async fn asset_payload_sent_to_push_is_acknowledged_without_note_log_entry() {
        let Some(fixture) = pg_fixture().await else {
            return;
        };
        let pool = &fixture.pool;
        let account_id = test_account("acct_asset");
        create_account(pool, &account_id).await;
        let note_id = NoteId::new();
        let mut tx = pool.begin().await.unwrap();
        let result = apply_push_change(
            &mut tx,
            &account_id,
            "device-a",
            PushChange {
                queue_id: "asset".to_string(),
                note_id: note_id.clone(),
                base_version: 9,
                payload: SyncPayload::Asset(snapline_domain::AssetUploadPayload {
                    asset_id: AssetId::new(),
                    note_id,
                    content_type: "image/png".to_string(),
                    byte_size: 4,
                    sha256: "sha".to_string(),
                    markdown_path: "assets/notes/note/image.png".to_string(),
                }),
            },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        assert!(matches!(
            result,
            PushChangeResult::Accepted {
                server_version: 9,
                cursor: 0,
                ..
            }
        ));
        let note_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM notes WHERE account_id = $1")
                .bind(&account_id)
                .fetch_one(pool)
                .await
                .unwrap();
        let log_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM change_log WHERE account_id = $1")
                .bind(&account_id)
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(note_count, 0, "schema {}", fixture.schema_name());
        assert_eq!(log_count, 0, "schema {}", fixture.schema_name());
    }
}
