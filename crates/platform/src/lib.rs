use anyhow::{anyhow, Result};
use directories::ProjectDirs;
use snapline_domain::{AssetId, NoteId};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub db_path: PathBuf,
}

impl AppPaths {
    pub fn resolve() -> Result<Self> {
        let dirs = ProjectDirs::from("", "", "Snapline")
            .ok_or_else(|| anyhow!("could not resolve Snapline data directory"))?;
        Ok(Self::from_data_dir(dirs.data_dir()))
    }

    pub fn from_data_dir(data_dir: impl AsRef<Path>) -> Self {
        let data_dir = data_dir.as_ref().to_path_buf();
        Self {
            db_path: data_dir.join("snapline.db"),
            data_dir,
        }
    }

    pub fn note_asset_dir(&self, note_id: &NoteId) -> PathBuf {
        self.data_dir.join("assets").join("notes").join(note_id.to_string())
    }

    pub fn note_asset_path(&self, note_id: &NoteId, asset_id: &AssetId, ext: &str) -> PathBuf {
        self.note_asset_dir(note_id).join(format!("{}.{}", asset_id, ext))
    }

    pub fn markdown_asset_path(&self, note_id: &NoteId, asset_id: &AssetId, ext: &str) -> String {
        format!("assets/notes/{}/{}.{}", note_id, asset_id, ext)
    }
}

#[cfg(test)]
mod tests {
    use super::AppPaths;
    use snapline_domain::{AssetId, NoteId};

    #[test]
    fn resolves_asset_paths() {
        let paths = AppPaths::from_data_dir("C:/snapline-data");
        let note_id = NoteId::new();
        let asset_id = AssetId::new();

        let expected_dir = format!("C:/snapline-data/assets/notes/{}", note_id);
        assert_eq!(paths.note_asset_dir(&note_id), std::path::PathBuf::from(expected_dir));
        assert_eq!(
            paths.markdown_asset_path(&note_id, &asset_id, "png"),
            format!("assets/notes/{}/{}.png", note_id, asset_id)
        );
    }
}
