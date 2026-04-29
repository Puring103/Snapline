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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteSummary {
    pub id: NoteId,
    pub title: String,
    pub preview: String,
    pub pinned: bool,
    pub updated_at: DateTime<Utc>,
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
    let body_line = content_md
        .lines()
        .map(str::trim)
        .skip_while(|line| line.is_empty() || line.starts_with("# "))
        .find(|line| !line.is_empty())
        .unwrap_or("");

    strip_markdown_markup(body_line)
        .chars()
        .take(80)
        .collect::<String>()
}

fn strip_markdown_markup(line: &str) -> String {
    line
        .trim_start_matches("- ")
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
    use super::{derive_preview, derive_title};

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
        assert_eq!(derive_preview("# Daily note\n\n- First item\nSecond"), "First item");
    }
}
