/// 应用核心层：协调存储、平台路径和同步队列，向 UI 层（Tauri）暴露高层操作。
///
/// `AppCore` 是整个应用的业务门面，所有笔记编辑、资源保存、账户管理操作都经过此处。
/// 它不直接处理网络 IO，同步由 `sync-client` crate 中的 processor 单独负责。
use anyhow::{bail, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use snapline_domain::{
    crypto, AssetId, AssetMetadata, AssetRef, AssetUploadPayload, Note, NoteChangePayload, NoteId,
    NoteSummary, SyncOpType, SyncPayload,
};
use snapline_platform::AppPaths;
use snapline_storage::NoteRepository;
use std::fs;

/// 存储全局快捷键设置的 key。
const OPEN_SHORTCUT_KEY: &str = "open_shortcut";
/// 默认全局快捷键。
const DEFAULT_OPEN_SHORTCUT: &str = "Ctrl+Shift+Space";

/// 应用核心结构，持有数据库连接、路径信息和内存中的数据加密密钥。
pub struct AppCore {
    repo: NoteRepository,
    paths: AppPaths,
    /// DEK（数据加密密钥），仅在内存中持有，从不落盘。登录/注册后设置。
    dek: Option<[u8; 32]>,
}

/// 启动时一次性下发给前端的初始状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapState {
    /// 当前账户（或匿名）最近的笔记列表。
    pub notes: Vec<NoteSummary>,
    /// 空白草稿，供前端立即展示编辑界面。
    pub current: Note,
    /// 应用数据目录路径（用于前端展示或调试）。
    pub data_dir: String,
}

/// 前端展示同步账户状态所需的精简信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncAccountState {
    pub account_id: Option<String>,
    pub device_id: String,
    pub server_base_url: Option<String>,
    /// 是否已持有有效的 access token。
    pub is_logged_in: bool,
}

impl AppCore {
    /// 打开（或创建）应用数据目录和数据库，返回初始化好的 AppCore。
    pub fn open(paths: AppPaths) -> Result<Self> {
        fs::create_dir_all(&paths.data_dir)?;
        let repo = NoteRepository::open(&paths.db_path)?;
        Ok(Self { repo, paths, dek: None })
    }

    /// 使用外部传入的 repo 构造 AppCore（测试用）。
    pub fn with_repo(paths: AppPaths, repo: NoteRepository) -> Self {
        Self { repo, paths, dek: None }
    }

    /// 返回启动初始状态：笔记列表 + 空白草稿。草稿不写入数据库。
    pub fn bootstrap(&self) -> Result<BootstrapState> {
        let owner = self.current_account_id()?;
        let notes = self.repo.list_recent_for_owner(50, owner.as_deref())?;
        let current = Note::draft(Utc::now());
        Ok(BootstrapState {
            notes,
            current,
            data_dir: self.paths.data_dir.to_string_lossy().to_string(),
        })
    }

    /// 创建一条新的空白笔记并持久化，归属当前账户（未登录则为匿名）。
    pub fn create_note(&self) -> Result<Note> {
        let owner = self.current_account_id()?;
        self.repo.create_note(Utc::now(), owner.as_deref())
    }

    /// 按 ID 获取笔记，校验 owner 一致性。
    pub fn get_note(&self, id: &NoteId) -> Result<Note> {
        let owner = self.current_account_id()?;
        self.repo.get_note_for_owner(id, owner.as_deref())
    }

    /// 保存笔记内容，写入数据库后入队同步变更。
    pub fn save_note(
        &self,
        id: &NoteId,
        title: &str,
        content_md: &str,
        pinned: bool,
    ) -> Result<Note> {
        let owner = self.current_account_id()?;
        // 若笔记已存在，先验证 owner，防止跨账户写入
        if self.repo.note_exists(id)? {
            self.repo.get_note_for_owner(id, owner.as_deref())?;
        }
        let note =
            self.repo
                .save_note(id, title, content_md, pinned, Utc::now(), owner.as_deref())?;
        self.enqueue_note_change(&note, SyncOpType::UpsertNote, note.server_version)?;
        Ok(note)
    }

