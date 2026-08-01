# Snapline 实现方案

## 1. 目标与范围

Snapline 是本地优先、端到端加密、可自部署的多模态记录工具。本阶段先完成服务端，再完成 Windows 桌面端；Android 暂不实现，但共享协议和数据模型不得阻碍后续移动端接入。

本阶段必须交付：

- 账号登录与设备管理，不支持游客模式。
- 文本、Markdown、图片粘贴、截图、录音和视频附件记录。
- 独立快速记录窗口，全局快捷键只进入记录界面。
- 全过程自动保存，不设置“保存”按钮。
- 普通标签和特殊标记；`账目`是系统预置标记，不存在独立记账功能。
- 客户端端到端加密，服务端只保存密文。
- PostgreSQL 保存账号、设备、同步信封和附件元数据。
- 附件分片上传、断点续传、完整性校验和增量同步。
- 用户提供唯一的 OpenAI 兼容 API Base URL、API Key 和模型名称。
- 单一多模态模型负责理解、转写、摘要、结构化元数据、Agent 规划与回答。
- 搜索使用受控 Agent 工具、SQLite FTS5 和结构化过滤，不使用 Embedding 或向量数据库。
- Docker Compose 部署到 SSH 已配置的 `myServer`，公网仅使用现有 HTTP/HTTPS 端口。
- 功能说明、自部署、安全与测试文档持续维护。

## 2. 产品交互

### 2.1 快速记录窗口

快速入口只显示标题、Markdown 正文、附件、普通标签、特殊标记和保存状态，不显示历史、AI、搜索、设置或侧栏。

默认快捷键：

- `Ctrl+Shift+Space`：打开文本记录并聚焦正文。
- `Ctrl+Shift+1`：区域截图，完成后进入记录窗口。
- `Ctrl+Shift+2`：打开记录窗口并立即录音。
- `Ctrl+Shift+V`：打开记录窗口并等待粘贴。
- `Esc`：关闭窗口，已输入内容保留；全空记录自动清理。

文本停止输入 300ms 后写入本地数据库；标题、标签和标记立即写入；图片、截图、音视频创建时立即建立记录和加密附件；窗口关闭前强制刷新队列；异常退出后恢复草稿。界面只显示“正在保存、已保存到本地、等待同步、已加密同步、保存失败重试中”。

### 2.2 完整主界面

- 左侧：全部记录、收藏、归档、普通标签、特殊标记。
- 中间：时间线、历史记录与搜索结果。
- 右侧：Markdown 编辑器或 AI 对话。
- 顶部：全局搜索、同步状态、AI 状态、账号和设置。

`账目`仅作为不可误删的系统预置特殊标记，无金额、分类、统计或报表逻辑。

## 3. 技术架构

```text
Tauri 2 Desktop
  React + TypeScript + CodeMirror 6
             |
  Rust desktop core
  |-- SQLCipher/SQLite + FTS5
  |-- XChaCha20-Poly1305 encryption
  |-- local attachment store
  |-- media processing queue
  |-- OpenAI-compatible client
  |-- constrained search agent
  `-- encrypted sync client
             |
        HTTPS /api/v1
             |
  Rust Axum sync server on myServer
  |-- PostgreSQL
  |-- S3-compatible object storage (MinIO)
  `-- Caddy reverse proxy
```

服务端不得持有内容解密密钥、AI API Key、明文标题、正文、标签、标记、AI 元数据或全文索引。AI 请求由已解锁客户端直接发送到用户配置的 OpenAI 兼容服务。

## 4. 仓库结构

```text
apps/desktop/                 React 桌面界面
apps/desktop/src-tauri/       Tauri 壳与平台命令
crates/domain/                共享领域类型与协议
crates/crypto/                密钥和信封加密
crates/server/                Axum HTTP 服务
crates/desktop-core/          本地存储、处理、搜索与同步
deploy/                       myServer Docker Compose/Caddy/脚本
docs/                         功能、安全、部署与测试说明
```

Rust workspace 固定依赖版本并提交 `Cargo.lock`。前端使用 npm 锁文件。

## 5. 数据与加密

### 5.1 客户端领域模型

`Item` 包含 ID、标题、Markdown、来源类型、摘要、AI 元数据、收藏/归档状态、创建/更新时间、版本、AI 状态和同步状态。`Attachment` 包含媒体类型、文件名、MIME、大小、SHA-256、转写、摘要和本地对象引用。标签与特殊标记均为多对多关系。

### 5.2 密钥层次

