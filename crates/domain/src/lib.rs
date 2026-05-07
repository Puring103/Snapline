/// 领域模型层：定义 Snapline 的核心数据结构，不依赖任何持久化或网络细节。
pub mod asset;
pub mod crypto;
pub mod note;
pub mod sync;

pub use asset::{AssetId, AssetMetadata, AssetRef};
pub use note::{derive_preview, derive_preview_markdown, derive_title, Note, NoteId, NoteSummary};
pub use sync::{
    AssetUploadPayload, ConflictCopyRequest, NoteChangePayload, SyncOpType, SyncPayload,
};
