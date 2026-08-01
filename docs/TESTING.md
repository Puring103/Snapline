# 测试记录

每个模块完成后记录执行命令、环境、结果和未消除风险。只有全部必需命令通过，模块状态才能更新为完成。

## M0 工程与契约

环境：Windows 11、Rust 1.95.0。

2026-08-01 已执行：

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

结果：格式检查通过，Clippy 零警告，2 个单元测试通过，0 个失败、0 个忽略。

## M1 服务端认证与设备

环境：Windows 11 Rust 客户端，通过 SSH 隧道连接 `myserver` 上仅绑定回环端口的 PostgreSQL 17 测试容器。

2026-08-01 已执行完整工作区格式、Clippy 和测试。结果：5 个单元/集成测试通过，0 个失败、0 个忽略。HTTP 集成测试覆盖注册、重复账号冲突、登录、设备列举、Refresh Token 轮换与重用拒绝、设备撤销、被撤销设备访问拒绝，以及 Refresh Token 不以明文入库。

## M2 服务端加密同步

环境同 M1。2026-08-01 已执行完整工作区格式、Clippy 和测试。结果：7 个单元/集成测试通过，0 个失败、0 个忽略。同步集成测试覆盖请求幂等、base version 冲突、密文往返、跨用户隔离和 cursor ack。

## M3 服务端附件

环境同 M1，附件使用测试临时目录。2026-08-01 已执行完整工作区格式、Clippy 和测试。附件测试覆盖密文分片、已上传分片查询、完整哈希、流式下载、跨用户不可见、原子用户配额、配额释放、24 小时过期清理及数据库/密文文件删除。就绪探针测试还覆盖 PostgreSQL 连接关闭后返回 500。

## M4 myServer 部署

2026-08-01 部署到 SSH Host `myserver`：

- Docker 离线 vendor 构建成功，API 和 PostgreSQL 容器持续运行。
- Caddy 保留原有根应用及 `/hermes`，新增 `/snapline/`。
- 公网健康检查、注册、密文 push/pull 冒烟通过，测试用户随后清理。
- PostgreSQL 和密文对象卷完成备份、SHA-256 校验；转储恢复到临时数据库后验证 10 张 public 表并清理临时库。
- API 仅绑定 `127.0.0.1:58080`，PostgreSQL 无主机端口。
- 部署脚本具备 release 指针、Caddy 备份、三次连续健康门槛和失败回滚。
- `status.ps1`、`logs.ps1`、`backup.ps1` 在真实服务器通过；`restore.ps1` 与 `rollback.ps1` 无确认开关时正确拒绝。
- 相对 SHA-256 清单在复制后的备份目录独立校验；同一 `restore.sh` 在隔离项目恢复 9 张业务表和对象卷，API 在 `58081` 健康。
- 同一 `rollback.sh` 在隔离项目执行升级前备份、release 指针切换和健康检查。两套隔离资源均已清理。

已知部署约束：当前入口是临时 HTTP，正式输入真实账号凭据前仍需域名和 HTTPS。

## M5 桌面本地存储与加密

环境：Windows 11、Rust 1.95.0、Tauri 2.11、WebView2、Windows Credential Manager。

2026-08-01 已执行：

```powershell
cargo clippy -p snapline-crypto -p snapline-desktop-core -p snapline-desktop --all-targets -- -D warnings
cargo test -p snapline-crypto -p snapline-desktop-core -p snapline-desktop
$env:SNAPLINE_LIVE_TEST='1'
cargo test -p snapline-desktop live_server_register_unlock_save_and_login_again -- --ignored --nocapture
npm test -- --run
npm run build
npm run tauri -- build --no-bundle
```

结果：

- 加密 crate 5 个测试通过，覆盖密码/恢复密钥解锁、错误密码、密文篡改、对象替换、跨分块附件往返、截断和错误密钥。
- 本地仓库 5 个测试通过，覆盖记录 CRUD/版本、错误 UMK、数据库明文扫描、加密附件往返、磁盘明文扫描和密文篡改。
- Tauri 原生边界 3 个常规测试通过，另有 1 个真实 myServer 测试通过；真实测试覆盖注册、Windows Credential Manager、保存本地加密记录、再次登录和重新解密。测试账号完成后已精确删除。
- Windows Release 可执行文件构建成功，并在真实 WebView2 窗口中完成登录界面启动与视觉验收。
- React 2 个交互测试和生产构建通过；CodeMirror 主包仍有非阻塞体积警告，后续在 M6 做按需拆包。

安全边界：UMK 只存在于原生进程内存，退出时随状态释放并零化；Refresh Token 只写入 Windows Credential Manager；记录敏感字段和附件内容均以 XChaCha20-Poly1305 加密后落盘。当前公网 API 仍为临时 HTTP，只使用专用随机测试凭据完成验证，正式使用真实账号前必须切换 HTTPS。

## M6 Markdown 与快速记录

