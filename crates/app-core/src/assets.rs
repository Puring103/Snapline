use anyhow::{bail, Result};
use chrono::Utc;
use sha2::{Digest, Sha256};
use snapline_domain::{
    AssetId, AssetMetadata, AssetRef, AssetUploadPayload, NoteId, SyncOpType, SyncPayload,
};
use std::fs;

use crate::AppCore;

impl AppCore {
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

    pub fn resolve_asset_url(&self, markdown_path: &str) -> String {
        self.paths.markdown_asset_url(markdown_path)
    }

    pub fn resolve_asset_path(&self, markdown_path: &str) -> std::path::PathBuf {
        self.paths.resolve_markdown_asset_path(markdown_path)
    }

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
}

fn asset_extension(asset: &AssetMetadata) -> &str {
    match asset.content_type.as_str() {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        _ => "bin",
    }
}
