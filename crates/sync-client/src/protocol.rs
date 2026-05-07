/// 客户端与服务端之间的 HTTP 协议数据结构。
///
/// 这些类型同时被 `sync-client`（发送方）和 `sync-server`（接收方）共享，
/// 修改时需确保两端一致。
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use snapline_domain::{AssetMetadata, AssetUploadPayload, Note, NoteId, SyncPayload};

/// 登录请求体（同时用于注册和登录）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
    /// 发起请求的设备唯一 ID（UUID）。
    pub device_id: String,
    pub device_name: String,
}

/// 登录成功响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    pub account_id: String,
    /// Bearer token，后续所有需鉴权的请求均须携带。
    pub access_token: String,
}

/// 批量推送变更的请求体。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushRequest {
    /// 发起推送的设备 ID（服务端用于过滤拉取时的自身变更）。
    pub device_id: String,
    pub changes: Vec<PushChange>,
}

/// 单条推送变更。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushChange {
    /// 本地队列条目 ID，用于匹配响应中的结果。
    pub queue_id: String,
    pub note_id: NoteId,
    /// 推送时的基准版本号，服务端据此判断是否冲突。
    pub base_version: i64,
    pub payload: SyncPayload,
}

/// 单条推送变更的处理结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PushChangeResult {
    /// 服务端已接受，返回新版本号和游标。
    Accepted {
        queue_id: String,
        note_id: NoteId,
        server_version: i64,
        cursor: i64,
    },
    /// 版本冲突：服务端当前版本与 `base_version` 不符，附带服务端当前笔记。
    Conflict {
        queue_id: String,
        note_id: NoteId,
        server_note: Note,
    },
}

/// 批量推送的响应体。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushResponse {
    pub results: Vec<PushChangeResult>,
}

/// 增量拉取响应：返回指定游标之后的所有变更。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullResponse {
    /// 本批次最新游标，客户端保存后作为下次拉取的起点。
    pub cursor: i64,
    pub changes: Vec<RemoteChange>,
}

/// 服务端下发的单条远端变更。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteChange {
    /// 此变更在服务端 change_log 中的游标位置。
    pub cursor: i64,
    /// 产生此变更的设备 ID（客户端用此跳过自身的回显）。
    pub device_id: String,
    pub note: Note,
    pub changed_at: DateTime<Utc>,
}

/// 全量快照响应（首次登录或客户端丢失状态时使用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotResponse {
    /// 快照对应的服务端游标，后续增量拉取从此游标开始。
    pub cursor: i64,
    pub notes: Vec<Note>,
    pub assets: Vec<AssetMetadata>,
}

/// 资源上传请求（内部结构，不序列化为 JSON；通过 multipart form 发送）。
#[derive(Debug, Clone)]
pub struct AssetUploadRequest {
    /// 资源元数据（序列化为 multipart `metadata` 字段）。
    pub metadata: AssetUploadPayload,
    /// 文件字节（multipart `file` 字段）。
    pub bytes: Vec<u8>,
}

/// 资源下载响应。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetDownload {
    pub content_type: String,
    pub bytes: Vec<u8>,
}
