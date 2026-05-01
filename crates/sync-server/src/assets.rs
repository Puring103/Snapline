use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use std::path::{Path, PathBuf};

#[async_trait]
pub trait AssetStore: Send + Sync {
    async fn put(&self, key: &str, bytes: Bytes) -> Result<()>;
    async fn get(&self, key: &str) -> Result<Bytes>;
}

#[derive(Debug, Clone)]
pub struct LocalFsAssetStore {
    root: PathBuf,
}

impl LocalFsAssetStore {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    fn resolve(&self, key: &str) -> PathBuf {
        self.root.join(key.replace('\\', "/"))
    }
}

#[async_trait]
impl AssetStore for LocalFsAssetStore {
    async fn put(&self, key: &str, bytes: Bytes) -> Result<()> {
        let path = self.resolve(key);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(path, bytes).await?;
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Bytes> {
        Ok(Bytes::from(tokio::fs::read(self.resolve(key)).await?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_store_puts_and_gets_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let store = LocalFsAssetStore::new(dir.path());
        store
            .put("accounts/a/notes/n/image.png", Bytes::from_static(b"png"))
            .await
            .unwrap();

        let loaded = store.get("accounts/a/notes/n/image.png").await.unwrap();
        assert_eq!(loaded, Bytes::from_static(b"png"));
    }
}
