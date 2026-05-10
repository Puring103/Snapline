/// 平台路径解析：将应用数据目录和资源路径统一封装，屏蔽 Windows/macOS/Linux 差异。
use anyhow::{anyhow, Result};
use directories::ProjectDirs;
use snapline_domain::{AssetId, NoteId};
use std::path::{Path, PathBuf};

pub fn is_allowed_markdown_asset_path(markdown_path: &str) -> bool {
    markdown_path.starts_with("assets/")
        && !markdown_path.contains('\\')
        && !markdown_path
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
}

pub fn is_allowed_external_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    !url.chars()
        .any(|character| character == '\r' || character == '\n')
        && (lower.starts_with("http://")
            || lower.starts_with("https://")
            || lower.starts_with("mailto:"))
}

pub fn markdown_export_filename(title: &str) -> String {
    if title.trim().is_empty() {
        return "Untitled.md".to_string();
    }

    let safe: String = title
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{}.md", safe.trim())
}

/// 应用数据目录及数据库路径的集合。
///
/// 通过 `AppPaths::resolve()` 获取系统标准位置（如 Windows 的 `%APPDATA%\Snapline`），
/// 测试中使用 `AppPaths::from_data_dir(dir)` 注入临时目录。
#[derive(Debug, Clone)]
pub struct AppPaths {
    /// 应用数据根目录（资源文件、数据库都存放于此）。
    pub data_dir: PathBuf,
    /// SQLite 数据库文件路径（`<data_dir>/snapline.db`）。
    pub db_path: PathBuf,
}

impl AppPaths {
    /// 使用 `directories` crate 解析当前操作系统的标准应用数据目录。
    pub fn resolve() -> Result<Self> {
        let dirs = ProjectDirs::from("", "", "Snapline")
            .ok_or_else(|| anyhow!("could not resolve Snapline data directory"))?;
        Ok(Self::from_data_dir(dirs.data_dir()))
    }

    /// 从指定目录构造路径集合（测试友好）。
    pub fn from_data_dir(data_dir: impl AsRef<Path>) -> Self {
        let data_dir = data_dir.as_ref().to_path_buf();
        Self {
            db_path: data_dir.join("snapline.db"),
            data_dir,
        }
    }

    /// 返回指定笔记的资源文件存放目录（`<data_dir>/assets/notes/<note_id>/`）。
    pub fn note_asset_dir(&self, note_id: &NoteId) -> PathBuf {
        self.data_dir
            .join("assets")
            .join("notes")
            .join(note_id.to_string())
    }

    /// 返回某个资源文件在磁盘上的完整路径（`<data_dir>/assets/notes/<note_id>/<asset_id>.<ext>`）。
    pub fn note_asset_path(&self, note_id: &NoteId, asset_id: &AssetId, ext: &str) -> PathBuf {
        self.note_asset_dir(note_id)
            .join(format!("{}.{}", asset_id, ext))
    }

    /// 返回写入 Markdown 正文的资源引用路径（相对路径，如 `assets/notes/<note_id>/<asset_id>.<ext>`）。
    ///
    /// 此路径同时作为 `AssetUploadPayload.markdown_path` 发送给服务端。
    pub fn markdown_asset_path(&self, note_id: &NoteId, asset_id: &AssetId, ext: &str) -> String {
        format!("assets/notes/{}/{}.{}", note_id, asset_id, ext)
    }

    /// 将 Markdown 中的相对资源路径还原为磁盘绝对路径。
    pub fn resolve_markdown_asset_path(&self, markdown_path: &str) -> PathBuf {
        self.data_dir.join(markdown_path)
    }

    /// 将 Markdown 中的相对资源路径转换为 `asset://localhost/...` URL。
    ///
    /// Tauri 的 `asset` 协议允许 WebView 访问沙箱外的本地文件，
    /// 前端 `<img src="asset://localhost/...">` 即可渲染本地图片。
    pub fn markdown_asset_url(&self, markdown_path: &str) -> String {
        format!(
            "asset://localhost/{}",
            markdown_path.trim_start_matches('/')
        )
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
        assert_eq!(
            paths.note_asset_dir(&note_id),
            std::path::PathBuf::from(expected_dir)
        );
        assert_eq!(
            paths.markdown_asset_path(&note_id, &asset_id, "png"),
            format!("assets/notes/{}/{}.png", note_id, asset_id)
        );
    }

    #[test]
    fn validates_internal_markdown_asset_paths() {
        assert!(super::is_allowed_markdown_asset_path(
            "assets/notes/note-id/image-id.png"
        ));
        assert!(!super::is_allowed_markdown_asset_path("../snapline.db"));
        assert!(!super::is_allowed_markdown_asset_path(
            "assets/../snapline.db"
        ));
        assert!(!super::is_allowed_markdown_asset_path(
            "C:/Users/wtl/image.png"
        ));
        assert!(!super::is_allowed_markdown_asset_path(
            "assets\\notes\\note-id\\image-id.png"
        ));
    }

    #[test]
    fn validates_external_urls() {
        assert!(super::is_allowed_external_url("https://example.com"));
        assert!(super::is_allowed_external_url("mailto:a@example.com"));
        assert!(!super::is_allowed_external_url("file:///etc/passwd"));
        assert!(!super::is_allowed_external_url("https://example.com\nbad"));
    }

    #[test]
    fn derives_markdown_export_filename() {
        assert_eq!(super::markdown_export_filename(""), "Untitled.md");
        assert_eq!(
            super::markdown_export_filename("Daily: note/one"),
            "Daily_ note_one.md"
        );
    }
}
