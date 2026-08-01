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