    /// 仅更新标题，保持正文和置顶状态不变。
    pub fn set_note_title(&self, id: &NoteId, title: &str) -> Result<Note> {
        self.get_note(id)?;
        let note = self.repo.update_note_title(id, title, Utc::now())?;
        self.enqueue_note_change(&note, SyncOpType::UpsertNote, note.server_version)?;
        Ok(note)
    }

    /// 更新笔记置顶状态。
    pub fn set_note_pinned(&self, id: &NoteId, pinned: bool) -> Result<Note> {
        self.get_note(id)?;
        let note = self.repo.set_pinned(id, pinned, Utc::now())?;
        self.enqueue_note_change(&note, SyncOpType::UpsertNote, note.server_version)?;
        Ok(note)
    }

    /// 软删除笔记并入队 DeleteNote，返回更新后的笔记列表。
    pub fn delete_note(&self, id: &NoteId) -> Result<Vec<NoteSummary>> {
        let owner = self.current_account_id()?;
        let existing = self.repo.get_note_for_owner(id, owner.as_deref())?;
        self.repo.soft_delete(id, Utc::now())?;
        let deleted = self.repo.get_note_for_owner(id, owner.as_deref())?;
        // 以删除前的 server_version 作为 base_version，确保服务端能正确检测冲突
        self.enqueue_note_change(&deleted, SyncOpType::DeleteNote, existing.server_version)?;
        self.repo.list_recent_for_owner(50, owner.as_deref())
    }

    /// 将 PNG 字节保存到磁盘，并为已登录账户入队资源上传任务。
    ///
    /// 返回 `AssetRef`，其中 `markdown_path` 可直接插入 Markdown 正文，
    /// `asset_url` 可供 WebView 渲染预览。
    pub fn save_png_asset(&self, note_id: &NoteId, png_bytes: &[u8]) -> Result<AssetRef> {
        if png_bytes.is_empty() {
            bail!("image bytes are empty");
        }
        let note = self.get_note(note_id)?;
        let asset_id = AssetId::new();
        let dir = self.paths.note_asset_dir(note_id);
        fs::create_dir_all(&dir)?;
        let path = self.paths.note_asset_path(note_id, &asset_id, "png");
        fs::write(path, png_bytes)?;
        let markdown_path = self.paths.markdown_asset_path(note_id, &asset_id, "png");
        let mut hasher = Sha256::new();
        hasher.update(png_bytes);
        let sha256 = format!("{:x}", hasher.finalize());
        let payload = SyncPayload::Asset(AssetUploadPayload {
            asset_id: asset_id.clone(),
            note_id: note_id.clone(),
            content_type: "image/png".to_string(),
            byte_size: png_bytes.len() as i64,
            sha256,
            markdown_path: markdown_path.clone(),
        });
        if let Some(account_id) = note.owner_account_id.as_deref() {
            self.repo.enqueue_change(
                Some(account_id),
                note_id,
                SyncOpType::AssetUpload,
                0,
                &payload,
                Utc::now(),
            )?;
        }
        Ok(AssetRef {
            markdown_path: markdown_path.clone(),
            filesystem_path: self
                .paths
                .note_asset_path(note_id, &asset_id, "png")
                .to_string_lossy()
                .to_string(),
            asset_url: self.paths.markdown_asset_url(&markdown_path),
        })
    }

    /// 将 Markdown 相对路径转换为 `asset://` URL（供前端渲染已有图片）。
    pub fn resolve_asset_url(&self, markdown_path: &str) -> String {
        self.paths.markdown_asset_url(markdown_path)
    }

    /// 将 Markdown 相对路径转换为磁盘绝对路径。
    pub fn resolve_asset_path(&self, markdown_path: &str) -> std::path::PathBuf {
        self.paths.resolve_markdown_asset_path(markdown_path)
    }

    /// 读取全局快捷键设置，未设置时返回默认值。
    pub fn get_open_shortcut(&self) -> Result<String> {
        Ok(self
            .repo
            .get_setting(OPEN_SHORTCUT_KEY)?
            .unwrap_or_else(|| DEFAULT_OPEN_SHORTCUT.to_string()))
    }

