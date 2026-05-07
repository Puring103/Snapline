use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 笔记的唯一标识符，基于 UUID v4。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NoteId(pub Uuid);

impl NoteId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for NoteId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for NoteId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// 笔记的完整数据模型。
///
/// `server_version` 为乐观锁版本号，初始为 0，每次服务端接受推送后递增。
/// `owner_account_id` 为 None 表示匿名（本地）笔记，登录后迁移为账户笔记。
/// `is_conflict_copy` 标记此笔记是同步冲突时自动保存的副本。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub id: NoteId,
    pub title: String,
    pub content_md: String,
    pub pinned: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// 软删除时间戳；不为 None 表示已删除。
    pub deleted_at: Option<DateTime<Utc>>,
    /// 服务端版本号（乐观锁），本地草稿为 0。
    pub server_version: i64,
    /// 最后修改此笔记的设备 ID（用于拉取时过滤自己的推送）。
    pub last_modified_by_device: Option<String>,
    /// 是否为冲突副本。
    pub is_conflict_copy: bool,
    /// 冲突副本的来源笔记 ID。
    pub source_note_id: Option<NoteId>,
    /// 所属账户 ID；None 表示匿名本地笔记。
    pub owner_account_id: Option<String>,
}

/// 笔记列表中的摘要视图，仅包含展示所需的字段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteSummary {
    pub id: NoteId,
    pub title: String,
    /// 纯文本预览（已剥离 Markdown 标记，截断到 500 字符）。
    pub preview: String,
    /// 保留 Markdown 标记的预览（不截断，用于富文本渲染）。
    pub preview_md: String,
    pub pinned: bool,
    pub updated_at: DateTime<Utc>,
    pub is_conflict_copy: bool,
    pub source_note_id: Option<NoteId>,
    pub owner_account_id: Option<String>,
}

impl Note {
    /// 创建一个未持久化的空白草稿笔记。
    pub fn draft(now: DateTime<Utc>) -> Self {
        Self {
            id: NoteId::new(),
            title: "Untitled".to_string(),
            content_md: String::new(),
            pinned: false,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            server_version: 0,
            last_modified_by_device: None,
            is_conflict_copy: false,
            source_note_id: None,
            owner_account_id: None,
        }
    }
}

/// 从 Markdown 内容中提取标题。
///
/// 取第一个 `# ` 开头的非空行作为标题；找不到则返回 `"Untitled"`。
pub fn derive_title(content_md: &str) -> String {
    content_md
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("# ").map(str::trim))
        .filter(|title| !title.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| "Untitled".to_string())
}

/// 从 Markdown 内容中生成纯文本预览。
///
/// 跳过标题行和空行，剥离列表标记，最多取前 500 个字符。
pub fn derive_preview(content_md: &str) -> String {
    let preview = content_md
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("# "))
        .map(strip_markdown_markup)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    preview
        .chars()
        .take(500)
        .collect::<String>()
        .trim()
        .to_string()
}

/// 从 Markdown 内容中生成保留标记的预览。
///
/// 只去掉标题行，保留其余所有 Markdown 标记，不截断——供前端渲染器使用。
pub fn derive_preview_markdown(content_md: &str) -> String {
    let title_line_index = content_md
        .lines()
        .position(|line| line.trim().starts_with("# "));

    content_md
        .lines()
        .enumerate()
        .filter(|(index, _line)| Some(*index) != title_line_index)
        .map(|(_index, line)| line)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// 剥离行首的常见 Markdown 列表标记（`- `、`* `、`1. ` 等）。
fn strip_markdown_markup(line: &str) -> String {
    line.trim_start_matches("- ")
        .trim_start_matches("* ")
        .trim_start_matches("1. ")
        .trim_start_matches("2. ")
        .trim_start_matches("3. ")
        .trim_start_matches("4. ")
        .trim_start_matches("5. ")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{derive_preview, derive_preview_markdown, derive_title, NoteId};

    #[test]
    fn default_note_id_generates_uuid() {
        let id = NoteId::default();

        assert_eq!(id.to_string().len(), 36);
    }

    #[test]
    fn derives_title_from_first_non_empty_heading() {
        assert_eq!(derive_title("\n# Daily note\nbody"), "Daily note");
        assert_eq!(derive_title("## Secondary\n# Primary"), "Primary");
    }

    #[test]
    fn falls_back_for_empty_content() {
        assert_eq!(derive_title(" \n\t"), "Untitled");
    }

    #[test]
    fn derives_preview_from_first_body_line() {
        assert_eq!(
            derive_preview("# Daily note\n\n- First item\nSecond"),
            "First item\nSecond"
        );
    }

    #[test]
    fn derives_markdown_preview_without_stripping_markup() {
        assert_eq!(
            derive_preview_markdown("# Daily note\n\n- **First** item\nSecond"),
            "- **First** item\nSecond"
        );
    }

    #[test]
    fn derives_full_markdown_preview_without_truncating() {
        let long_body = format!(
            "# Daily note\n\n{}\n\n{}\n\n{}",
            "- Parent\n  - Child",
            "```ts\n  const value = 1;\n```",
            "x".repeat(620)
        );

        let preview = derive_preview_markdown(&long_body);
        assert!(!preview.contains("# Daily note"));
        assert!(preview.contains("  - Child"));
        assert!(preview.contains("  const value = 1;"));
        assert!(preview.len() > 500);
    }
}
