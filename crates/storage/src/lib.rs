/// 本地持久化层：基于 SQLite 的笔记存储与同步队列管理。
pub mod repository;
pub mod sync;

pub use repository::NoteRepository;
pub use sync::{ChangeQueueItem, SyncState};
