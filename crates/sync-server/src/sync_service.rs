use anyhow::Result;
use chrono::{DateTime, Utc};
use snapline_domain::{Note, NoteChangePayload, NoteId, SyncPayload};
use snapline_sync_client::protocol::{PushChange, PushChangeResult, RemoteChange};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

pub async fn apply_push_change(
    tx: &mut Transaction<'_, Postgres>,
    account_id: &str,
    device_id: &str,
    change: PushChange,
) -> Result<PushChangeResult> {
    let existing = sqlx::query(
        "SELECT title, content_md, pinned, created_at, updated_at, deleted_at, version, last_modified_by_device
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
    )
    .await?;
    Ok(PushChangeResult::Accepted {
        queue_id: change.queue_id,
        note_id: change.note_id,
        server_version: next_version,
        cursor,
    })
}

pub async fn pull_changes(
    pool: &PgPool,
    account_id: &str,
    cursor: i64,
) -> Result<Vec<RemoteChange>> {
    let rows = sqlx::query(
        "SELECT c.cursor, c.device_id, c.created_at, n.id, n.title, n.content_md, n.pinned,
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

pub async fn snapshot(pool: &PgPool, account_id: &str) -> Result<(i64, Vec<Note>)> {
    let rows = sqlx::query(
        "SELECT id, title, content_md, pinned, created_at, updated_at, deleted_at, version, last_modified_by_device
         FROM notes WHERE account_id = $1",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await?;
    let cursor = sqlx::query("SELECT COALESCE(MAX(cursor), 0) AS cursor FROM change_log WHERE account_id = $1")
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
    Ok((cursor, notes))
}

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

async fn append_change_log(
    tx: &mut Transaction<'_, Postgres>,
    account_id: &str,
    device_id: &str,
    note_id: &str,
    version: i64,
    payload: &NoteChangePayload,
) -> Result<i64> {
    let row = sqlx::query(
        "INSERT INTO change_log (account_id, note_id, op_type, note_version, payload_json, device_id)
         VALUES ($1, $2, 'upsert_note', $3, $4, $5)
         RETURNING cursor",
    )
    .bind(account_id)
    .bind(note_id)
    .bind(version)
    .bind(serde_json::to_value(payload)?)
    .bind(device_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(row.get("cursor"))
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
    })
}

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
    })
}
