/// 领域模型层：定义 Snapline 的核心数据结构，不依赖任何持久化或网络细节。
pub mod asset;
pub mod crypto;
pub mod markdown;
pub mod note;
pub mod sync;

pub use asset::{AssetId, AssetMetadata, AssetRef};
pub use markdown::{
    asset_url_from_markdown_path, compose_draft_markdown, has_meaningful_draft_content,
    hydrate_markdown_assets, markdown_path_from_asset_url, normalize_markdown,
    restore_markdown_asset_sources, rewrite_markdown_image_sources, split_draft_markdown,
    split_stored_note_markdown, DraftParts, HydratedMarkdown, MarkdownImageMapping,
};
pub use note::{
    derive_preview, derive_preview_markdown, derive_title, summarize_note, Note, NoteId,
    NoteSummary,
};
pub use sync::{
    AssetUploadPayload, ConflictCopyRequest, NoteChangePayload, SyncOpType, SyncPayload,
};
