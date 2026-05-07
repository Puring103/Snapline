use anyhow::Result;
use snapline_platform::AppPaths;
use snapline_storage::NoteRepository;
use std::fs;

pub struct AppCore {
    pub(crate) repo: NoteRepository,
    pub(crate) paths: AppPaths,
    /// DEK（数据加密密钥），仅在内存中持有，从不落盘。登录/注册后设置。
    pub(crate) dek: Option<[u8; 32]>,
}

impl AppCore {
    pub fn open(paths: AppPaths) -> Result<Self> {
        fs::create_dir_all(&paths.data_dir)?;
        let repo = NoteRepository::open(&paths.db_path)?;
        Ok(Self { repo, paths, dek: None })
    }

    pub fn with_repo(paths: AppPaths, repo: NoteRepository) -> Self {
        Self { repo, paths, dek: None }
    }

    pub fn data_dir(&self) -> &std::path::Path {
        &self.paths.data_dir
    }
}
