use crate::processor::ProcessReport;
use crate::protocol::AssetUploadRequest;
use crate::SyncApi;
use anyhow::{Context, Result};
use snapline_domain::SyncPayload;
use snapline_storage::NoteRepository;
use std::{fs, path::Path};

pub(super) async fn upload_pending_assets_from_path<A: SyncApi + Sync>(
    db_path: &Path,
    api: &A,
    token: &str,
    data_dir: &Path,
) -> Result<ProcessReport> {
    let pending = {
        let repo = NoteRepository::open(db_path)?;
        let account_id = repo
            .get_or_create_sync_state()?
            .account_id
            .ok_or_else(|| anyhow::anyhow!("not logged in"))?;
        repo.list_pending_changes(Some(&account_id), 100)?
            .into_iter()
            .filter(|item| matches!(item.payload, SyncPayload::Asset(_)))
            .collect::<Vec<_>>()
    };

    let mut report = ProcessReport::default();
    for item in pending {
        let SyncPayload::Asset(metadata) = item.payload else {
            continue;
        };
        let asset_path = data_dir.join(&metadata.markdown_path);
        let bytes = fs::read(&asset_path)
            .with_context(|| format!("failed to read asset {}", asset_path.display()))?;
        api.upload_asset(token, AssetUploadRequest { metadata, bytes })
            .await?;
        let repo = NoteRepository::open(db_path)?;
        repo.delete_change(&item.id)?;
        report.accepted += 1;
    }
    Ok(report)
}

/// 将当前账户的待上传资源文件逐一读取并上传到服务端。
///
/// 文件路径从 `AssetUploadPayload.markdown_path` 中解析，相对于 `data_dir`。
pub async fn upload_pending_assets<A: SyncApi + Sync>(
    repo: &NoteRepository,
    api: &A,
    token: &str,
    data_dir: &Path,
) -> Result<ProcessReport> {
    let account_id = repo
        .get_or_create_sync_state()?
        .account_id
        .ok_or_else(|| anyhow::anyhow!("not logged in"))?;
    let pending = repo.list_pending_changes(Some(&account_id), 100)?;
    let mut report = ProcessReport::default();
    for item in pending {
        let SyncPayload::Asset(metadata) = item.payload else {
            continue;
        };
        let asset_path = data_dir.join(&metadata.markdown_path);
        let bytes = fs::read(&asset_path)
            .with_context(|| format!("failed to read asset {}", asset_path.display()))?;
        api.upload_asset(token, AssetUploadRequest { metadata, bytes })
            .await?;
        repo.delete_change(&item.id)?;
        report.accepted += 1;
    }
    Ok(report)
}
