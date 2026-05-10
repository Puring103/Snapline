use crate::processor::assets::upload_pending_assets_from_path;
use crate::processor::pull::pull_remote_changes_from_path;
use crate::processor::push::push_pending_changes_from_path;
use crate::processor::snapshot::import_snapshot_and_assets_from_path;
use crate::processor::{FullSyncContext, FullSyncReport};
use crate::SyncApi;
use anyhow::Result;
use std::path::Path;

pub async fn run_full_sync_from_path<A: SyncApi + Sync>(
    db_path: &Path,
    api: &A,
    context: FullSyncContext<'_>,
) -> Result<FullSyncReport> {
    let asset_report =
        upload_pending_assets_from_path(db_path, api, context.token, context.data_dir).await?;
    let push_report =
        push_pending_changes_from_path(db_path, api, context.token, context.device_id, context.dek)
            .await?;
    let pull_report =
        pull_remote_changes_from_path(db_path, api, context.token, context.device_id, context.dek)
            .await?;
    import_snapshot_and_assets_from_path(db_path, api, context.token, context.data_dir).await?;

    Ok(FullSyncReport {
        uploaded_assets: asset_report.accepted,
        pushed: push_report.accepted,
        pulled: pull_report.accepted,
        conflicts: push_report.conflicts + pull_report.conflicts,
        failed: asset_report.failed + push_report.failed + pull_report.failed,
    })
}