    /// 持久化全局快捷键设置。
    pub fn set_open_shortcut(&self, shortcut: &str) -> Result<()> {
        self.repo.set_setting(OPEN_SHORTCUT_KEY, Some(shortcut))
    }

    /// 返回前端展示登录状态所需的精简同步信息。
    pub fn sync_account_state(&self) -> Result<SyncAccountState> {
        let state = self.repo.get_or_create_sync_state()?;
        Ok(SyncAccountState {
            account_id: state.account_id,
            device_id: state.device_id,
            server_base_url: state.server_base_url,
            is_logged_in: state.access_token.is_some(),
        })
    }

    /// 登录成功后保存服务端地址、账户 ID 和 access token。
    ///
    /// 若服务端返回了 `kek_salt` 和 `encrypted_dek`，则用密码派生 KEK、解包 DEK 并保存至内存。
    pub fn save_sync_login(
        &mut self,
        server_base_url: &str,
        account_id: &str,
        access_token: &str,
        password: Option<&str>,
        kek_salt: Option<&str>,
        encrypted_dek: Option<&str>,
    ) -> Result<SyncAccountState> {
        let mut state = self.repo.get_or_create_sync_state()?;
        state.server_base_url = Some(server_base_url.to_string());
        state.account_id = Some(account_id.to_string());
        state.access_token = Some(access_token.to_string());
        state.kek_salt = kek_salt.map(str::to_string);
        state.encrypted_dek = encrypted_dek.map(str::to_string);
        self.repo.save_sync_state(&state)?;
        // 若具备全部 E2EE 材料，立即解包 DEK 存入内存
        if let (Some(pw), Some(salt_b64), Some(wrapped)) = (password, kek_salt, encrypted_dek) {
            let salt = crypto::decode_salt(salt_b64)?;
            let kek = crypto::derive_kek(pw, &salt)?;
            self.dek = Some(crypto::unwrap_dek(&kek, wrapped)?);
        }
        self.sync_account_state()
    }

    /// 注册新账户时生成 E2EE 材料，返回供上传的字段。
    ///
    /// 生成随机 DEK 和 kek_salt，用密码派生 KEK 包裹 DEK，将 DEK 存入内存。
    /// 返回值 `(kek_salt_b64, encrypted_dek_b64)` 应随注册请求发送给服务端。
    pub fn generate_e2ee_material(&mut self, password: &str) -> Result<(String, String)> {
        let salt = crypto::generate_kek_salt();
        let kek = crypto::derive_kek(password, &salt)?;
        let dek = crypto::generate_dek();
        let wrapped = crypto::wrap_dek(&kek, &dek)?;
        self.dek = Some(dek);
        Ok((crypto::encode_salt(&salt), wrapped))
    }

    /// 返回当前内存中的 DEK（供 processor 使用）。
    pub fn dek(&self) -> Option<&[u8; 32]> {
        self.dek.as_ref()
    }

    /// 获取当前登录账户 ID（未登录时返回 None）。
    fn current_account_id(&self) -> Result<Option<String>> {
        Ok(self.repo.get_or_create_sync_state()?.account_id)
    }

    /// 统计匿名本地笔记数量，用于登录提示（"你有 N 条本地笔记，是否迁移？"）。
    pub fn anonymous_note_count(&self) -> Result<usize> {
        self.repo.count_anonymous_notes()
    }

    /// 将所有匿名笔记归属到当前账户，并为每条笔记入队 UpsertNote 同步变更。
    pub fn import_anonymous_notes_to_current_account(&self) -> Result<Vec<NoteSummary>> {
        let account_id = self
            .current_account_id()?
            .ok_or_else(|| anyhow::anyhow!("not logged in"))?;
        let imported_ids = self.repo.import_anonymous_notes(&account_id)?;
        for note_id in imported_ids {
            // 清除旧的匿名队列条目，再以账户身份重新入队
            self.repo.delete_changes_for_note(None, &note_id)?;
            let note = self.repo.get_note(&note_id)?;
            self.enqueue_note_change(&note, SyncOpType::UpsertNote, 0)?;
        }
        self.repo.list_recent_for_owner(50, Some(&account_id))
    }