1. 注册设备生成随机用户主密钥 UMK 和恢复密钥。
2. 每条记录或附件生成随机 DEK。
3. DEK 使用 XChaCha20-Poly1305 加密内容。
4. UMK 包装各 DEK。
5. 用户密码经 Argon2id 派生 KEK，只用于包装 UMK。
6. 本地敏感凭据进入 Windows Credential Manager；数据库使用 SQLCipher。
7. 云端只接收包含 ciphertext、nonce、wrapped key、版本的加密信封。

修改密码只重新包装 UMK。没有恢复密钥时，遗忘密码不能恢复内容。所有 nonce 必须随机且在同一密钥下不重复，解密必须验证认证标签。

## 6. 服务端设计

### 6.1 职责

- 注册、登录、刷新与撤销会话。
- 设备注册、列举与撤销。
- 加密变更 push、按 cursor pull 和 ack。
- tombstone、版本冲突检测与幂等请求。
- 加密附件初始化、分片上传、完成、下载和删除。
- 用户隔离、限流、审计、健康检查和数据库迁移。

密码使用 Argon2id 哈希；Access Token 短期有效，Refresh Token 随机生成、数据库仅存哈希并支持轮换和撤销。

### 6.2 API

```text
POST   /api/v1/auth/register
POST   /api/v1/auth/login
POST   /api/v1/auth/refresh
POST   /api/v1/auth/logout
GET    /api/v1/devices
POST   /api/v1/devices
DELETE /api/v1/devices/{id}
POST   /api/v1/sync/push
GET    /api/v1/sync/pull?cursor=...&limit=...
POST   /api/v1/sync/ack
POST   /api/v1/attachments
PUT    /api/v1/attachments/{id}/parts/{part}
POST   /api/v1/attachments/{id}/complete
GET    /api/v1/attachments/{id}
GET    /health/live
GET    /health/ready
```

所有资源查询必须同时约束已认证 `user_id`。上传大小、分片数、游标 limit 和请求体均设上限。错误响应使用稳定机器码，不泄露数据库或认证细节。

### 6.3 PostgreSQL

表：`users`、`sessions`、`devices`、`sync_changes`、`sync_acks`、`attachments`、`attachment_parts`、`schema_migrations`。同步序号使用服务端单调递增 BIGINT；客户端对象版本用于冲突检测；请求幂等键建立用户级唯一索引。

## 7. 多模态元数据

用户只填写 API Base URL、API Key 和模型名称。配置时验证 OpenAI 格式、结构化 JSON 输出、Tool Calling，以及需要的文本/图片/音频能力。视频由客户端抽取关键帧和音轨后组成多模态请求。

模型输出经过 JSON Schema 校验，包含：摘要、转写、主题、实体、关键词、人物、地点、事件时间、语言、建议标签、建议特殊标记和规范化 `search_text`。失败任务指数退避，具备幂等键和明确状态：未配置 AI、等待、处理中、完成、失败、需重建。

没有 API 配置时，原始记录、编辑、加密和同步正常；不生成元数据、不建立内容全文索引、不开放 Agent 对话。配置后可分批补建历史元数据。API Key 仅保存在系统凭据库，不进入日志、本地普通配置、Snapline 服务端或 PostgreSQL。

## 8. Agent 搜索

不使用 Embedding。模型只能调用本地受控工具：

- `search_records(query, filters, limit)`
- `get_record(id)`
- `search_transcripts(query, filters)`
- `search_by_marker(marker, date_range)`
- `search_by_tag(tag, date_range)`
- `list_recent_records(date_range)`
- `get_attachment_metadata(id)`

工具由 SQLite FTS5 和结构化 SQL 实现。Agent 多轮调整查询后生成带记录引用的回答。必须限制工具轮数、单轮条数、总上下文、超时和可选费用预算；工具层强制当前账户作用域，模型输出不能构造任意 SQL 或文件路径。记录正文中的指令视为不可信数据，不能覆盖系统策略。

## 9. 同步与冲突

本地操作先写 SQLite 并进入 durable outbox。客户端加密后 push；服务端产生 cursor；其他设备按 cursor pull。附件分片可续传并校验 SHA-256。删除使用 tombstone，避免离线设备复活数据。

非正文属性按字段时间戳合并；Markdown 并发修改保留两个版本并创建冲突副本，禁止服务端静默覆盖。所有 push 具备幂等键，重复请求不得产生重复变更。

## 10. myServer 部署

部署目标使用现有 SSH Host `myServer`。外部仅复用已开放的 HTTP/HTTPS 端口：80 跳转、443 提供 API；若暂时只能使用明文 HTTP，只允许开发验证，正式账号登录和同步必须 HTTPS。PostgreSQL 与 MinIO 仅在 Docker 内部网络监听。

