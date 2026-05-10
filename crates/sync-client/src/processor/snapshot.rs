use crate::SyncApi;
use anyhow::Result;
use chrono::Utc;
use snapline_domain::{AssetMetadata, Note};
use snapline_platform::AppPaths;
use snapline_storage::NoteRepository;
use std::{fs, path::Path};

pub(super) async fn import_snapshot_and_assets_from_path<A: SyncApi + Sync>(
    db_path: &Path,
    api: &A,
    token: &str,
    data_dir: &Path,
) -> Result<()> {
    let snapshot = api.snapshot(token).await?;
    let missing_assets = {
        let repo = NoteRepository::open(db_path)?;
        import_snapshot(&repo, &snapshot.notes, snapshot.cursor)?;
        missing_asset_metadata(data_dir, &snapshot.assets)
    };
    for asset in missing_assets {
        let downloaded = api.download_asset(token, &asset.id.to_string()).await?;
        save_remote_asset(data_dir, &asset, &downloaded.bytes)?;
    }
    Ok(())
}

pub async fn import_snapshot_and_assets<A: SyncApi + Sync>(
    repo: &NoteRepository,
    api: &A,
    token: &str,
    data_dir: &Path,
) -> Result<()> {
    let snapshot = api.snapshot(token).await?;
    import_snapshot(repo, &snapshot.notes, snapshot.cursor)?;
    let missing_assets = missing_asset_metadata(data_dir, &snapshot.assets);
    for asset in missing_assets {
        let downloaded = api.download_asset(token, &asset.id.to_string()).await?;
        save_remote_asset(data_dir, &asset, &downloaded.bytes)?;
    }
    Ok(())
}

fn import_snapshot(repo: &NoteRepository, notes: &[Note], cursor: i64) -> Result<()> {
    let account_id = repo
        .get_or_create_sync_state()?
        .account_id
        .ok_or_else(|| anyhow::anyhow!("not logged in"))?;
    for note in notes {
        if repo.has_pending_note_change(Some(&account_id), &note.id)? {
            let local_note = repo.get_note_for_owner(&note.id, Some(&account_id))?;
            repo.create_conflict_copy(&local_note, Utc::now())?;
            repo.delete_changes_for_note(Some(&account_id), &note.id)?;
        }
        repo.apply_remote_note(note)?;
    }
    repo.update_sync_cursor_success(cursor, Utc::now())
}

fn missing_asset_metadata(data_dir: &Path, assets: &[AssetMetadata]) -> Vec<AssetMetadata> {
    assets
        .iter()
        .filter(|asset| {
            let paths = AppPaths::from_data_dir(data_dir);
            let path = paths.markdown_asset_path(&asset.note_id, &asset.id, asset_extension(asset));
            !paths.resolve_markdown_asset_path(&path).exists()
        })
        .cloned()
        .collect()
}

fn save_remote_asset(data_dir: &Path, asset: &AssetMetadata, bytes: &[u8]) -> Result<()> {
    let paths = AppPaths::from_data_dir(data_dir);
    let path = paths.note_asset_path(&asset.note_id, &asset.id, asset_extension(asset));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes)?;
    Ok(())
}

fn asset_extension(asset: &AssetMetadata) -> &str {
    match asset.content_type.as_str() {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        _ => "bin",
    }
}