    /// 返回当前账户的待处理同步队列（最多 100 条）。
    pub fn pending_sync_changes(&self) -> Result<Vec<snapline_storage::ChangeQueueItem>> {
        let account_id = self
            .current_account_id()?
            .ok_or_else(|| anyhow::anyhow!("not logged in"))?;
        self.repo.list_pending_changes(Some(&account_id), 100)
    }

    /// 返回应用数据根目录。
    pub fn data_dir(&self) -> &std::path::Path {
        &self.paths.data_dir
    }

    /// 返回完整的同步状态（供调试或日志使用）。
    pub fn sync_state(&self) -> Result<snapline_storage::SyncState> {
        self.repo.get_or_create_sync_state()
    }

    /// 返回同步所需的凭据三元组 `(base_url, token, device_id)`，未登录时返回 None。
    pub fn sync_credentials(&self) -> Result<Option<(String, String, String)>> {
        let state = self.repo.get_or_create_sync_state()?;
        match (state.server_base_url, state.access_token) {
            (Some(base_url), Some(token)) => Ok(Some((base_url, token, state.device_id))),
            _ => Ok(None),
        }
    }

    /// 删除单条同步队列条目（推送成功后由 processor 调用）。
    pub fn delete_sync_change(&self, queue_id: &str) -> Result<()> {
        self.repo.delete_change(queue_id)
    }

    /// 删除某笔记的所有待处理同步变更（冲突解决后调用）。
    pub fn delete_sync_changes_for_note(&self, note_id: &NoteId) -> Result<()> {
        let account_id = self.current_account_id()?;
        self.repo
            .delete_changes_for_note(account_id.as_deref(), note_id)
    }

    /// 标记同步变更失败（增加重试计数）。
    pub fn mark_sync_change_failed(&self, queue_id: &str, error: &str) -> Result<()> {
        self.repo.mark_change_failed(queue_id, error)
    }

    /// 更新笔记的服务端版本号（推送被接受后由 processor 调用）。
    pub fn update_note_server_version(&self, id: &NoteId, server_version: i64) -> Result<()> {
        self.repo.update_note_server_version(id, server_version)
    }

    /// 将远端拉取的笔记写入本地镜像（不入队同步）。
    pub fn apply_remote_note(&self, note: &Note) -> Result<()> {
        self.repo.apply_remote_note(note)
    }

    /// 检查指定笔记是否有待处理的本地变更（用于冲突预判）。
    pub fn has_pending_note_change(&self, note_id: &NoteId) -> Result<bool> {
        let account_id = self.current_account_id()?;
        self.repo
            .has_pending_note_change(account_id.as_deref(), note_id)
    }

    /// 为冲突笔记创建副本（保留本地编辑内容）。
    pub fn create_conflict_copy(&self, note: &Note) -> Result<Note> {
        self.repo.create_conflict_copy(note, Utc::now())
    }

    /// 将服务端快照批量应用到本地：若本地有未推送变更则先创建冲突副本。
    ///
    /// 若内存中持有 DEK，则在写入本地前对每条笔记解密。
    pub fn import_snapshot(&self, notes: &[Note], cursor: i64) -> Result<()> {
        let account_id = self
            .current_account_id()?
            .ok_or_else(|| anyhow::anyhow!("not logged in"))?;
        for note in notes {
            let decrypted = match self.dek.as_ref() {
                Some(key) => {
                    let title = crypto::decrypt_field(key, &note.title)?;
                    let content_md = crypto::decrypt_field(key, &note.content_md)?;
                    std::borrow::Cow::Owned(Note { title, content_md, ..note.clone() })
                }
                None => std::borrow::Cow::Borrowed(note),
            };
            if self
                .repo
                .has_pending_note_change(Some(&account_id), &decrypted.id)?
            {
                let local_note = self.repo.get_note_for_owner(&decrypted.id, Some(&account_id))?;
                self.repo.create_conflict_copy(&local_note, Utc::now())?;
                self.repo
                    .delete_changes_for_note(Some(&account_id), &decrypted.id)?;
            }
            self.repo.apply_remote_note(&decrypted)?;
        }
        self.repo.update_sync_cursor_success(cursor, Utc::now())
    }