```text
/opt/snapline/
  compose.yml
  Caddyfile
  .env
  backups/
  data/postgres/
  data/objects/
```

部署流程：本地全测通过、构建带版本镜像、SSH 上传配置、备份、迁移、启动、健康检查、登录/同步冒烟、失败自动回滚。脚本提供 deploy、status、logs、backup、restore、rollback。秘密只存在服务器权限收紧的 `.env`，不提交 Git。

## 11. 分模块交付与测试门槛

每次只完整交付一个模块；功能、异常路径、测试、迁移和文档全部完成后才能进入下一模块。

### M0 工程与契约

建立 workspace、领域类型、错误规范、CI 和迁移框架。测试所有 crate 构建、Clippy、格式、TypeScript 类型和空应用启动。

### M1 服务端认证与设备

完成账号、令牌轮换、设备授权/撤销、限流和 PostgreSQL 持久化。覆盖密码哈希、枚举防护、令牌过期/重用、设备越权、并发刷新、数据库回滚和 HTTP 端到端测试。

### M2 服务端加密同步

完成 push/pull/ack、cursor、幂等、tombstone、版本冲突和分页。覆盖双用户隔离、双设备同步、重复请求、并发 push、错误 base version、删除传播和大数据分页。

### M3 服务端附件

完成分片上传、续传、校验、下载、清理和配额。覆盖越权、乱序/重复分片、错误哈希、超限、终止上传、服务重启续传和大文件流式测试。

### M4 myServer 部署

完成容器、反向代理、健康检查、迁移、备份恢复和回滚。验收 SSH、现有端口、HTTPS、容器重启持久化、客户端级登录/同步、备份恢复和旧版本升级。

### M5 桌面本地存储与加密

完成登录后本地库、UMK/DEK、恢复密钥、自动保存、崩溃恢复和加密附件。覆盖测试向量、篡改、错误密钥、密码修改、锁定读取拒绝、300ms 防抖、强制 flush 和磁盘明文扫描。

### M6 Markdown 与快速记录

完成 CodeMirror、图片内部协议、独立窗口、快捷键和启动性能。覆盖 Markdown 无损往返、XSS、粘贴、撤销、焦点、多次快捷键、防丢失及冷/热启动指标。

### M7 截图、录音与视频附件

完成屏幕选区、录音生命周期、视频导入、流式加密和临时文件清理。覆盖权限拒绝、取消、中断、设备消失、大文件、校验和、异常退出和明文残留检查。

### M8 历史、标签和特殊标记

完成时间线、收藏、归档、标签、标记及内置 `账目`。覆盖多对多关系、系统标记保护、组合筛选、分页、恢复和大列表性能。

### M9 单模型多模态处理

完成配置、能力探测、处理队列、JSON Schema、关键帧/音轨和 FTS5。使用 OpenAI 格式模拟服务器覆盖无效 Key、超时、限流、坏 JSON、能力缺失、幂等重试、无 API 降级和批量重建。

### M10 Agent 搜索与对话

完成受控工具、多轮规划、引用和安全限制。覆盖参数验证、越权、无结果、轮数/上下文限制、Prompt Injection、引用准确性和完整模拟对话。

### M11 桌面同步闭环

完成 durable outbox、双向增量、附件续传、冲突 UI 和设备撤销响应。覆盖断网、重启、重复包、多设备并发、撤销、服务器升级和真实 myServer 冒烟。

## 12. 每模块 Definition of Done

- 无占位实现或依赖下一模块才能验证的核心路径。
- 单元、集成和必要端到端测试全部通过，无关键测试 skip。
- 安全与失败路径具有可复现测试。
- 数据库变化提供向前迁移；部署模块验证回滚。
- 更新 `docs/FEATURES.md`、相关架构/安全/部署说明。
- Rust fmt、Clippy、测试、前端类型、测试与构建全部通过。
- 模块验收证据记录在 `docs/TESTING.md`，再开始下一模块。

## 13. 最终验收

Windows 桌面端必须能通过 myServer 注册/登录，离线自动保存文本和多媒体记录，加密同步到另一客户端，使用普通标签与特殊标记管理记录；用户配置一个 OpenAI 兼容多模态模型后可生成元数据，并由无向量 Agent 搜索历史内容、输出可点击引用。数据库、对象存储、日志和网络服务端均不能出现用户内容明文或 AI Key。全部测试、部署、备份恢复和需求逐项审计通过后才视为完成。
