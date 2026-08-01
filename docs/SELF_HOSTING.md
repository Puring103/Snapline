# myServer 自部署

## 当前入口

Snapline API 部署到 SSH Host `myserver`，经现有系统 Caddy 暴露为：

```text
http://122.51.119.75/snapline/
```

这是临时 HTTP 验收入口。正式账号和数据同步前必须为服务器配置域名与 HTTPS；端到端加密保护记录内容，但明文 HTTP 仍会暴露登录凭据和令牌。

PostgreSQL 仅在 Compose 内部网络监听，API 仅绑定主机 `127.0.0.1:58080`，附件使用 Docker 持久卷保存密文对象。

Compose 中的 `object-init` 是一次性初始化服务，只负责把密文对象卷设置为 API 的非 root UID `10001`；完成后退出。`snapline-api` 始终以 UID `10001` 运行。

## 部署

本地先运行完整测试，然后执行：

```powershell
./deploy/deploy.ps1
```

首次部署在 `/opt/snapline/.env` 生成数据库密码和 JWT 密钥，权限为 `0600`。部署脚本不会覆盖已有秘密。

## 状态与日志

```powershell
./deploy/status.ps1
./deploy/logs.ps1 -Lines 200
./deploy/logs.ps1 -Follow
```

`/health/live` 只证明进程存活；`/health/ready` 会执行 PostgreSQL 查询，数据库不可用时部署健康门槛不会放行。

附件生命周期默认配置如下，可在 Compose 环境中调整：

```text
SNAPLINE_ATTACHMENT_QUOTA_BYTES=10737418240
SNAPLINE_UPLOAD_TTL_SECONDS=86400
SNAPLINE_UPLOAD_CLEANUP_INTERVAL_SECONDS=3600
```

## 备份

```powershell
./deploy/backup.ps1
```

备份包含 PostgreSQL 自定义格式转储、密文对象归档和使用相对文件名的 SHA-256 清单。每次已有版本升级前会自动备份。

## 恢复与回滚

```powershell
./deploy/restore.ps1 -Backup 20260801T092023Z -ConfirmRestore
./deploy/rollback.ps1 -Release previous -ConfirmRollback
```

两个操作默认拒绝执行，必须提供确认开关。恢复会停止 API、校验当前备份副本、重建数据库、恢复密文对象卷、修复非 root 所有权并等待健康检查。回滚会先备份当前状态，再切换 release 指针；目标版本不健康时自动恢复原 release。

2026-08-01 已在 `myserver` 使用独立 Compose 项目、独立 PostgreSQL/对象卷和回环端口 `58081` 执行同一份恢复与回滚脚本。恢复库的 9 张业务表逐表计数与备份源一致，API 健康；回滚从 `20260801T000001Z` 切换到 `20260801T000000Z` 并产生回滚前备份。所有隔离容器、卷、网络和临时目录随后删除。
