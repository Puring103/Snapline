use snapline_domain::{
    asset_url_from_markdown_path, compose_draft_markdown, derive_title, hydrate_markdown_assets,
    markdown_path_from_asset_url, normalize_markdown, restore_markdown_asset_sources,
    split_draft_markdown, DraftParts, HydratedMarkdown, MarkdownImageMapping,
};

use crate::AppCore;

impl AppCore {
    pub fn derive_title_from_markdown(&self, markdown: &str) -> String {
        derive_title(markdown)
    }

    pub fn compose_draft_markdown(&self, title: &str, body_md: &str) -> String {
        compose_draft_markdown(title, body_md)
    }

    pub fn split_draft_markdown(&self, markdown: &str) -> DraftParts {
        split_draft_markdown(markdown)
    }

    pub fn prepare_draft_for_save(&self, title: &str, body_md: &str) -> DraftParts {
        let markdown = compose_draft_markdown(title, body_md);
        split_draft_markdown(&markdown)
    }

    pub fn normalize_markdown(&self, markdown: &str) -> String {
        normalize_markdown(markdown)
    }

    pub fn asset_url_from_markdown_path(&self, markdown_path: &str) -> String {
        asset_url_from_markdown_path(markdown_path)
    }

    pub fn markdown_path_from_asset_url(&self, asset_url: &str) -> String {
        markdown_path_from_asset_url(asset_url)
    }

    pub fn hydrate_markdown_assets(&self, markdown: &str) -> HydratedMarkdown {
        hydrate_markdown_assets(markdown)
    }

    pub fn restore_markdown_asset_sources(
        &self,
        markdown: &str,
        mappings: &[MarkdownImageMapping],
    ) -> String {
        restore_markdown_asset_sources(markdown, mappings)
    }
}
