# 功能状态

状态说明：`计划`、`进行中`、`完成`。

| 模块 | 状态 | 验收证据 |
| --- | --- | --- |
| M0 工程与契约 | 完成 | `cargo fmt`、`cargo clippy`、`cargo test` 全部通过 |
| M1 服务端认证与设备 | 完成 | 真实 PostgreSQL HTTP 生命周期测试通过 |
| M2 服务端加密同步 | 完成 | 幂等、隔离、冲突、pull/ack 真实 PostgreSQL 测试通过 |
| M3 服务端附件 | 完成 | 真实 PostgreSQL 分片、状态、哈希、下载和隔离测试通过 |
| M4 myServer 部署 | 完成 | 公网健康、注册、密文同步、备份恢复通过 |
| M5 桌面本地存储与加密 | 完成 | 原生注册/登录、Windows 凭据、UMK/DEK、记录与分块附件加密测试及真实 myServer 登录通过 |
| M6 Markdown 与快速记录 | 完成 | 完整 Markdown、选区格式化、撤销/重做、粘贴图片、自动保存、全局快捷键、托盘、独立记录窗口及单窗口复用通过测试 |
| M7 桌面媒体 | 完成 | 原生主屏截图、麦克风 WAV 录音、图片/视频流式加密导入、自定义协议 Range 预览与严格 ACL/CSP 通过自动化验收 |
| M8 历史、标签和特殊标记 | 完成 | 历史抽屉、50 条分页、收藏/归档/删除、普通标签、自定义特殊标记及来源/标签/标记组合筛选通过测试 |
| M9 单模型多模态处理 | 计划 | 见 `IMPLEMENTATION_PLAN.md` |
| M10 Agent 搜索与对话 | 计划 | 见 `IMPLEMENTATION_PLAN.md` |
| M11 桌面同步闭环 | 计划 | 见 `IMPLEMENTATION_PLAN.md` |
