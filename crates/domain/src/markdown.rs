use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftParts {
    pub title: String,
    pub body_md: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownImageMapping {
    pub display_source: String,
    pub markdown_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HydratedMarkdown {
    pub markdown: String,
    pub mappings: Vec<MarkdownImageMapping>,
}

pub fn normalize_markdown(markdown: &str) -> String {
    markdown.replace("\r\n", "\n").trim_end().to_string()
}

pub fn asset_url_from_markdown_path(markdown_path: &str) -> String {
    if !markdown_path.starts_with("assets/") {
        return markdown_path.to_string();
    }

    format!("asset://localhost/{markdown_path}")
}

pub fn markdown_path_from_asset_url(asset_url: &str) -> String {
    asset_url
        .strip_prefix("asset://localhost/")
        .unwrap_or(asset_url)
        .to_string()
}

pub fn compose_draft_markdown(title: &str, body_md: &str) -> String {
    let safe_title = normalize_title(title);
    let normalized_body = normalize_markdown(body_md);

    if normalized_body.is_empty() {
        format!("# {safe_title}")
    } else {
        format!("# {safe_title}\n\n{normalized_body}")
    }
}

pub fn has_meaningful_draft_content(title: &str, body_md: &str) -> bool {
    let normalized_title = normalize_markdown(title).trim().to_string();
    let normalized_body = normalize_markdown(body_md);

    (!normalized_title.is_empty() && normalized_title != "Untitled")
        || !normalized_body.trim().is_empty()
}

pub fn split_draft_markdown(markdown: &str) -> DraftParts {
    let normalized = normalize_markdown(markdown);
    let lines = normalized.lines().collect::<Vec<_>>();
    let first_visible_line_index = lines.iter().position(|line| !line.trim().is_empty());

    let Some(first_visible_line_index) = first_visible_line_index else {
        return DraftParts {
            title: "Untitled".to_string(),
            body_md: String::new(),
        };
    };

    let title = normalize_title(lines[first_visible_line_index]);
    let mut body_lines = lines[..first_visible_line_index]
        .iter()
        .chain(lines[first_visible_line_index + 1..].iter())
        .copied()
        .collect::<Vec<_>>();

    if body_lines
        .first()
        .is_some_and(|line| line.trim().is_empty())
    {
        body_lines.remove(0);
    }

    DraftParts {
        title,
        body_md: normalize_markdown(&body_lines.join("\n")),
    }
}

pub fn split_stored_note_markdown(stored_title: &str, markdown: &str) -> DraftParts {
    let normalized_title = normalize_title(stored_title);
    let normalized_markdown = normalize_markdown(markdown);

    if normalized_markdown.is_empty() {
        return DraftParts {
            title: normalized_title,
            body_md: String::new(),
        };
    }

    let lines = normalized_markdown.lines().collect::<Vec<_>>();
    let first_visible_line_index = lines.iter().position(|line| !line.trim().is_empty());

    let Some(first_visible_line_index) = first_visible_line_index else {
        return DraftParts {
            title: normalized_title,
            body_md: String::new(),
        };
    };

    let first_visible_title = normalize_title(lines[first_visible_line_index]);
    if first_visible_title != normalized_title {
        return DraftParts {
            title: normalized_title,
            body_md: normalized_markdown,
        };
    }

    let mut body_lines = lines[..first_visible_line_index]
        .iter()
        .chain(lines[first_visible_line_index + 1..].iter())
        .copied()
        .collect::<Vec<_>>();

    if body_lines
        .first()
        .is_some_and(|line| line.trim().is_empty())
    {
        body_lines.remove(0);
    }

    DraftParts {
        title: normalized_title,
        body_md: normalize_markdown(&body_lines.join("\n")),
    }
}

pub fn rewrite_markdown_image_sources<F>(markdown: &str, mut transform: F) -> String
where
    F: FnMut(&str) -> String,
{
    let mut output = String::with_capacity(markdown.len());
    let mut cursor = 0usize;

    while let Some(start) = markdown[cursor..].find("![") {
        let absolute_start = cursor + start;
        output.push_str(&markdown[cursor..absolute_start]);
        let Some(alt_end_offset) = markdown[absolute_start + 2..].find("](") else {
            output.push_str(&markdown[absolute_start..]);
            return output;
        };
        let source_start = absolute_start + 2 + alt_end_offset + 2;
        let Some(end_offset) = markdown[source_start..].find(')') else {
            output.push_str(&markdown[absolute_start..]);
            return output;
        };
        let source_end = source_start + end_offset;
        output.push_str(&markdown[absolute_start..source_start]);
        output.push_str(&transform(&markdown[source_start..source_end]));
        output.push(')');
        cursor = source_end + 1;
    }

    output.push_str(&markdown[cursor..]);
    output
}

pub fn hydrate_markdown_assets(markdown: &str) -> HydratedMarkdown {
    let mut mappings = Vec::new();
    let hydrated = rewrite_markdown_image_sources(markdown, |source| {
        if source.starts_with("assets/") {
            let display_source = asset_url_from_markdown_path(source);
            mappings.push(MarkdownImageMapping {
                display_source: display_source.clone(),
                markdown_path: source.to_string(),
            });
            display_source
        } else {
            source.to_string()
        }
    });

    HydratedMarkdown {
        markdown: hydrated,
        mappings,
    }
}

pub fn restore_markdown_asset_sources(markdown: &str, mappings: &[MarkdownImageMapping]) -> String {
    rewrite_markdown_image_sources(markdown, |source| {
        mappings
            .iter()
            .find(|mapping| mapping.display_source == source)
            .map(|mapping| mapping.markdown_path.clone())
            .unwrap_or_else(|| source.to_string())
    })
}

fn normalize_title(title: &str) -> String {
    let trimmed = normalize_markdown(title).trim().to_string();
    if trimmed.is_empty() {
        return "Untitled".to_string();
    }

    let stripped = trimmed.trim_start_matches('#').trim().to_string();
    if stripped.is_empty() {
        "Untitled".to_string()
    } else {
        stripped
    }
}

#[cfg(test)]
mod tests {
    use super::{
        asset_url_from_markdown_path, compose_draft_markdown, has_meaningful_draft_content,
        hydrate_markdown_assets, markdown_path_from_asset_url, normalize_markdown,
        restore_markdown_asset_sources, rewrite_markdown_image_sources, split_draft_markdown,
        split_stored_note_markdown, DraftParts, MarkdownImageMapping,
    };

    #[test]
    fn normalizes_markdown_line_endings() {
        assert_eq!(normalize_markdown("# A\r\nBody\n\n"), "# A\nBody");
    }

    #[test]
    fn maps_asset_paths_to_asset_urls() {
        assert_eq!(
            asset_url_from_markdown_path("assets/notes/note/image.png"),
            "asset://localhost/assets/notes/note/image.png"
        );
        assert_eq!(
            asset_url_from_markdown_path("https://example.com/image.png"),
            "https://example.com/image.png"
        );
    }

    #[test]
    fn maps_asset_urls_back_to_markdown_paths() {
        assert_eq!(
            markdown_path_from_asset_url("asset://localhost/assets/notes/note/image.png"),
            "assets/notes/note/image.png"
        );
        assert_eq!(
            markdown_path_from_asset_url("blob:temporary-image"),
            "blob:temporary-image"
        );
    }

    #[test]
    fn composes_and_splits_draft_markdown() {
        let markdown = compose_draft_markdown("Title", "Body line");
        assert_eq!(markdown, "# Title\n\nBody line");
        assert_eq!(
            split_draft_markdown(&markdown),
            DraftParts {
                title: "Title".to_string(),
                body_md: "Body line".to_string(),
            }
        );
    }

    #[test]
    fn split_draft_markdown_defaults_empty_title() {
        assert_eq!(
            split_draft_markdown("   \n"),
            DraftParts {
                title: "Untitled".to_string(),
                body_md: String::new(),
            }
        );
    }

    #[test]
    fn detects_meaningful_draft_content() {
        assert!(!has_meaningful_draft_content("Untitled", ""));
        assert!(!has_meaningful_draft_content("   ", "   "));
        assert!(has_meaningful_draft_content("Idea", ""));
        assert!(has_meaningful_draft_content("Untitled", "Body"));
    }

    #[test]
    fn splits_stored_note_markdown_when_body_repeats_title() {
        assert_eq!(
            split_stored_note_markdown("Stored title", "# Stored title\n\nFirst line\nSecond line"),
            DraftParts {
                title: "Stored title".to_string(),
                body_md: "First line\nSecond line".to_string(),
            }
        );
    }

    #[test]
    fn keeps_stored_markdown_intact_when_body_does_not_repeat_title() {
        assert_eq!(
            split_stored_note_markdown("第一步", "1. 第一步\n2. 第二步\n3. 第三步"),
            DraftParts {
                title: "第一步".to_string(),
                body_md: "1. 第一步\n2. 第二步\n3. 第三步".to_string(),
            }
        );
    }

    #[test]
    fn rewrites_markdown_image_sources() {
        assert_eq!(
            rewrite_markdown_image_sources(
                "![](assets/notes/note/image.png)",
                asset_url_from_markdown_path
            ),
            "![](asset://localhost/assets/notes/note/image.png)"
        );
    }

    #[test]
    fn rewrites_markdown_image_sources_with_alt_text_and_multiple_images() {
        assert_eq!(
            rewrite_markdown_image_sources(
                "![cover](assets/notes/note/cover.png)\n![diagram](assets/notes/note/diagram.png)",
                asset_url_from_markdown_path
            ),
            "![cover](asset://localhost/assets/notes/note/cover.png)\n![diagram](asset://localhost/assets/notes/note/diagram.png)"
        );
    }

    #[test]
    fn rewrite_markdown_image_sources_ignores_links_and_preserves_broken_images() {
        assert_eq!(
            rewrite_markdown_image_sources(
                "[asset](assets/notes/note/file.png)\n![broken](assets/notes/note/missing.png",
                asset_url_from_markdown_path
            ),
            "[asset](assets/notes/note/file.png)\n![broken](assets/notes/note/missing.png"
        );
    }

    #[test]
    fn hydrates_and_restores_markdown_assets() {
        let hydrated = hydrate_markdown_assets("![alt](assets/notes/note/image.png)");
        assert_eq!(
            hydrated.markdown,
            "![alt](asset://localhost/assets/notes/note/image.png)"
        );
        assert_eq!(
            hydrated.mappings,
            vec![MarkdownImageMapping {
                display_source: "asset://localhost/assets/notes/note/image.png".to_string(),
                markdown_path: "assets/notes/note/image.png".to_string(),
            }]
        );
        assert_eq!(
            restore_markdown_asset_sources(
                &hydrated.markdown,
                &[MarkdownImageMapping {
                    display_source: "asset://localhost/assets/notes/note/image.png".to_string(),
                    markdown_path: "assets/notes/note/image.png".to_string(),
                }],
            ),
            "![alt](assets/notes/note/image.png)"
        );
    }
}
