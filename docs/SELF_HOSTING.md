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

```bash
ssh myserver
cd /opt/snapline/current
sudo docker compose --env-file /opt/snapline/.env -f deploy/compose.yml ps
sudo docker compose --env-file /opt/snapline/.env -f deploy/compose.yml logs -f api
```

## 备份

```bash
ssh myserver 'sudo sh /opt/snapline/current/deploy/backup.sh'
```

备份包含 PostgreSQL 自定义格式转储、密文对象归档和 SHA-256 清单。恢复前必须停止 API，在新数据库验证转储后再切换。每次升级前执行备份并保留上一版本源码与镜像，健康检查失败时使用上一版本执行 Compose 构建和启动。
