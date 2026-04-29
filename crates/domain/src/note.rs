use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NoteId(pub Uuid);

impl NoteId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl std::fmt::Display for NoteId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub id: NoteId,
    pub title: String,
    pub content_md: String,
    pub pinned: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub server_version: i64,
    pub last_modified_by_device: Option<String>,
    pub is_conflict_copy: bool,
    pub source_note_id: Option<NoteId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteSummary {
    pub id: NoteId,
    pub title: String,
    pub preview: String,
    pub preview_md: String,
    pub pinned: bool,
    pub updated_at: DateTime<Utc>,
    pub is_conflict_copy: bool,
    pub source_note_id: Option<NoteId>,
}

impl Note {
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
        }
    }
}

pub fn derive_title(content_md: &str) -> String {
    content_md
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("# ").map(str::trim))
        .filter(|title| !title.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| "Untitled".to_string())
}

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
    use super::{derive_preview, derive_preview_markdown, derive_title};

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
