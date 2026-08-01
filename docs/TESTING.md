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

环境同 M1，附件使用测试临时目录。2026-08-01 已执行完整工作区格式、Clippy 和测试。结果：9 个单元/集成测试通过，0 个失败、0 个忽略。附件测试覆盖密文分片、已上传分片查询、完整哈希验证、流式下载和跨用户不可见。

## M4 myServer 部署

2026-08-01 部署到 SSH Host `myserver`：

- Docker 离线 vendor 构建成功，API 和 PostgreSQL 容器持续运行。
- Caddy 保留原有根应用及 `/hermes`，新增 `/snapline/`。
- 公网健康检查、注册、密文 push/pull 冒烟通过，测试用户随后清理。
- PostgreSQL 和密文对象卷完成备份、SHA-256 校验；转储恢复到临时数据库后验证 10 张 public 表并清理临时库。
- API 仅绑定 `127.0.0.1:58080`，PostgreSQL 无主机端口。
- 部署脚本具备 release 指针、Caddy 备份、三次连续健康门槛和失败回滚。

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
