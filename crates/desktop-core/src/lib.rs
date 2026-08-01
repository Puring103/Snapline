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

pub struct Repository {
    connection: Mutex<Connection>,
    attachment_dir: Option<PathBuf>,
}

impl Repository {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let path = path.as_ref();
        let connection = Connection::open(path)?;
        connection.execute_batch(SCHEMA)?;
        let attachment_dir = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("attachments");
        fs::create_dir_all(&attachment_dir)?;
        Ok(Self {
            connection: Mutex::new(connection),
            attachment_dir: Some(attachment_dir),
        })
    }

    pub fn open_in_memory() -> Result<Self, StorageError> {
        let connection = Connection::open_in_memory()?;
        connection.execute_batch(SCHEMA)?;
        Ok(Self {
            connection: Mutex::new(connection),
            attachment_dir: None,
        })
    }

    pub fn save(&self, master_key: &MasterKey, input: SaveItem) -> Result<Item, StorageError> {
        let now = Utc::now();
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
        transaction.commit()?;
        Ok(Item {
            id: input.id,
            content: input.content,
            created_at,
            updated_at: now,
            version,
            archived: input.archived,
            pinned: input.pinned,
            sync_status: "pending".into(),
        })
    }

    pub fn get(&self, master_key: &MasterKey, id: Uuid) -> Result<Item, StorageError> {
        let connection = self.connection.lock().expect("repository mutex poisoned");
        let stored = connection.query_row(
            "SELECT nonce,ciphertext,key_nonce,wrapped_key,created_at,updated_at,version,archived,pinned,sync_status \
             FROM items WHERE id=?1",
            [id.to_string()],
            |row| Ok(StoredItem { encrypted: EncryptedRecord { nonce: row.get(0)?, ciphertext: row.get(1)?,
                key_nonce: row.get(2)?, wrapped_key: row.get(3)? }, created_at: row.get(4)?,
                updated_at: row.get(5)?, version: row.get(6)?, archived: row.get(7)?, pinned: row.get(8)?,
                sync_status: row.get(9)? }),
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
            "SELECT id,nonce,ciphertext,key_nonce,wrapped_key,created_at,updated_at,version,archived,pinned,sync_status \
             FROM items WHERE (?1 OR archived=0) ORDER BY pinned DESC,updated_at DESC",
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

struct StoredItem {
    encrypted: EncryptedRecord,
    created_at: String,
    updated_at: String,
    version: i64,
    archived: bool,
    pinned: bool,
    sync_status: String,
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
}
