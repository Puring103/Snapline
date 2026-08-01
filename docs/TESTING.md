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
