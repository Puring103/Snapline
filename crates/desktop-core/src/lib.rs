use std::{
    fs::{self, File},
    io::{BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use snapline_crypto::{EncryptedAttachmentHeader, EncryptedRecord, MasterKey};
use snapline_domain::{EncryptedEnvelope, SyncChange, SyncOperation};
use uuid::Uuid;

const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS items (
    id TEXT PRIMARY KEY,
    nonce TEXT NOT NULL,
    ciphertext TEXT NOT NULL,
    key_nonce TEXT NOT NULL,
    wrapped_key TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    version INTEGER NOT NULL,
    archived INTEGER NOT NULL DEFAULT 0,
    pinned INTEGER NOT NULL DEFAULT 0,
    sync_status TEXT NOT NULL DEFAULT 'pending'
);
CREATE INDEX IF NOT EXISTS items_updated_idx ON items(updated_at DESC);
CREATE TABLE IF NOT EXISTS outbox (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    object_id TEXT NOT NULL,
    operation TEXT NOT NULL,
    created_at TEXT NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT
);
CREATE TABLE IF NOT EXISTS sync_state (
    object_id TEXT PRIMARY KEY,
    server_version INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS sync_meta (
    key TEXT PRIMARY KEY,
    value INTEGER NOT NULL
);
INSERT OR IGNORE INTO sync_meta (key,value) VALUES ('pull_cursor',0);
CREATE TABLE IF NOT EXISTS sync_conflicts (
    object_id TEXT PRIMARY KEY,
    version INTEGER NOT NULL,
    device_id TEXT NOT NULL,
    operation TEXT NOT NULL,
    nonce TEXT NOT NULL,
    ciphertext TEXT NOT NULL,
    wrapped_key TEXT NOT NULL,
    client_updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS attachments (
    id TEXT PRIMARY KEY,
    nonce_prefix TEXT NOT NULL,
    key_nonce TEXT NOT NULL,
    wrapped_key TEXT NOT NULL,
    chunk_bytes INTEGER NOT NULL,
    ciphertext_bytes INTEGER NOT NULL,
    ciphertext_sha256 TEXT NOT NULL,
    created_at TEXT NOT NULL,
    sync_status TEXT NOT NULL DEFAULT 'pending'
);
CREATE TABLE IF NOT EXISTS attachment_descriptors (
    id TEXT PRIMARY KEY REFERENCES attachments(id) ON DELETE CASCADE,
    media_type TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS ai_jobs (
    item_id TEXT PRIMARY KEY REFERENCES items(id) ON DELETE CASCADE,
    content_fingerprint TEXT NOT NULL,
    status TEXT NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt_at TEXT,
    last_error TEXT,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS ai_jobs_status_idx ON ai_jobs(status,next_attempt_at);
INSERT OR IGNORE INTO ai_jobs
    (item_id,content_fingerprint,status,attempts,next_attempt_at,last_error,updated_at)
    SELECT id,'legacy','pending',0,NULL,NULL,updated_at FROM items;
"#;

const SEARCH_SCHEMA: &str = r#"
CREATE VIRTUAL TABLE IF NOT EXISTS record_search USING fts5(
    item_id UNINDEXED,
    title,
    search_text,
    transcript,
    tags,
    markers,
    tokenize='unicode61'
);
"#;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("database operation failed")]
    Database(#[from] rusqlite::Error),
    #[error("record encryption failed")]
    Crypto(#[from] snapline_crypto::CryptoError),
    #[error("record serialization failed")]
    Serialization(#[from] serde_json::Error),
    #[error("record was not found")]
    NotFound,
    #[error("stored timestamp is invalid")]
    InvalidTimestamp,
    #[error("attachment file operation failed")]
    Io(#[from] std::io::Error),
    #[error("attachment storage is unavailable for an in-memory repository")]
    AttachmentStorageUnavailable,
    #[error("AI job is unavailable")]
    AiJobUnavailable,
    #[error("sync state is invalid")]
    InvalidSyncState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    Text,
    Screenshot,
    Image,
    Audio,
    Video,
    Mixed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AiMetadata {
    pub summary: String,
    pub transcript: Option<String>,
    pub topics: Vec<String>,
    pub entities: Vec<String>,
    pub keywords: Vec<String>,
    pub people: Vec<String>,
    pub locations: Vec<String>,
    pub event_time: Option<String>,
    pub language: String,
    pub suggested_tags: Vec<String>,
    pub suggested_markers: Vec<String>,
    pub search_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ItemContent {
    pub title: String,
    pub markdown: String,
    pub source_type: SourceType,
    pub tags: Vec<String>,
    pub markers: Vec<String>,
    pub attachment_ids: Vec<Uuid>,
    pub ai_metadata: Option<AiMetadata>,
}

impl Default for ItemContent {
    fn default() -> Self {
        Self {
            title: String::new(),
            markdown: String::new(),
            source_type: SourceType::Text,
            tags: Vec::new(),
            markers: Vec::new(),
            attachment_ids: Vec::new(),
            ai_metadata: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Item {
    pub id: Uuid,
    pub content: ItemContent,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: i64,
    pub archived: bool,
    pub pinned: bool,
    pub sync_status: String,
    pub ai_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveItem {
    pub id: Uuid,
    pub content: ItemContent,
    pub archived: bool,
    pub pinned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Attachment {
    pub id: Uuid,
    pub ciphertext_bytes: u64,
    pub ciphertext_sha256: String,
    pub created_at: DateTime<Utc>,
    pub sync_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttachmentDescriptor {
    pub id: Uuid,
    pub media_type: String,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttachmentSyncData {
    pub attachment: Attachment,
    pub header: EncryptedAttachmentHeader,
    pub descriptor: AttachmentDescriptor,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct SyncedItemPayload {
    content: ItemContent,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    archived: bool,
    pinned: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingSyncChange {
    pub sequence: i64,
    pub envelope: EncryptedEnvelope,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SyncConflict {
    pub object_id: Uuid,
    pub local: Option<Item>,
    pub remote: Option<Item>,
    pub remote_deleted: bool,
    pub remote_version: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictChoice {
    Local,
    Remote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteApply {
    Applied,
    Conflict,
    Ignored,
}

pub struct Repository {
    connection: Mutex<Connection>,
    search: Mutex<Connection>,
    attachment_dir: Option<PathBuf>,
}

impl Repository {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let path = path.as_ref();
        let connection = Connection::open(path)?;
        connection.execute_batch(SCHEMA)?;
        connection.execute(
            "UPDATE ai_jobs SET status='pending' WHERE status='processing'",
            [],
        )?;
        let search = Connection::open_in_memory()?;
        search.execute_batch(SEARCH_SCHEMA)?;
        let attachment_dir = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("attachments");
        fs::create_dir_all(&attachment_dir)?;
        Ok(Self {
            connection: Mutex::new(connection),
            search: Mutex::new(search),
            attachment_dir: Some(attachment_dir),
        })
    }

    pub fn open_in_memory() -> Result<Self, StorageError> {
        let connection = Connection::open_in_memory()?;
        connection.execute_batch(SCHEMA)?;
        connection.execute(
            "UPDATE ai_jobs SET status='pending' WHERE status='processing'",
            [],
        )?;
        let search = Connection::open_in_memory()?;
        search.execute_batch(SEARCH_SCHEMA)?;
        Ok(Self {
            connection: Mutex::new(connection),
            search: Mutex::new(search),
            attachment_dir: None,
        })
    }

    pub fn save(&self, master_key: &MasterKey, input: SaveItem) -> Result<Item, StorageError> {
        let now = Utc::now();
        let content_fingerprint = content_fingerprint(&input.content)?;
        let ai_status = if input.content.ai_metadata.is_some() {
            "complete"
        } else {
            "pending"
        };
        let plaintext = serde_json::to_vec(&input.content)?;
        let encrypted = master_key.encrypt(input.id.as_bytes(), &plaintext)?;
        let mut connection = self.connection.lock().expect("repository mutex poisoned");
        let transaction = connection.transaction()?;
        let existing = transaction
            .query_row(
                "SELECT created_at,version FROM items WHERE id=?1",
                [input.id.to_string()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?;
        let (created_at, version) = match existing {
            Some((created_at, version)) => (parse_time(&created_at)?, version + 1),
            None => (now, 1),
        };
        transaction.execute(
            "INSERT INTO items \
             (id,nonce,ciphertext,key_nonce,wrapped_key,created_at,updated_at,version,archived,pinned,sync_status) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'pending') \
             ON CONFLICT(id) DO UPDATE SET nonce=excluded.nonce,ciphertext=excluded.ciphertext, \
             key_nonce=excluded.key_nonce,wrapped_key=excluded.wrapped_key,updated_at=excluded.updated_at, \
             version=excluded.version,archived=excluded.archived,pinned=excluded.pinned,sync_status='pending'",
            params![input.id.to_string(), encrypted.nonce, encrypted.ciphertext, encrypted.key_nonce,
                encrypted.wrapped_key, created_at.to_rfc3339(), now.to_rfc3339(), version,
                input.archived, input.pinned],
        )?;
        transaction.execute(
            "INSERT INTO outbox (object_id,operation,created_at) VALUES (?1,'upsert',?2)",
            params![input.id.to_string(), now.to_rfc3339()],
        )?;
        transaction.execute(
            "INSERT INTO ai_jobs (item_id,content_fingerprint,status,attempts,next_attempt_at,last_error,updated_at) \
             VALUES (?1,?2,?3,0,NULL,NULL,?4) \
             ON CONFLICT(item_id) DO UPDATE SET content_fingerprint=excluded.content_fingerprint, \
             status=CASE WHEN ai_jobs.content_fingerprint<>excluded.content_fingerprint OR excluded.status='complete' \
                         THEN excluded.status ELSE ai_jobs.status END, \
             attempts=CASE WHEN ai_jobs.content_fingerprint<>excluded.content_fingerprint THEN 0 ELSE ai_jobs.attempts END, \
             next_attempt_at=CASE WHEN ai_jobs.content_fingerprint<>excluded.content_fingerprint THEN NULL ELSE ai_jobs.next_attempt_at END, \
             last_error=CASE WHEN ai_jobs.content_fingerprint<>excluded.content_fingerprint THEN NULL ELSE ai_jobs.last_error END, \
             updated_at=excluded.updated_at",
            params![input.id.to_string(), content_fingerprint, ai_status, now.to_rfc3339()],
        )?;
        transaction.commit()?;
        if input.content.ai_metadata.is_some() {
            self.index_item(&input.id, &input.content)?;
        } else {
            self.remove_from_index(input.id)?;
        }
        Ok(Item {
            id: input.id,
            content: input.content,
            created_at,
            updated_at: now,
            version,
            archived: input.archived,
            pinned: input.pinned,
            sync_status: "pending".into(),
            ai_status: ai_status.into(),
        })
    }

    pub fn get(&self, master_key: &MasterKey, id: Uuid) -> Result<Item, StorageError> {
        let connection = self.connection.lock().expect("repository mutex poisoned");
        let stored = connection.query_row(
            "SELECT i.nonce,i.ciphertext,i.key_nonce,i.wrapped_key,i.created_at,i.updated_at,i.version,i.archived,i.pinned,i.sync_status,COALESCE(j.status,'pending') \
             FROM items i LEFT JOIN ai_jobs j ON j.item_id=i.id WHERE i.id=?1",
            [id.to_string()],
            |row| Ok(StoredItem { encrypted: EncryptedRecord { nonce: row.get(0)?, ciphertext: row.get(1)?,
                key_nonce: row.get(2)?, wrapped_key: row.get(3)? }, created_at: row.get(4)?,
                updated_at: row.get(5)?, version: row.get(6)?, archived: row.get(7)?, pinned: row.get(8)?,
                sync_status: row.get(9)?, ai_status: row.get(10)? }),
        ).optional()?.ok_or(StorageError::NotFound)?;
        decrypt_item(master_key, id, stored)
    }

    pub fn list(
        &self,
        master_key: &MasterKey,
        include_archived: bool,
    ) -> Result<Vec<Item>, StorageError> {
        let connection = self.connection.lock().expect("repository mutex poisoned");
        let mut statement = connection.prepare(
            "SELECT i.id,i.nonce,i.ciphertext,i.key_nonce,i.wrapped_key,i.created_at,i.updated_at,i.version,i.archived,i.pinned,i.sync_status,COALESCE(j.status,'pending') \
             FROM items i LEFT JOIN ai_jobs j ON j.item_id=i.id WHERE (?1 OR i.archived=0) ORDER BY i.pinned DESC,i.updated_at DESC",
        )?;
        let rows = statement.query_map([include_archived], |row| {
            Ok((
                Uuid::parse_str(&row.get::<_, String>(0)?)
                    .map_err(|_| rusqlite::Error::InvalidQuery)?,
                StoredItem {
                    encrypted: EncryptedRecord {
                        nonce: row.get(1)?,
                        ciphertext: row.get(2)?,
                        key_nonce: row.get(3)?,
                        wrapped_key: row.get(4)?,
                    },
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                    version: row.get(7)?,
                    archived: row.get(8)?,
                    pinned: row.get(9)?,
                    sync_status: row.get(10)?,
                    ai_status: row.get(11)?,
                },
            ))
        })?;
        rows.map(|row| {
            let (id, stored) = row?;
            decrypt_item(master_key, id, stored)
        })
        .collect()
    }

    pub fn delete(&self, id: Uuid) -> Result<(), StorageError> {
        let now = Utc::now().to_rfc3339();
        let mut connection = self.connection.lock().expect("repository mutex poisoned");
        let transaction = connection.transaction()?;
        if transaction.execute("DELETE FROM items WHERE id=?1", [id.to_string()])? == 0 {
            return Err(StorageError::NotFound);
        }
        transaction.execute(
            "INSERT INTO outbox (object_id,operation,created_at) VALUES (?1,'delete',?2)",
            params![id.to_string(), now],
        )?;
        transaction.commit()?;
        self.remove_from_index(id)?;
        Ok(())
    }

    pub fn pending_sync_changes(
        &self,
        master_key: &MasterKey,
        device_id: Uuid,
        limit: usize,
    ) -> Result<Vec<PendingSyncChange>, StorageError> {
        let rows = {
            let connection = self.connection.lock().expect("repository mutex poisoned");
            let mut statement = connection.prepare(
                "SELECT o.sequence,o.object_id,o.operation,s.server_version \
                 FROM outbox o \
                 JOIN (SELECT object_id,MAX(sequence) AS sequence FROM outbox \
                       WHERE operation IN ('upsert','delete') GROUP BY object_id) latest \
                   ON latest.sequence=o.sequence \
                 LEFT JOIN sync_state s ON s.object_id=o.object_id \
                 WHERE NOT EXISTS(SELECT 1 FROM sync_conflicts c WHERE c.object_id=o.object_id) \
                 ORDER BY o.sequence LIMIT ?1",
            )?;
            statement
                .query_map([limit.clamp(1, 100) as i64], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<i64>>(3)?.unwrap_or(0),
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?
        };
        rows.into_iter()
            .map(|(sequence, id, operation, base_version)| {
                let id = Uuid::parse_str(&id).map_err(|_| StorageError::InvalidSyncState)?;
                let (operation, encrypted, updated_at) = if operation == "delete" {
                    (
                        SyncOperation::Delete,
                        master_key.encrypt(id.as_bytes(), b"deleted")?,
                        Utc::now(),
                    )
                } else {
                    let item = self.get(master_key, id)?;
                    let payload = SyncedItemPayload {
                        content: item.content,
                        created_at: item.created_at,
                        updated_at: item.updated_at,
                        archived: item.archived,
                        pinned: item.pinned,
                    };
                    (
                        SyncOperation::Upsert,
                        master_key.encrypt(id.as_bytes(), &serde_json::to_vec(&payload)?)?,
                        payload.updated_at,
                    )
                };
                let wrapped_key = pack_key(&encrypted)?;
                Ok(PendingSyncChange {
                    sequence,
                    envelope: EncryptedEnvelope {
                        object_id: id,
                        object_type: "item".into(),
                        device_id,
                        base_version,
                        operation,
                        ciphertext: encrypted.ciphertext,
                        nonce: encrypted.nonce,
                        wrapped_key,
                        client_updated_at: updated_at,
                    },
                })
            })
            .collect()
    }

    pub fn complete_sync_push(&self, accepted: &[(Uuid, i64, i64)]) -> Result<(), StorageError> {
        let mut connection = self.connection.lock().expect("repository mutex poisoned");
        let transaction = connection.transaction()?;
        for (id, version, sequence) in accepted {
            transaction.execute(
                "INSERT INTO sync_state (object_id,server_version) VALUES (?1,?2) \
                 ON CONFLICT(object_id) DO UPDATE SET server_version=excluded.server_version",
                params![id.to_string(), version],
            )?;
            transaction.execute(
                "DELETE FROM outbox WHERE object_id=?1 AND sequence<=?2 AND operation IN ('upsert','delete')",
                params![id.to_string(), sequence],
            )?;
            let remains: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM outbox WHERE object_id=?1 AND operation IN ('upsert','delete'))",
                [id.to_string()],
                |row| row.get(0),
            )?;
            if !remains {
                transaction.execute(
                    "UPDATE items SET sync_status='synced' WHERE id=?1",
                    [id.to_string()],
                )?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn pull_cursor(&self) -> Result<i64, StorageError> {
        let connection = self.connection.lock().expect("repository mutex poisoned");
        connection
            .query_row(
                "SELECT value FROM sync_meta WHERE key='pull_cursor'",
                [],
                |row| row.get(0),
            )
            .map_err(StorageError::Database)
    }

    pub fn set_pull_cursor(&self, cursor: i64) -> Result<(), StorageError> {
        if cursor < 0 {
            return Err(StorageError::InvalidSyncState);
        }
        let connection = self.connection.lock().expect("repository mutex poisoned");
        connection.execute(
            "UPDATE sync_meta SET value=MAX(value,?1) WHERE key='pull_cursor'",
            [cursor],
        )?;
        Ok(())
    }

    pub fn apply_remote_change(
        &self,
        master_key: &MasterKey,
        current_device: Uuid,
        change: &SyncChange,
    ) -> Result<RemoteApply, StorageError> {
        if change.envelope.object_type != "item" || change.version <= 0 {
            return Ok(RemoteApply::Ignored);
        }
        let connection = self.connection.lock().expect("repository mutex poisoned");
        let server_version = connection
            .query_row(
                "SELECT server_version FROM sync_state WHERE object_id=?1",
                [change.envelope.object_id.to_string()],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or(0);
        if server_version >= change.version {
            return Ok(RemoteApply::Ignored);
        }
        let has_pending: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM outbox WHERE object_id=?1 AND operation IN ('upsert','delete'))",
            [change.envelope.object_id.to_string()],
            |row| row.get(0),
        )?;
        drop(connection);
        if has_pending && change.envelope.device_id != current_device {
            self.store_conflict(change)?;
            return Ok(RemoteApply::Conflict);
        }
        if has_pending {
            let connection = self.connection.lock().expect("repository mutex poisoned");
            connection.execute(
                "INSERT INTO sync_state (object_id,server_version) VALUES (?1,?2) \
                 ON CONFLICT(object_id) DO UPDATE SET server_version=MAX(sync_state.server_version,excluded.server_version)",
                params![change.envelope.object_id.to_string(), change.version],
            )?;
            return Ok(RemoteApply::Ignored);
        }
        self.apply_remote_force(master_key, change, false)?;
        Ok(RemoteApply::Applied)
    }

    pub fn list_sync_conflicts(
        &self,
        master_key: &MasterKey,
    ) -> Result<Vec<SyncConflict>, StorageError> {
        let changes = self.stored_conflicts()?;
        changes
            .into_iter()
            .map(|change| {
                let local = self.get(master_key, change.envelope.object_id).ok();
                let remote_deleted = change.envelope.operation == SyncOperation::Delete;
                let remote = if remote_deleted {
                    None
                } else {
                    Some(remote_item(master_key, &change)?)
                };
                Ok(SyncConflict {
                    object_id: change.envelope.object_id,
                    local,
                    remote,
                    remote_deleted,
                    remote_version: change.version,
                })
            })
            .collect()
    }

    pub fn resolve_sync_conflict(
        &self,
        master_key: &MasterKey,
        id: Uuid,
        choice: ConflictChoice,
    ) -> Result<(), StorageError> {
        let change = self
            .stored_conflicts()?
            .into_iter()
            .find(|change| change.envelope.object_id == id)
            .ok_or(StorageError::NotFound)?;
        match choice {
            ConflictChoice::Local => {
                let connection = self.connection.lock().expect("repository mutex poisoned");
                connection.execute(
                    "INSERT INTO sync_state (object_id,server_version) VALUES (?1,?2) \
                     ON CONFLICT(object_id) DO UPDATE SET server_version=excluded.server_version",
                    params![id.to_string(), change.version],
                )?;
                connection.execute(
                    "DELETE FROM sync_conflicts WHERE object_id=?1",
                    [id.to_string()],
                )?;
            }
            ConflictChoice::Remote => self.apply_remote_force(master_key, &change, true)?,
        }
        Ok(())
    }

    pub fn claim_ai_jobs(
        &self,
        master_key: &MasterKey,
        limit: usize,
    ) -> Result<Vec<Item>, StorageError> {
        let now = Utc::now();
        let ids = {
            let mut connection = self.connection.lock().expect("repository mutex poisoned");
            let transaction = connection.transaction()?;
            let ids = {
                let mut statement = transaction.prepare(
                    "SELECT item_id FROM ai_jobs \
                     WHERE status='pending' OR (status='failed' AND (next_attempt_at IS NULL OR next_attempt_at<=?1)) \
                     ORDER BY updated_at ASC LIMIT ?2",
                )?;
                statement
                    .query_map(params![now.to_rfc3339(), limit.min(50) as i64], |row| {
                        row.get::<_, String>(0)
                    })?
                    .collect::<Result<Vec<_>, _>>()?
            };
            for id in &ids {
                transaction.execute(
                    "UPDATE ai_jobs SET status='processing',attempts=attempts+1,updated_at=?2 WHERE item_id=?1",
                    params![id, now.to_rfc3339()],
                )?;
            }
            transaction.commit()?;
            ids
        };
        ids.into_iter()
            .map(|id| {
                Uuid::parse_str(&id)
                    .map_err(|_| StorageError::AiJobUnavailable)
                    .and_then(|id| self.get(master_key, id))
            })
            .collect()
    }

    pub fn complete_ai_job(
        &self,
        master_key: &MasterKey,
        id: Uuid,
        metadata: AiMetadata,
    ) -> Result<Item, StorageError> {
        let mut item = self.get(master_key, id)?;
        item.content.ai_metadata = Some(metadata);
        let plaintext = serde_json::to_vec(&item.content)?;
        let encrypted = master_key.encrypt(id.as_bytes(), &plaintext)?;
        let now = Utc::now();
        let mut connection = self.connection.lock().expect("repository mutex poisoned");
        let transaction = connection.transaction()?;
        transaction.execute(
            "UPDATE items SET nonce=?2,ciphertext=?3,key_nonce=?4,wrapped_key=?5,updated_at=?6,version=version+1,sync_status='pending' WHERE id=?1",
            params![id.to_string(), encrypted.nonce, encrypted.ciphertext, encrypted.key_nonce, encrypted.wrapped_key, now.to_rfc3339()],
        )?;
        transaction.execute(
            "UPDATE ai_jobs SET status='complete',next_attempt_at=NULL,last_error=NULL,updated_at=?2 WHERE item_id=?1",
            params![id.to_string(), now.to_rfc3339()],
        )?;
        transaction.execute(
            "INSERT INTO outbox (object_id,operation,created_at) VALUES (?1,'upsert',?2)",
            params![id.to_string(), now.to_rfc3339()],
        )?;
        transaction.commit()?;
        drop(connection);
        self.index_item(&id, &item.content)?;
        item.updated_at = now;
        item.version += 1;
        item.sync_status = "pending".into();
        item.ai_status = "complete".into();
        Ok(item)
    }

    pub fn fail_ai_job(
        &self,
        id: Uuid,
        message: &str,
        retry_after_seconds: i64,
    ) -> Result<(), StorageError> {
        let now = Utc::now();
        let retry_at = now + chrono::Duration::seconds(retry_after_seconds.clamp(1, 86_400));
        let connection = self.connection.lock().expect("repository mutex poisoned");
        if connection.execute(
            "UPDATE ai_jobs SET status='failed',next_attempt_at=?2,last_error=?3,updated_at=?4 WHERE item_id=?1",
            params![id.to_string(), retry_at.to_rfc3339(), message.chars().take(500).collect::<String>(), now.to_rfc3339()],
        )? == 0 {
            return Err(StorageError::AiJobUnavailable);
        }
        Ok(())
    }

    pub fn ai_job_attempts(&self, id: Uuid) -> Result<i64, StorageError> {
        let connection = self.connection.lock().expect("repository mutex poisoned");
        connection
            .query_row(
                "SELECT attempts FROM ai_jobs WHERE item_id=?1",
                [id.to_string()],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(StorageError::AiJobUnavailable)
    }

    pub fn reset_ai_jobs(&self) -> Result<usize, StorageError> {
        let connection = self.connection.lock().expect("repository mutex poisoned");
        Ok(connection.execute(
            "UPDATE ai_jobs SET status='pending',attempts=0,next_attempt_at=NULL,last_error=NULL,updated_at=?1",
            [Utc::now().to_rfc3339()],
        )?)
    }

    pub fn rebuild_search_index(&self, master_key: &MasterKey) -> Result<usize, StorageError> {
        let items = self.list(master_key, true)?;
        let search = self.search.lock().expect("search index mutex poisoned");
        search.execute("DELETE FROM record_search", [])?;
        drop(search);
        let mut indexed = 0;
        for item in items {
            if item.content.ai_metadata.is_some() {
                self.index_item(&item.id, &item.content)?;
                indexed += 1;
            }
        }
        Ok(indexed)
    }

    pub fn search_index(&self, query: &str, limit: usize) -> Result<Vec<Uuid>, StorageError> {
        let terms = query
            .split_whitespace()
            .filter(|term| !term.is_empty())
            .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" AND ");
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        let search = self.search.lock().expect("search index mutex poisoned");
        let mut statement = search.prepare(
            "SELECT item_id FROM record_search WHERE record_search MATCH ?1 ORDER BY rank LIMIT ?2",
        )?;
        statement
            .query_map(params![terms, limit.min(100) as i64], |row| {
                row.get::<_, String>(0)
            })?
            .filter_map(|row| row.ok().and_then(|id| Uuid::parse_str(&id).ok()).map(Ok))
            .collect()
    }

    fn index_item(&self, id: &Uuid, content: &ItemContent) -> Result<(), StorageError> {
        let Some(metadata) = content.ai_metadata.as_ref() else {
            return self.remove_from_index(*id);
        };
        let search = self.search.lock().expect("search index mutex poisoned");
        search.execute(
            "DELETE FROM record_search WHERE item_id=?1",
            [id.to_string()],
        )?;
        search.execute(
            "INSERT INTO record_search (item_id,title,search_text,transcript,tags,markers) VALUES (?1,?2,?3,?4,?5,?6)",
            params![id.to_string(), content.title, metadata.search_text, metadata.transcript.as_deref().unwrap_or_default(), content.tags.join(" "), content.markers.join(" ")],
        )?;
        Ok(())
    }

    fn remove_from_index(&self, id: Uuid) -> Result<(), StorageError> {
        let search = self.search.lock().expect("search index mutex poisoned");
        search.execute(
            "DELETE FROM record_search WHERE item_id=?1",
            [id.to_string()],
        )?;
        Ok(())
    }

    pub fn save_attachment_descriptor(
        &self,
        descriptor: &AttachmentDescriptor,
    ) -> Result<(), StorageError> {
        let connection = self.connection.lock().expect("repository mutex poisoned");
        connection.execute(
            "INSERT INTO attachment_descriptors (id,media_type) VALUES (?1,?2) \
             ON CONFLICT(id) DO UPDATE SET media_type=excluded.media_type",
            params![descriptor.id.to_string(), descriptor.media_type],
        )?;
        Ok(())
    }

    pub fn attachment_descriptor(&self, id: Uuid) -> Result<AttachmentDescriptor, StorageError> {
        let connection = self.connection.lock().expect("repository mutex poisoned");
        connection
            .query_row(
                "SELECT media_type FROM attachment_descriptors WHERE id=?1",
                [id.to_string()],
                |row| {
                    Ok(AttachmentDescriptor {
                        id,
                        media_type: row.get(0)?,
                    })
                },
            )
            .optional()?
            .ok_or(StorageError::NotFound)
    }

    pub fn pending_attachments(
        &self,
        limit: usize,
    ) -> Result<Vec<AttachmentSyncData>, StorageError> {
        let connection = self.connection.lock().expect("repository mutex poisoned");
        let mut statement = connection.prepare(
            "SELECT a.id,a.nonce_prefix,a.key_nonce,a.wrapped_key,a.chunk_bytes,a.ciphertext_bytes, \
                    a.ciphertext_sha256,a.created_at,a.sync_status,d.media_type \
             FROM attachments a JOIN attachment_descriptors d ON d.id=a.id \
             WHERE a.sync_status<>'synced' AND EXISTS(SELECT 1 FROM outbox o WHERE o.object_id=a.id AND o.operation='attachment') \
             ORDER BY a.created_at LIMIT ?1",
        )?;
        statement
            .query_map([limit.clamp(1, 20) as i64], |row| {
                let id = Uuid::parse_str(&row.get::<_, String>(0)?)
                    .map_err(|_| rusqlite::Error::InvalidQuery)?;
                let created = row.get::<_, String>(7)?;
                Ok((
                    id,
                    EncryptedAttachmentHeader {
                        nonce_prefix: row.get(1)?,
                        key_nonce: row.get(2)?,
                        wrapped_key: row.get(3)?,
                        chunk_bytes: row.get(4)?,
                    },
                    row.get::<_, u64>(5)?,
                    row.get::<_, String>(6)?,
                    created,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                ))
            })?
            .map(|row| {
                let (id, header, bytes, sha, created, status, media_type) = row?;
                Ok(AttachmentSyncData {
                    attachment: Attachment {
                        id,
                        ciphertext_bytes: bytes,
                        ciphertext_sha256: sha,
                        created_at: parse_time(&created)?,
                        sync_status: status,
                    },
                    header,
                    descriptor: AttachmentDescriptor { id, media_type },
                })
            })
            .collect()
    }

    pub fn has_attachment(&self, id: Uuid) -> Result<bool, StorageError> {
        let connection = self.connection.lock().expect("repository mutex poisoned");
        connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM attachments WHERE id=?1)",
                [id.to_string()],
                |row| row.get(0),
            )
            .map_err(StorageError::Database)
    }

    pub fn read_attachment_ciphertext_part(
        &self,
        id: Uuid,
        offset: u64,
        length: usize,
    ) -> Result<Vec<u8>, StorageError> {
        use std::io::{Seek, SeekFrom};
        let directory = self
            .attachment_dir
            .as_ref()
            .ok_or(StorageError::AttachmentStorageUnavailable)?;
        let total = self.attachment_ciphertext_bytes(id)?;
        if offset > total || length as u64 > total.saturating_sub(offset) {
            return Err(StorageError::InvalidSyncState);
        }
        let mut file = File::open(directory.join(format!("{id}.blob")))?;
        file.seek(SeekFrom::Start(offset))?;
        let mut bytes = vec![0_u8; length];
        file.read_exact(&mut bytes)?;
        Ok(bytes)
    }

    pub fn mark_attachment_synced(&self, id: Uuid) -> Result<(), StorageError> {
        let mut connection = self.connection.lock().expect("repository mutex poisoned");
        let transaction = connection.transaction()?;
        transaction.execute(
            "UPDATE attachments SET sync_status='synced' WHERE id=?1",
            [id.to_string()],
        )?;
        transaction.execute(
            "DELETE FROM outbox WHERE object_id=?1 AND operation='attachment'",
            [id.to_string()],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn import_encrypted_attachment(
        &self,
        data: &AttachmentSyncData,
        mut ciphertext: impl Read,
    ) -> Result<(), StorageError> {
        let directory = self
            .attachment_dir
            .as_ref()
            .ok_or(StorageError::AttachmentStorageUnavailable)?;
        let temporary = directory.join(format!(
            ".{}-{}.download",
            data.attachment.id,
            Uuid::new_v4()
        ));
        let destination = directory.join(format!("{}.blob", data.attachment.id));
        let mut output = File::create(&temporary)?;
        let mut hasher = Sha256::new();
        let mut total = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = ciphertext.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            output.write_all(&buffer[..read])?;
            hasher.update(&buffer[..read]);
            total += read as u64;
        }
        output.sync_all()?;
        if total != data.attachment.ciphertext_bytes
            || format!("{:x}", hasher.finalize()) != data.attachment.ciphertext_sha256
        {
            drop(output);
            let _ = fs::remove_file(&temporary);
            return Err(StorageError::InvalidSyncState);
        }
        drop(output);
        fs::rename(&temporary, &destination)?;
        let connection = self.connection.lock().expect("repository mutex poisoned");
        connection.execute(
            "INSERT INTO attachments (id,nonce_prefix,key_nonce,wrapped_key,chunk_bytes,ciphertext_bytes,ciphertext_sha256,created_at,sync_status) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'synced') ON CONFLICT(id) DO NOTHING",
            params![data.attachment.id.to_string(), data.header.nonce_prefix, data.header.key_nonce, data.header.wrapped_key,
                data.header.chunk_bytes, data.attachment.ciphertext_bytes, data.attachment.ciphertext_sha256,
                data.attachment.created_at.to_rfc3339()],
        )?;
        connection.execute(
            "INSERT INTO attachment_descriptors (id,media_type) VALUES (?1,?2) ON CONFLICT(id) DO NOTHING",
            params![data.attachment.id.to_string(), data.descriptor.media_type],
        )?;
        Ok(())
    }

    pub fn save_attachment(
        &self,
        master_key: &MasterKey,
        id: Uuid,
        reader: impl Read,
    ) -> Result<Attachment, StorageError> {
        let directory = self
            .attachment_dir
            .as_ref()
            .ok_or(StorageError::AttachmentStorageUnavailable)?;
        let temporary = directory.join(format!(".{id}-{}.partial", Uuid::new_v4()));
        let destination = directory.join(format!("{id}.blob"));
        let mut output = BufWriter::new(File::create(&temporary)?);
        let header = match master_key.encrypt_attachment(id.as_bytes(), reader, &mut output) {
            Ok(header) => header,
            Err(error) => {
                drop(output);
                let _ = fs::remove_file(&temporary);
                return Err(error.into());
            }
        };
        output.flush()?;
        output.get_ref().sync_all()?;
        drop(output);

        let (ciphertext_bytes, ciphertext_sha256) = hash_file(&temporary)?;
        fs::rename(&temporary, &destination)?;
        let now = Utc::now();
        let connection = self.connection.lock().expect("repository mutex poisoned");
        connection.execute(
            "INSERT INTO attachments (id,nonce_prefix,key_nonce,wrapped_key,chunk_bytes,\
             ciphertext_bytes,ciphertext_sha256,created_at,sync_status) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'pending') \
             ON CONFLICT(id) DO UPDATE SET nonce_prefix=excluded.nonce_prefix,\
             key_nonce=excluded.key_nonce,wrapped_key=excluded.wrapped_key,\
             chunk_bytes=excluded.chunk_bytes,ciphertext_bytes=excluded.ciphertext_bytes,\
             ciphertext_sha256=excluded.ciphertext_sha256,sync_status='pending'",
            params![
                id.to_string(),
                header.nonce_prefix,
                header.key_nonce,
                header.wrapped_key,
                header.chunk_bytes,
                ciphertext_bytes,
                ciphertext_sha256,
                now.to_rfc3339()
            ],
        )?;
        connection.execute(
            "INSERT INTO outbox (object_id,operation,created_at) VALUES (?1,'attachment',?2)",
            params![id.to_string(), now.to_rfc3339()],
        )?;
        Ok(Attachment {
            id,
            ciphertext_bytes,
            ciphertext_sha256,
            created_at: now,
            sync_status: "pending".into(),
        })
    }

    pub fn read_attachment(
        &self,
        master_key: &MasterKey,
        id: Uuid,
        writer: impl Write,
    ) -> Result<(), StorageError> {
        let directory = self
            .attachment_dir
            .as_ref()
            .ok_or(StorageError::AttachmentStorageUnavailable)?;
        let connection = self.connection.lock().expect("repository mutex poisoned");
        let header = connection
            .query_row(
                "SELECT nonce_prefix,key_nonce,wrapped_key,chunk_bytes FROM attachments WHERE id=?1",
                [id.to_string()],
                |row| {
                    Ok(EncryptedAttachmentHeader {
                        nonce_prefix: row.get(0)?,
                        key_nonce: row.get(1)?,
                        wrapped_key: row.get(2)?,
                        chunk_bytes: row.get(3)?,
                    })
                },
            )
            .optional()?
            .ok_or(StorageError::NotFound)?;
        drop(connection);
        let input = BufReader::new(File::open(directory.join(format!("{id}.blob")))?);
        master_key
            .decrypt_attachment(id.as_bytes(), &header, input, writer)
            .map_err(StorageError::Crypto)
    }

    pub fn attachment_ciphertext_bytes(&self, id: Uuid) -> Result<u64, StorageError> {
        let connection = self.connection.lock().expect("repository mutex poisoned");
        connection
            .query_row(
                "SELECT ciphertext_bytes FROM attachments WHERE id=?1",
                [id.to_string()],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(StorageError::NotFound)
    }

    pub fn delete_attachment(&self, id: Uuid) -> Result<(), StorageError> {
        let directory = self
            .attachment_dir
            .as_ref()
            .ok_or(StorageError::AttachmentStorageUnavailable)?;
        let connection = self.connection.lock().expect("repository mutex poisoned");
        if connection.execute("DELETE FROM attachments WHERE id=?1", [id.to_string()])? == 0 {
            return Err(StorageError::NotFound);
        }
        match fs::remove_file(directory.join(format!("{id}.blob"))) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    fn store_conflict(&self, change: &SyncChange) -> Result<(), StorageError> {
        let connection = self.connection.lock().expect("repository mutex poisoned");
        connection.execute(
            "INSERT INTO sync_conflicts \
             (object_id,version,device_id,operation,nonce,ciphertext,wrapped_key,client_updated_at) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8) \
             ON CONFLICT(object_id) DO UPDATE SET version=excluded.version,device_id=excluded.device_id, \
             operation=excluded.operation,nonce=excluded.nonce,ciphertext=excluded.ciphertext, \
             wrapped_key=excluded.wrapped_key,client_updated_at=excluded.client_updated_at",
            params![
                change.envelope.object_id.to_string(),
                change.version,
                change.envelope.device_id.to_string(),
                operation_name(&change.envelope.operation),
                change.envelope.nonce,
                change.envelope.ciphertext,
                change.envelope.wrapped_key,
                change.envelope.client_updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    fn stored_conflicts(&self) -> Result<Vec<SyncChange>, StorageError> {
        let connection = self.connection.lock().expect("repository mutex poisoned");
        let mut statement = connection.prepare(
            "SELECT object_id,version,device_id,operation,nonce,ciphertext,wrapped_key,client_updated_at \
             FROM sync_conflicts ORDER BY client_updated_at DESC",
        )?;
        statement
            .query_map([], |row| {
                let operation = match row.get::<_, String>(3)?.as_str() {
                    "upsert" => SyncOperation::Upsert,
                    "delete" => SyncOperation::Delete,
                    _ => return Err(rusqlite::Error::InvalidQuery),
                };
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    operation,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            })?
            .map(|row| {
                let (id, version, device, operation, nonce, ciphertext, wrapped_key, updated) =
                    row?;
                Ok(SyncChange {
                    cursor: 0,
                    version,
                    envelope: EncryptedEnvelope {
                        object_id: Uuid::parse_str(&id)
                            .map_err(|_| StorageError::InvalidSyncState)?,
                        object_type: "item".into(),
                        device_id: Uuid::parse_str(&device)
                            .map_err(|_| StorageError::InvalidSyncState)?,
                        base_version: version - 1,
                        operation,
                        ciphertext,
                        nonce,
                        wrapped_key,
                        client_updated_at: parse_time(&updated)?,
                    },
                    server_created_at: DateTime::UNIX_EPOCH,
                })
            })
            .collect()
    }

    fn apply_remote_force(
        &self,
        master_key: &MasterKey,
        change: &SyncChange,
        discard_local: bool,
    ) -> Result<(), StorageError> {
        let id = change.envelope.object_id;
        let payload = if change.envelope.operation == SyncOperation::Upsert {
            Some(remote_payload(master_key, change)?)
        } else {
            None
        };
        let content_fingerprint = payload
            .as_ref()
            .map(|payload| content_fingerprint(&payload.content))
            .transpose()?;
        let encrypted_content = payload
            .as_ref()
            .map(|payload| {
                master_key
                    .encrypt(id.as_bytes(), &serde_json::to_vec(&payload.content)?)
                    .map_err(StorageError::Crypto)
            })
            .transpose()?;
        let mut connection = self.connection.lock().expect("repository mutex poisoned");
        let transaction = connection.transaction()?;
        if discard_local {
            transaction.execute("DELETE FROM outbox WHERE object_id=?1", [id.to_string()])?;
        }
        if let (Some(payload), Some(encrypted), Some(fingerprint)) =
            (&payload, encrypted_content, content_fingerprint)
        {
            let local_version = transaction
                .query_row(
                    "SELECT version FROM items WHERE id=?1",
                    [id.to_string()],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?
                .unwrap_or(0)
                + 1;
            transaction.execute(
                "INSERT INTO items \
                 (id,nonce,ciphertext,key_nonce,wrapped_key,created_at,updated_at,version,archived,pinned,sync_status) \
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'synced') \
                 ON CONFLICT(id) DO UPDATE SET nonce=excluded.nonce,ciphertext=excluded.ciphertext, \
                 key_nonce=excluded.key_nonce,wrapped_key=excluded.wrapped_key,updated_at=excluded.updated_at, \
                 version=excluded.version,archived=excluded.archived,pinned=excluded.pinned,sync_status='synced'",
                params![id.to_string(), encrypted.nonce, encrypted.ciphertext, encrypted.key_nonce,
                    encrypted.wrapped_key, payload.created_at.to_rfc3339(), payload.updated_at.to_rfc3339(),
                    local_version, payload.archived, payload.pinned],
            )?;
            let ai_status = if payload.content.ai_metadata.is_some() {
                "complete"
            } else {
                "pending"
            };
            transaction.execute(
                "INSERT INTO ai_jobs (item_id,content_fingerprint,status,attempts,next_attempt_at,last_error,updated_at) \
                 VALUES (?1,?2,?3,0,NULL,NULL,?4) \
                 ON CONFLICT(item_id) DO UPDATE SET content_fingerprint=excluded.content_fingerprint, \
                 status=excluded.status,attempts=0,next_attempt_at=NULL,last_error=NULL,updated_at=excluded.updated_at",
                params![id.to_string(), fingerprint, ai_status, Utc::now().to_rfc3339()],
            )?;
        } else {
            transaction.execute("DELETE FROM items WHERE id=?1", [id.to_string()])?;
        }
        transaction.execute(
            "INSERT INTO sync_state (object_id,server_version) VALUES (?1,?2) \
             ON CONFLICT(object_id) DO UPDATE SET server_version=excluded.server_version",
            params![id.to_string(), change.version],
        )?;
        transaction.execute(
            "DELETE FROM sync_conflicts WHERE object_id=?1",
            [id.to_string()],
        )?;
        transaction.commit()?;
        drop(connection);
        if let Some(payload) = payload {
            if payload.content.ai_metadata.is_some() {
                self.index_item(&id, &payload.content)?;
            } else {
                self.remove_from_index(id)?;
            }
        } else {
            self.remove_from_index(id)?;
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct PackedKey<'a> {
    key_nonce: &'a str,
    wrapped_key: &'a str,
}

#[derive(Deserialize)]
struct OwnedPackedKey {
    key_nonce: String,
    wrapped_key: String,
}

fn pack_key(encrypted: &EncryptedRecord) -> Result<String, StorageError> {
    Ok(serde_json::to_string(&PackedKey {
        key_nonce: &encrypted.key_nonce,
        wrapped_key: &encrypted.wrapped_key,
    })?)
}

fn remote_payload(
    master_key: &MasterKey,
    change: &SyncChange,
) -> Result<SyncedItemPayload, StorageError> {
    let key: OwnedPackedKey = serde_json::from_str(&change.envelope.wrapped_key)?;
    let encrypted = EncryptedRecord {
        nonce: change.envelope.nonce.clone(),
        ciphertext: change.envelope.ciphertext.clone(),
        key_nonce: key.key_nonce,
        wrapped_key: key.wrapped_key,
    };
    Ok(serde_json::from_slice(&master_key.decrypt(
        change.envelope.object_id.as_bytes(),
        &encrypted,
    )?)?)
}

fn remote_item(master_key: &MasterKey, change: &SyncChange) -> Result<Item, StorageError> {
    let payload = remote_payload(master_key, change)?;
    Ok(Item {
        id: change.envelope.object_id,
        content: payload.content.clone(),
        created_at: payload.created_at,
        updated_at: payload.updated_at,
        version: change.version,
        archived: payload.archived,
        pinned: payload.pinned,
        sync_status: "conflict".into(),
        ai_status: if payload.content.ai_metadata.is_some() {
            "complete"
        } else {
            "pending"
        }
        .into(),
    })
}

fn operation_name(operation: &SyncOperation) -> &'static str {
    match operation {
        SyncOperation::Upsert => "upsert",
        SyncOperation::Delete => "delete",
    }
}

fn hash_file(path: &Path) -> Result<(u64, String), std::io::Error> {
    let mut input = BufReader::new(File::open(path)?);
    let mut hasher = Sha256::new();
    let mut bytes = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        bytes += read as u64;
    }
    Ok((bytes, format!("{:x}", hasher.finalize())))
}

fn content_fingerprint(content: &ItemContent) -> Result<String, serde_json::Error> {
    let mut indexable = content.clone();
    indexable.ai_metadata = None;
    let bytes = serde_json::to_vec(&indexable)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

struct StoredItem {
    encrypted: EncryptedRecord,
    created_at: String,
    updated_at: String,
    version: i64,
    archived: bool,
    pinned: bool,
    sync_status: String,
    ai_status: String,
}

fn decrypt_item(
    master_key: &MasterKey,
    id: Uuid,
    stored: StoredItem,
) -> Result<Item, StorageError> {
    let plaintext = master_key.decrypt(id.as_bytes(), &stored.encrypted)?;
    Ok(Item {
        id,
        content: serde_json::from_slice(&plaintext)?,
        created_at: parse_time(&stored.created_at)?,
        updated_at: parse_time(&stored.updated_at)?,
        version: stored.version,
        archived: stored.archived,
        pinned: stored.pinned,
        sync_status: stored.sync_status,
        ai_status: stored.ai_status,
    })
}

fn parse_time(value: &str) -> Result<DateTime<Utc>, StorageError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| StorageError::InvalidTimestamp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn private_content() -> ItemContent {
        ItemContent {
            title: "绝密标题".into(),
            markdown: "# 私密 Markdown\n服务器密码提示".into(),
            source_type: SourceType::Text,
            tags: vec!["私密标签".into()],
            markers: vec!["账目".into()],
            attachment_ids: vec![],
            ai_metadata: None,
        }
    }

    #[test]
    fn encrypted_crud_round_trip_and_versions() {
        let repository = Repository::open_in_memory().unwrap();
        let key = MasterKey::generate();
        let id = Uuid::new_v4();
        let mut saved = repository
            .save(
                &key,
                SaveItem {
                    id,
                    content: private_content(),
                    archived: false,
                    pinned: false,
                },
            )
            .unwrap();
        assert_eq!(saved.version, 1);
        saved.content.markdown.push_str("\n更新");
        let updated = repository
            .save(
                &key,
                SaveItem {
                    id,
                    content: saved.content.clone(),
                    archived: false,
                    pinned: true,
                },
            )
            .unwrap();
        assert_eq!(updated.version, 2);
        assert_eq!(repository.get(&key, id).unwrap().content, saved.content);
        assert_eq!(repository.list(&key, false).unwrap().len(), 1);
        repository.delete(id).unwrap();
        assert!(matches!(
            repository.get(&key, id),
            Err(StorageError::NotFound)
        ));
    }

    #[test]
    fn database_file_contains_no_private_content() {
        let directory = TempDir::new().unwrap();
        let path = directory.path().join("snapline.db");
        let repository = Repository::open(&path).unwrap();
        let key = MasterKey::generate();
        repository
            .save(
                &key,
                SaveItem {
                    id: Uuid::new_v4(),
                    content: private_content(),
                    archived: false,
                    pinned: false,
                },
            )
            .unwrap();
        drop(repository);
        let bytes = fs::read(path).unwrap();
        let dump = String::from_utf8_lossy(&bytes);
        for private in [
            "绝密标题",
            "私密 Markdown",
            "服务器密码提示",
            "私密标签",
            "账目",
        ] {
            assert!(!dump.contains(private), "database leaked {private}");
        }
    }

    #[test]
    fn wrong_master_key_cannot_read_record() {
        let repository = Repository::open_in_memory().unwrap();
        let id = Uuid::new_v4();
        repository
            .save(
                &MasterKey::generate(),
                SaveItem {
                    id,
                    content: private_content(),
                    archived: false,
                    pinned: false,
                },
            )
            .unwrap();
        assert!(matches!(
            repository.get(&MasterKey::generate(), id),
            Err(StorageError::Crypto(_))
        ));
    }

    #[test]
    fn encrypted_attachment_round_trip_leaves_no_plaintext_on_disk() {
        let directory = TempDir::new().unwrap();
        let repository = Repository::open(directory.path().join("snapline.db")).unwrap();
        let key = MasterKey::generate();
        let id = Uuid::new_v4();
        let plaintext = b"private audio transcript and binary payload".repeat(80_000);
        let stored = repository
            .save_attachment(&key, id, plaintext.as_slice())
            .unwrap();
        assert_eq!(
            repository.attachment_ciphertext_bytes(id).unwrap(),
            stored.ciphertext_bytes
        );
        assert!(stored.ciphertext_bytes > plaintext.len() as u64);
        assert_eq!(stored.ciphertext_sha256.len(), 64);
        let encrypted = fs::read(
            directory
                .path()
                .join("attachments")
                .join(format!("{id}.blob")),
        )
        .unwrap();
        assert!(
            !encrypted
                .windows(32)
                .any(|window| window == &plaintext[..32])
        );
        let mut restored = Vec::new();
        repository.read_attachment(&key, id, &mut restored).unwrap();
        assert_eq!(restored, plaintext);
    }

    #[test]
    fn ai_queue_encrypts_metadata_and_rebuilds_memory_only_fts() {
        let directory = TempDir::new().unwrap();
        let database = directory.path().join("snapline.db");
        let repository = Repository::open(&database).unwrap();
        let key = MasterKey::generate();
        let id = Uuid::new_v4();
        let saved = repository
            .save(
                &key,
                SaveItem {
                    id,
                    content: ItemContent {
                        title: "Private launch note".into(),
                        markdown: "The cobalt launch happens next Tuesday".into(),
                        ..Default::default()
                    },
                    archived: false,
                    pinned: false,
                },
            )
            .unwrap();
        assert_eq!(saved.ai_status, "pending");
        let claimed = repository.claim_ai_jobs(&key, 10).unwrap();
        assert_eq!(claimed.len(), 1);
        assert_eq!(claimed[0].ai_status, "processing");
        assert_eq!(repository.ai_job_attempts(id).unwrap(), 1);
        let completed = repository
            .complete_ai_job(
                &key,
                id,
                AiMetadata {
                    summary: "Confidential launch summary".into(),
                    transcript: None,
                    topics: vec!["launch".into()],
                    entities: vec![],
                    keywords: vec!["cobalt".into()],
                    people: vec![],
                    locations: vec![],
                    event_time: None,
                    language: "en".into(),
                    suggested_tags: vec![],
                    suggested_markers: vec![],
                    search_text: "cobalt launch schedule".into(),
                },
            )
            .unwrap();
        assert_eq!(completed.ai_status, "complete");
        assert_eq!(repository.search_index("cobalt", 10).unwrap(), vec![id]);
        assert_eq!(repository.rebuild_search_index(&key).unwrap(), 1);
        assert_eq!(repository.search_index("schedule", 10).unwrap(), vec![id]);
        assert_eq!(repository.reset_ai_jobs().unwrap(), 1);
        assert_eq!(repository.claim_ai_jobs(&key, 10).unwrap().len(), 1);

        drop(repository);
        let disk = fs::read(database).unwrap();
        let text = String::from_utf8_lossy(&disk);
        assert!(!text.contains("Confidential launch summary"));
        assert!(!text.contains("cobalt launch schedule"));
    }

    #[test]
    fn opening_a_pre_ai_database_backfills_historical_jobs() {
        let directory = TempDir::new().unwrap();
        let database = directory.path().join("legacy.db");
        let key = MasterKey::generate();
        let id = Uuid::new_v4();
        let content = private_content();
        let encrypted = key
            .encrypt(id.as_bytes(), &serde_json::to_vec(&content).unwrap())
            .unwrap();
        let now = Utc::now().to_rfc3339();
        let connection = Connection::open(&database).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE items (
                    id TEXT PRIMARY KEY, nonce TEXT NOT NULL, ciphertext TEXT NOT NULL,
                    key_nonce TEXT NOT NULL, wrapped_key TEXT NOT NULL, created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL, version INTEGER NOT NULL, archived INTEGER NOT NULL,
                    pinned INTEGER NOT NULL, sync_status TEXT NOT NULL
                );",
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO items VALUES (?1,?2,?3,?4,?5,?6,?6,1,0,0,'pending')",
                params![
                    id.to_string(),
                    encrypted.nonce,
                    encrypted.ciphertext,
                    encrypted.key_nonce,
                    encrypted.wrapped_key,
                    now
                ],
            )
            .unwrap();
        drop(connection);

        let repository = Repository::open(&database).unwrap();
        let jobs = repository.claim_ai_jobs(&key, 10).unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, id);
        assert_eq!(jobs[0].content, content);
    }

    #[test]
    fn encrypted_attachment_rejects_wrong_key_and_tampering() {
        let directory = TempDir::new().unwrap();
        let repository = Repository::open(directory.path().join("snapline.db")).unwrap();
        let key = MasterKey::generate();
        let id = Uuid::new_v4();
        repository
            .save_attachment(&key, id, b"private image".as_slice())
            .unwrap();
        assert!(
            repository
                .read_attachment(&MasterKey::generate(), id, Vec::new())
                .is_err()
        );
        let path = directory
            .path()
            .join("attachments")
            .join(format!("{id}.blob"));
        let mut encrypted = fs::read(&path).unwrap();
        encrypted[9] ^= 1;
        fs::write(path, encrypted).unwrap();
        assert!(repository.read_attachment(&key, id, Vec::new()).is_err());
    }

    #[test]
    fn durable_sync_round_trip_and_conflict_resolution_use_server_versions() {
        let first = Repository::open_in_memory().unwrap();
        let second = Repository::open_in_memory().unwrap();
        let key = MasterKey::generate();
        let first_device = Uuid::new_v4();
        let second_device = Uuid::new_v4();
        let id = Uuid::new_v4();
        first
            .save(
                &key,
                SaveItem {
                    id,
                    content: ItemContent {
                        title: "first device".into(),
                        ..Default::default()
                    },
                    archived: false,
                    pinned: true,
                },
            )
            .unwrap();
        let pending = first.pending_sync_changes(&key, first_device, 100).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].envelope.base_version, 0);
        let initial = SyncChange {
            cursor: 1,
            version: 1,
            envelope: pending[0].envelope.clone(),
            server_created_at: Utc::now(),
        };
        first
            .complete_sync_push(&[(id, 1, pending[0].sequence)])
            .unwrap();
        assert!(
            first
                .pending_sync_changes(&key, first_device, 100)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            second
                .apply_remote_change(&key, second_device, &initial)
                .unwrap(),
            RemoteApply::Applied
        );
        assert_eq!(second.get(&key, id).unwrap().content.title, "first device");
        assert!(second.get(&key, id).unwrap().pinned);

        second
            .save(
                &key,
                SaveItem {
                    id,
                    content: ItemContent {
                        title: "second edit".into(),
                        ..Default::default()
                    },
                    archived: false,
                    pinned: false,
                },
            )
            .unwrap();
        assert_eq!(
            second.pending_sync_changes(&key, second_device, 1).unwrap()[0]
                .envelope
                .base_version,
            1
        );
        first
            .save(
                &key,
                SaveItem {
                    id,
                    content: ItemContent {
                        title: "first edit".into(),
                        ..Default::default()
                    },
                    archived: true,
                    pinned: false,
                },
            )
            .unwrap();
        let first_pending = first.pending_sync_changes(&key, first_device, 1).unwrap();
        let concurrent = SyncChange {
            cursor: 2,
            version: 2,
            envelope: first_pending[0].envelope.clone(),
            server_created_at: Utc::now(),
        };
        assert_eq!(
            second
                .apply_remote_change(&key, second_device, &concurrent)
                .unwrap(),
            RemoteApply::Conflict
        );
        let conflicts = second.list_sync_conflicts(&key).unwrap();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(
            conflicts[0].local.as_ref().unwrap().content.title,
            "second edit"
        );
        assert_eq!(
            conflicts[0].remote.as_ref().unwrap().content.title,
            "first edit"
        );
        second
            .resolve_sync_conflict(&key, id, ConflictChoice::Local)
            .unwrap();
        assert!(second.list_sync_conflicts(&key).unwrap().is_empty());
        assert_eq!(
            second.pending_sync_changes(&key, second_device, 1).unwrap()[0]
                .envelope
                .base_version,
            2
        );

        assert_eq!(
            second
                .apply_remote_change(&key, second_device, &concurrent)
                .unwrap(),
            RemoteApply::Ignored
        );
        let newer = SyncChange {
            cursor: 3,
            version: 3,
            ..concurrent
        };
        assert_eq!(
            second
                .apply_remote_change(&key, second_device, &newer)
                .unwrap(),
            RemoteApply::Conflict
        );
        second
            .resolve_sync_conflict(&key, id, ConflictChoice::Remote)
            .unwrap();
        assert_eq!(second.get(&key, id).unwrap().content.title, "first edit");
        assert!(second.get(&key, id).unwrap().archived);
        assert!(
            second
                .pending_sync_changes(&key, second_device, 100)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn encrypted_attachment_sync_streams_parts_and_imports_verified_ciphertext() {
        let first_dir = TempDir::new().unwrap();
        let second_dir = TempDir::new().unwrap();
        let first = Repository::open(first_dir.path().join("snapline.db")).unwrap();
        let second = Repository::open(second_dir.path().join("snapline.db")).unwrap();
        let key = MasterKey::generate();
        let id = Uuid::new_v4();
        let plaintext = vec![42_u8; 2 * 1024 * 1024 + 321];
        first
            .save_attachment(&key, id, plaintext.as_slice())
            .unwrap();
        first
            .save_attachment_descriptor(&AttachmentDescriptor {
                id,
                media_type: "video/mp4".into(),
            })
            .unwrap();
        let data = first.pending_attachments(10).unwrap().pop().unwrap();
        let first_part = first
            .read_attachment_ciphertext_part(id, 0, 1024 * 1024)
            .unwrap();
        let remainder = first
            .read_attachment_ciphertext_part(
                id,
                first_part.len() as u64,
                data.attachment.ciphertext_bytes as usize - first_part.len(),
            )
            .unwrap();
        let mut ciphertext = first_part;
        ciphertext.extend_from_slice(&remainder);
        second
            .import_encrypted_attachment(&data, ciphertext.as_slice())
            .unwrap();
        let mut restored = Vec::new();
        second.read_attachment(&key, id, &mut restored).unwrap();
        assert_eq!(restored, plaintext);
        assert_eq!(
            second.attachment_descriptor(id).unwrap().media_type,
            "video/mp4"
        );
        first.mark_attachment_synced(id).unwrap();
        assert!(first.pending_attachments(10).unwrap().is_empty());

        ciphertext[10] ^= 1;
        let third_dir = TempDir::new().unwrap();
        let third = Repository::open(third_dir.path().join("snapline.db")).unwrap();
        assert!(
            third
                .import_encrypted_attachment(&data, ciphertext.as_slice())
                .is_err()
        );
    }

    #[test]
    fn own_echo_after_lost_push_response_rebases_without_overwriting_newer_local_edit() {
        let repository = Repository::open_in_memory().unwrap();
        let key = MasterKey::generate();
        let device = Uuid::new_v4();
        let id = Uuid::new_v4();
        repository
            .save(
                &key,
                SaveItem {
                    id,
                    content: ItemContent {
                        title: "sent version".into(),
                        ..Default::default()
                    },
                    archived: false,
                    pinned: false,
                },
            )
            .unwrap();
        let sent = repository.pending_sync_changes(&key, device, 1).unwrap()[0]
            .envelope
            .clone();
        repository
            .save(
                &key,
                SaveItem {
                    id,
                    content: ItemContent {
                        title: "newer local edit".into(),
                        ..Default::default()
                    },
                    archived: false,
                    pinned: false,
                },
            )
            .unwrap();
        let echo = SyncChange {
            cursor: 1,
            version: 1,
            envelope: sent,
            server_created_at: Utc::now(),
        };
        assert_eq!(
            repository.apply_remote_change(&key, device, &echo).unwrap(),
            RemoteApply::Ignored
        );
        assert_eq!(
            repository.get(&key, id).unwrap().content.title,
            "newer local edit"
        );
        assert_eq!(
            repository.pending_sync_changes(&key, device, 1).unwrap()[0]
                .envelope
                .base_version,
            1
        );
    }
}
