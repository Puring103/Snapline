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

    if body_lines.first().is_some_and(|line| line.trim().is_empty()) {
        body_lines.remove(0);
    }

    DraftParts {
        title,
        body_md: normalize_markdown(&body_lines.join("\n")),
    }
}

pub fn rewrite_markdown_image_sources<F>(markdown: &str, mut transform: F) -> String
where
    F: FnMut(&str) -> String,
{
    let mut output = String::with_capacity(markdown.len());
    let mut cursor = 0usize;

    while let Some(start) = markdown[cursor..].find("![](") {
        let absolute_start = cursor + start;
        output.push_str(&markdown[cursor..absolute_start + 4]);
        let source_start = absolute_start + 4;
        let Some(end_offset) = markdown[source_start..].find(')') else {
            output.push_str(&markdown[source_start..]);
            return output;
        };
        let source_end = source_start + end_offset;
        let source = &markdown[source_start..source_end];
        output.push_str(&transform(source));
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

    HydratedMarkdown { markdown: hydrated, mappings }
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
        asset_url_from_markdown_path, compose_draft_markdown, hydrate_markdown_assets,
        markdown_path_from_asset_url, normalize_markdown, restore_markdown_asset_sources,
        rewrite_markdown_image_sources, split_draft_markdown, DraftParts, MarkdownImageMapping,
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
    fn rewrites_markdown_image_sources() {
        assert_eq!(
            rewrite_markdown_image_sources("![](assets/notes/note/image.png)", asset_url_from_markdown_path),
            "![](asset://localhost/assets/notes/note/image.png)"
        );
    }

    #[test]
    fn hydrates_and_restores_markdown_assets() {
        let hydrated = hydrate_markdown_assets("![](assets/notes/note/image.png)");
        assert_eq!(
            hydrated.markdown,
            "![](asset://localhost/assets/notes/note/image.png)"
        );
        assert_eq!(
            restore_markdown_asset_sources(
                &hydrated.markdown,
                &[MarkdownImageMapping {
                    display_source: "asset://localhost/assets/notes/note/image.png".to_string(),
                    markdown_path: "assets/notes/note/image.png".to_string(),
                }],
            ),
            "![](assets/notes/note/image.png)"
        );
    }
}
