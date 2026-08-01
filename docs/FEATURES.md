# 功能状态

状态说明：`计划`、`进行中`、`完成`。

| 模块 | 状态 | 验收证据 |
| --- | --- | --- |
| M0 工程与契约 | 完成 | `cargo fmt`、`cargo clippy`、`cargo test` 全部通过 |
| M1 服务端认证与设备 | 完成 | 真实 PostgreSQL HTTP 生命周期测试通过 |
| M2 服务端加密同步 | 完成 | 幂等、隔离、冲突、pull/ack 真实 PostgreSQL 测试通过 |
| M3 服务端附件 | 完成 | 真实 PostgreSQL 分片、状态、哈希、下载和隔离测试通过 |
| M4 myServer 部署 | 进行中 | 健康、登录、同步、备份恢复冒烟 |
| M5-M11 桌面端 | 计划 | 见 `IMPLEMENTATION_PLAN.md` |