    /// 从快照资源列表中过滤出本地缺失的资源（需要下载的部分）。
    pub fn missing_asset_metadata(&self, assets: &[AssetMetadata]) -> Vec<AssetMetadata> {
        assets
            .iter()
            .filter(|asset| {
                let path = self.paths.markdown_asset_path(
                    &asset.note_id,
                    &asset.id,
                    asset_extension(asset),
                );
                !self.paths.resolve_markdown_asset_path(&path).exists()
            })
            .cloned()
            .collect()
    }

    /// 将从服务端下载的资源文件写入本地磁盘。
    pub fn save_remote_asset(&self, asset: &AssetMetadata, bytes: &[u8]) -> Result<()> {
        let extension = asset_extension(asset);
        let path = self
            .paths
            .note_asset_path(&asset.note_id, &asset.id, extension);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, bytes)?;
        Ok(())
    }

    /// 更新服务端游标和最后同步时间（拉取成功后调用）。
    pub fn update_sync_cursor_success(
        &self,
        cursor: i64,
        now: chrono::DateTime<Utc>,
    ) -> Result<()> {
        self.repo.update_sync_cursor_success(cursor, now)
    }

    /// 向同步队列写入笔记变更（仅当笔记属于已登录账户时才入队）。
    fn enqueue_note_change(
        &self,
        note: &Note,
        op_type: SyncOpType,
        base_version: i64,
    ) -> Result<()> {
        let Some(account_id) = note.owner_account_id.as_deref() else {
            return Ok(()); // 匿名笔记不同步
        };
        let payload = SyncPayload::Note(NoteChangePayload::from_note(note));
        self.repo.enqueue_change(
            Some(account_id),
            &note.id,
            op_type,
            base_version,
            &payload,
            Utc::now(),
        )?;
        Ok(())
    }
}

/// 根据 MIME 类型返回文件扩展名（用于资源路径拼接）。
fn asset_extension(asset: &AssetMetadata) -> &str {
    match asset.content_type.as_str() {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        _ => "bin",
    }
}

#[cfg(test)]
mod tests {
    use super::AppCore;
    use snapline_platform::AppPaths;
    use snapline_storage::NoteRepository;

    #[test]
    fn bootstrap_starts_with_a_blank_draft_note() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let repo = NoteRepository::open_in_memory().unwrap();
        let core = AppCore::with_repo(paths, repo);

        let state = core.bootstrap().unwrap();