环境：Windows 11、Tauri 2.11、WebView2、CodeMirror 6。

2026-08-01 已验收：

- Vitest 3 个测试文件、9 个交互/工具测试通过，覆盖自动保存、空快捷草稿不落盘、特殊标记、`账目` 仅作为普通标记、Markdown 格式化、撤销和克隆返回对象不导致保存循环。
- 快捷记录使用轻量 `textarea`，完整 CodeMirror 按需加载；首屏 JS 由约 850 kB 降至 229 kB，生产构建通过。
- 真实 Release 中 `Ctrl+Shift+1` 从其他应用打开唯一的 `Snapline 快速记录` 窗口，重复触发保持同一窗口 ID。未登录时正确显示登录页，不允许绕过认证。
- 本机 `Ctrl+Shift+Space` 已被现有 `snapline-client` 进程占用；已验证 Snapline 会报告该组合不可用但不崩溃，其他快捷键仍正常注册。

## M7 桌面媒体

2026-08-01 已验收：

```powershell
$env:SNAPLINE_MEDIA_TEST='1'
cargo test -p snapline-desktop --offline live_windows_screen_is_encrypted_and_decodable -- --ignored --nocapture
cargo test -p snapline-desktop-core -p snapline-desktop --offline
cargo clippy -p snapline-desktop-core -p snapline-desktop --all-targets --offline -- -D warnings
```

- 真实主屏截图成功，加密落盘后解密校验 PNG 文件头并完整解码。
- 麦克风 PCM 样本仅在内存缓冲，WAV 仅在内存编码后直接加密；样本转换、WAV 往返和 30 分钟上限逻辑通过测试。当前自动化环境无输入设备，真实录音返回可操作的 `未找到可用麦克风` 错误，硬件实录需在有麦克风的 Windows 机器复验。
- 跨多个 1 MiB 加密分块的视频导入、解密及字节一致性测试通过；粘贴图片限制 32 MiB，文件导入限制 2 GiB。
- `snapline-attachment` 协议要求已解锁会话，支持最大 64 MiB 的 Range 分段响应；跨密文分块范围读取、长度推导、后缀范围、越界和篡改拒绝通过测试。图片/音频/视频在记录中直接预览，不创建明文临时文件。
- Tauri capability 仅授权 `main` 和 `capture` 窗口，CSP 仅额外放行本应用附件协议。

## M8 历史、标签和特殊标记

2026-08-01 已验收：

```powershell
cd apps/desktop
npm test -- --run
npm run build
cd ../..
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --offline -- -D warnings
cargo test --workspace --offline
```

- Vitest 5 个测试文件、19 个交互/工具测试通过。M8 覆盖可搜索历史抽屉、120 条记录的 50 条稳定分页、历史跳转、收藏与取消收藏、归档与恢复、删除二次确认、自定义特殊标记的规范化和自动保存。
- 来源类型采用包含匹配；多个普通标签和特殊标记分别采用 AND 匹配。测试覆盖 `图片 + 账目 + #项目` 的组合命中、加入不相容标记后的空结果以及一键清除恢复。
- `账目` 始终作为系统预置特殊标记提供，但其记录行为与用户自定义标记完全一致，不存在金额、收支、统计或报表逻辑。
- 前端生产构建通过，完整 CodeMirror 仍保持独立懒加载 chunk；其体积警告不影响快速记录首屏。

## M9 单模型多模态处理

2026-08-01 已验收：

```powershell
cd apps/desktop
npm test -- --run
npm run build
cd ../..
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --offline -- -D warnings
$env:SNAPLINE_MEDIA_AI_TEST='1'
cargo test -p snapline-desktop --offline pinned_ffmpeg_extracts_encrypted_video_without_plaintext_output -- --ignored --nocapture
cargo test --workspace --offline
```

- Vitest 6 个测试文件、22 个交互/工具测试通过。覆盖设置弹窗、首次 Key 要求、已有配置留空沿用 Key、开发环境不持久化 Key、无 API 配置仍可自动保存，以及内容修改使旧元数据失效。
- Rust 常规测试覆盖 HTTPS/回环 URL 限制、结构化字段和长度校验、OpenAI 兼容模拟服务、图片和音频能力探测、无效 Key、429、坏 JSON、持久作业认领/完成/重建及指数退避状态。
- 元数据摘要和 `search_text` 经记录 DEK 加密后落盘；磁盘扫描未发现测试摘要或索引文本。FTS5 仅在内存中重建并正确命中，未引入 Embedding 或向量数据库。
- 固定 FFmpeg ZIP 的下载 URL 与 SHA-256 受测试保护。真实下载完成校验后，测试视频先加密并删除明文，再从密文 stdin 抽取 JPEG 关键帧和完整 MP3 音轨；断言没有恢复明文源视频。
- 图片、音频、视频和严格 JSON Schema 使用同一个用户填写的模型。视频最多抽取每分钟一帧、30 帧；30 分钟录音压缩为单声道 32 kbit/s MP3 后处理。处理数据由客户端直连用户的 AI 服务，API Key 不进入 Snapline 服务端。