        assert!(state.notes.is_empty());
        assert_eq!(state.current.title, "Untitled");
        assert!(!state.current.pinned);
    }

    #[test]
    fn bootstrap_does_not_persist_a_blank_draft_note() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let core = AppCore::open(AppPaths::from_data_dir(dir.path())).unwrap();

        let state = core.bootstrap().unwrap();
        let repo = NoteRepository::open(&paths.db_path).unwrap();

        assert_eq!(state.current.title, "Untitled");
        assert!(repo.list_recent(10).unwrap().is_empty());
    }

    #[test]
    fn stores_and_loads_open_shortcut() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let core = AppCore::open(AppPaths::from_data_dir(dir.path())).unwrap();

        core.set_open_shortcut("Ctrl+Alt+S").unwrap();

        let reopened = AppCore::open(paths).unwrap();
        assert_eq!(reopened.get_open_shortcut().unwrap(), "Ctrl+Alt+S");
    }

    #[test]
    fn saves_png_asset_under_note_directory() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let repo = NoteRepository::open_in_memory().unwrap();
        let core = AppCore::with_repo(paths, repo);
        let note = core.create_note().unwrap();

        let asset = core.save_png_asset(&note.id, &[137, 80, 78, 71]).unwrap();

        assert!(asset
            .markdown_path
            .starts_with(&format!("assets/notes/{}/", note.id)));
        assert!(dir.path().join(&asset.markdown_path).exists());
        assert!(asset.asset_url.starts_with("asset://localhost/"));
    }

    #[test]
    fn resolves_asset_urls_without_frontend_path_api() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let repo = NoteRepository::open_in_memory().unwrap();
        let core = AppCore::with_repo(paths, repo);

        let resolved = core.resolve_asset_url("assets/notes/example/image.png");
        assert!(resolved.starts_with("asset://localhost/"));
        assert!(resolved.ends_with("image.png"));
    }

    #[test]
    fn anonymous_save_does_not_enqueue_sync_change() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let repo = NoteRepository::open_in_memory().unwrap();
        let core = AppCore::with_repo(paths, repo);
        let note = core.create_note().unwrap();

        core.save_note(&note.id, "Title", "# Title", false).unwrap();

        assert!(core.pending_sync_changes().is_err());
    }

    #[test]
    fn account_save_enqueues_upsert_change() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let repo = NoteRepository::open_in_memory().unwrap();
        let mut core = AppCore::with_repo(paths, repo);
        core.save_sync_login("http://localhost:8080", "acct_a", "token", None, None, None)
            .unwrap();
        let note = core.create_note().unwrap();

        core.save_note(&note.id, "Title", "# Title", false).unwrap();

        let changes = core.pending_sync_changes().unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].op_type, snapline_domain::SyncOpType::UpsertNote);
        assert_eq!(changes[0].account_id.as_deref(), Some("acct_a"));
    }

    #[test]
    fn set_note_title_enqueues_upsert_change() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let repo = NoteRepository::open_in_memory().unwrap();
        let mut core = AppCore::with_repo(paths, repo);
        core.save_sync_login("http://localhost:8080", "acct_a", "token", None, None, None)
            .unwrap();
        let note = core.create_note().unwrap();
        core.save_note(&note.id, "Title", "# Title", false).unwrap();
        for change in core.pending_sync_changes().unwrap() {
            core.delete_sync_change(&change.id).unwrap();
        }

        core.set_note_title(&note.id, "Renamed").unwrap();

        let changes = core.pending_sync_changes().unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].op_type, snapline_domain::SyncOpType::UpsertNote);
        assert_eq!(changes[0].base_version, 0);
    }

    #[test]
    fn save_png_asset_enqueues_asset_upload() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let repo = NoteRepository::open_in_memory().unwrap();
        let mut core = AppCore::with_repo(paths, repo);
        core.save_sync_login("http://localhost:8080", "acct_a", "token", None, None, None)
            .unwrap();
        let note = core.create_note().unwrap();

        core.save_png_asset(&note.id, &[137, 80, 78, 71]).unwrap();

        let changes = core.pending_sync_changes().unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].op_type, snapline_domain::SyncOpType::AssetUpload);
    }

    #[test]
    fn import_snapshot_applies_notes_and_updates_cursor() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let repo = NoteRepository::open_in_memory().unwrap();
        let mut core = AppCore::with_repo(paths, repo);
        core.save_sync_login("http://localhost:8080", "acct_a", "token", None, None, None)
            .unwrap();
        let mut note = snapline_domain::Note::draft(chrono::Utc::now());
        note.owner_account_id = Some("acct_a".to_string());
        note.title = "Remote".to_string();
        note.server_version = 4;

        core.import_snapshot(&[note.clone()], 9).unwrap();

        assert_eq!(core.get_note(&note.id).unwrap().title, "Remote");
        assert_eq!(core.sync_state().unwrap().server_cursor, 9);
    }

    #[test]
    fn import_snapshot_creates_conflict_copy_for_pending_local_changes() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let repo = NoteRepository::open_in_memory().unwrap();
        let mut core = AppCore::with_repo(paths, repo);
        core.save_sync_login("http://localhost:8080", "acct_a", "token", None, None, None)
            .unwrap();
        let note = core.create_note().unwrap();
        core.save_note(&note.id, "Local", "# Local", false).unwrap();
        let mut remote_note = note.clone();
        remote_note.title = "Remote".to_string();
        remote_note.content_md = "# Remote".to_string();
        remote_note.server_version = 3;

        core.import_snapshot(&[remote_note], 9).unwrap();

        assert_eq!(core.get_note(&note.id).unwrap().title, "Remote");
        assert!(core
            .bootstrap()
            .unwrap()
            .notes
            .iter()
            .any(|note| note.is_conflict_copy));
        assert!(core.pending_sync_changes().unwrap().is_empty());
        assert_eq!(core.sync_state().unwrap().server_cursor, 9);
    }

    #[test]
    fn save_remote_asset_uses_metadata_location() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let repo = NoteRepository::open_in_memory().unwrap();
        let core = AppCore::with_repo(paths, repo);
        let note_id = snapline_domain::NoteId::new();
        let asset = snapline_domain::AssetMetadata {
            id: snapline_domain::AssetId::new(),
            note_id: note_id.clone(),
            content_type: "image/png".to_string(),
            byte_size: 4,
            sha256: "sha".to_string(),
            storage_key: "server/key".to_string(),
            created_at: chrono::Utc::now(),
            deleted_at: None,
        };

        core.save_remote_asset(&asset, &[1, 2, 3, 4]).unwrap();

        assert_eq!(
            std::fs::read(
                dir.path()
                    .join(core.paths.markdown_asset_path(&note_id, &asset.id, "png"))
            )
            .unwrap(),
            vec![1, 2, 3, 4]
        );
    }

    #[test]
    fn bootstrap_shows_local_notes_when_logged_out_and_account_notes_when_logged_in() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let repo = NoteRepository::open_in_memory().unwrap();
        let mut core = AppCore::with_repo(paths, repo);

        let local = core.create_note().unwrap();
        core.save_note(&local.id, "Local", "Local", false).unwrap();
        assert_eq!(core.bootstrap().unwrap().notes.len(), 1);

        core.save_sync_login("http://localhost:8080", "acct_a", "token", None, None, None)
            .unwrap();
        assert!(core.bootstrap().unwrap().notes.is_empty());

        core.import_anonymous_notes_to_current_account().unwrap();
        assert_eq!(core.bootstrap().unwrap().notes.len(), 1);
    }

    #[test]
    fn importing_anonymous_notes_enqueues_upserts_for_current_account() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let repo = NoteRepository::open_in_memory().unwrap();
        let mut core = AppCore::with_repo(paths, repo);

        let local = core.create_note().unwrap();
        core.save_note(&local.id, "Local", "Local", false).unwrap();
        core.save_sync_login("http://localhost:8080", "acct_a", "token", None, None, None)
            .unwrap();
        core.import_anonymous_notes_to_current_account().unwrap();

        let changes = core.pending_sync_changes().unwrap();
        assert!(changes
            .iter()
            .all(|item| item.account_id.as_deref() == Some("acct_a")));
        assert!(changes.iter().any(|item| item.note_id == local.id));
    }

    #[test]
    fn importing_anonymous_notes_removes_old_anonymous_queue_rows() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let repo = NoteRepository::open_in_memory().unwrap();
        let mut core = AppCore::with_repo(paths, repo);

        let local = core.create_note().unwrap();
        core.save_note(&local.id, "Local", "Local", false).unwrap();
        let payload = snapline_domain::SyncPayload::Note(
            snapline_domain::NoteChangePayload::from_note(&local),
        );
        core.repo
            .enqueue_change(
                None,
                &local.id,
                snapline_domain::SyncOpType::UpsertNote,
                0,
                &payload,
                chrono::Utc::now(),
            )
            .unwrap();

        core.save_sync_login("http://localhost:8080", "acct_a", "token", None, None, None)
            .unwrap();
        core.import_anonymous_notes_to_current_account().unwrap();

        assert!(core.repo.list_pending_changes(None, 10).unwrap().is_empty());
        let changes = core.pending_sync_changes().unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].account_id.as_deref(), Some("acct_a"));
    }

    #[test]
    fn logged_in_account_cannot_modify_anonymous_note_by_id() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let repo = NoteRepository::open_in_memory().unwrap();
        let mut core = AppCore::with_repo(paths, repo);

        let local = core.create_note().unwrap();
        core.save_note(&local.id, "Local", "Local", false).unwrap();
        core.save_sync_login("http://localhost:8080", "acct_a", "token", None, None, None)
            .unwrap();

        assert!(core
            .save_note(&local.id, "Account edit", "Account edit", false)
            .is_err());
        assert!(core.set_note_pinned(&local.id, true).is_err());
        assert!(core.delete_note(&local.id).is_err());
        assert!(core.save_png_asset(&local.id, &[137, 80, 78, 71]).is_err());
    }
}