## M10 Agent 搜索与对话

2026-08-01 已验收：

```powershell
cd apps/desktop
npm test -- --run
npm run build
cd ../..
cargo fmt --all -- --check
cargo clippy -p snapline-desktop-core -p snapline-desktop --all-targets --offline -- -D warnings
cargo test -p snapline-desktop --offline
```

- Vitest 7 个测试文件、26 个交互/工具测试通过。覆盖发送问题、回答与引用渲染、引用跳转、4000 字符限制、失败后保留已有对话、未配置状态和生产构建。
- Rust 常规测试 18 个通过，3 个需要真实媒体、桌面或 `myserver` 的测试显式忽略。Agent 测试覆盖两轮工具调用、引用只来自实际交给模型的记录、记录提示注入按不可信数据传递、6 轮上限、工具白名单、拒绝额外路径/SQL 参数、RFC3339 日期校验、20 条结果上限和 12000 字符正文截断。
- Agent 只有 `search_records`、`get_record`、`search_transcripts`、`search_by_marker`、`search_by_tag`、`list_recent_records`、`get_attachment_metadata` 七个只读工具。最多 6 轮、每轮 8 次调用、总工具上下文 120000 字符；不能执行任意 SQL、命令或读取文件路径。
- 搜索使用已解锁进程内存中的 FTS5 与解密后的结构化过滤，不使用 Embedding 或向量数据库。API Key 保持在 Windows Credential Manager，Snapline `myserver` 服务端不参与 AI 请求。
- 完整工作区测试通过 SSH 隧道连接 `myserver` 上仅绑定回环端口的一次性 PostgreSQL 17 容器，合计 40 个测试通过、0 个失败、3 个条件忽略；测试后隧道与容器均已删除，正式数据库未被修改。

## M11 桌面同步闭环

2026-08-01 已验收：

- 本地 SQLite 新增独立服务端版本、pull cursor 和加密冲突表；outbox 按对象合并最新操作，但只有服务端确认后才删除。测试覆盖双仓库密文往返、服务端版本推进、本地/远端冲突选择，以及 push 响应丢失后保留更新的本地内容。
- Access Token 在到期前两分钟使用 Windows Credential Manager 中的 Refresh Token 轮换；401 会强制刷新一次。设备撤销或刷新失效会删除凭据并清空 UMK、仓库和会话状态。
- 每次登录、自动保存、删除、快捷记录完成、恢复联网及每 30 秒触发同步。pull 分页应用后持久化 cursor 并 ack；push 使用由设备和 outbox 序列派生的稳定幂等键，校验逐项确认对象。
- 附件先于引用记录上传，使用 8 MiB 分片并跳过服务端已持有分片。加密附件元数据由 UMK 认证加密；下载流式写入临时密文文件，大小、SHA-256 和对象 ID 验证通过后原子导入，不产生明文临时文件。
- Vitest 8 个测试文件、27 个测试通过；Rust 最终完整工作区在 `myserver` 一次性 PostgreSQL 17 上共 46 个常规测试通过、0 失败、4 个条件忽略；Clippy 零警告，生产前端构建通过。
- 更新后的服务端已部署到 `myserver`，API 仍仅绑定 `127.0.0.1:58080`，PostgreSQL 无主机端口。对象卷由一次性 root 初始化容器设置给非 root API UID `10001`，API 继续以非 root 运行。
- 真实 `myserver` 双设备测试通过：设备 A 上传一条记录和约 9 MiB 加密视频，设备 B 增量拉取、校验并完整解密；撤销设备 A 后刷新被拒绝且本地会话清空。两个测试账号和对应密文对象目录均已精确删除。
- 最终双设备复验增加删除生命周期：设备 B 的记录 tombstone 获确认后才放行附件删除；服务端对象返回 404，本地密文移除，测试账号与空对象目录精确清理。
- 当前公网入口仍为开发用 HTTP。测试仅使用随机临时凭据与内容；正式账号、真实记录和长期同步必须先配置 HTTPS。

## Windows Release

2026-08-01 执行完整 NSIS 构建成功，安装包位于 `target/release/bundle/nsis/Snapline_0.1.0_x64-setup.exe`。静默安装到一次性目录和静默卸载均返回 0，安装内容包含桌面可执行文件与卸载器。前端主包约 247 kB，完整 CodeMirror 保持为独立懒加载 chunk；其体积提示不影响快速入口首屏。

最终安装包大小为 4,418,962 字节，SHA-256 为 `6164EA0FE13B014493FAECBBFDF402549586A4D144916116C9BE5A506E475C9E`。

当前安装包未配置 Authenticode 证书，`Get-AuthenticodeSignature` 返回 `NotSigned`。这不影响功能测试，但正式对外分发前需要签名以消除未知发布者警告。
