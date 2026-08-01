param(
    [string]$SshHost = "myserver"
)

$ErrorActionPreference = "Stop"
$projectRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$archive = Join-Path ([System.IO.Path]::GetTempPath()) "snapline-deploy.tar.gz"

try {
    Push-Location $projectRoot
    $deploymentInputs = @(
        "Cargo.toml", "Cargo.lock", ".cargo", "crates", "deploy", "docs", "vendor",
        ".env.example", ".gitignore", ".dockerignore", "README.md"
    )
    if (Test-Path "apps") { $deploymentInputs += "apps" }
    tar -czf $archive --exclude='apps/desktop/node_modules' --exclude='apps/desktop/dist' --exclude='*.log' $deploymentInputs
    if ($LASTEXITCODE -ne 0) { throw "Failed to create deployment archive" }
    Pop-Location

    scp $archive "${SshHost}:/tmp/snapline-deploy.tar.gz"
    if ($LASTEXITCODE -ne 0) { throw "Failed to upload deployment archive" }

    ssh $SshHost @'
set -eu
release=$(date -u +%Y%m%dT%H%M%SZ)
release_dir="/opt/snapline/releases/$release"
previous_release=$(readlink -f /opt/snapline/current 2>/dev/null || true)
sudo mkdir -p "$release_dir" /opt/snapline/backups
sudo tar -xzf /tmp/snapline-deploy.tar.gz -C "$release_dir"
if [ ! -f /opt/snapline/.env ]; then
  db_password=$(openssl rand -hex 32)
  jwt_secret=$(openssl rand -hex 48)
  printf 'SNAPLINE_POSTGRES_PASSWORD=%s\nSNAPLINE_JWT_SECRET=%s\n' "$db_password" "$jwt_secret" | sudo tee /opt/snapline/.env >/dev/null
  sudo chmod 600 /opt/snapline/.env
fi
cd "$release_dir"
if sudo docker image inspect snapline-api:latest >/dev/null 2>&1; then
  sudo docker image tag snapline-api:latest snapline-api:rollback
fi
sudo docker compose --env-file /opt/snapline/.env -f deploy/compose.yml build
sudo caddy validate --config "$release_dir/deploy/Caddyfile" --adapter caddyfile
if ! sudo cmp -s "$release_dir/deploy/Caddyfile" /etc/caddy/Caddyfile; then
  sudo cp /etc/caddy/Caddyfile "/opt/snapline/backups/Caddyfile.$release"
  sudo install -o root -g root -m 644 "$release_dir/deploy/Caddyfile" /etc/caddy/Caddyfile
fi
sudo ln -sfn "$release_dir" /opt/snapline/current
sudo docker compose --env-file /opt/snapline/.env -f /opt/snapline/current/deploy/compose.yml up -d
sudo systemctl reload caddy
healthy=0
for attempt in $(seq 1 30); do
  if curl --fail --silent http://127.0.0.1/snapline/health/ready >/dev/null; then
    healthy=$((healthy + 1))
    if [ "$healthy" -ge 3 ]; then
      sudo docker compose --env-file /opt/snapline/.env -f /opt/snapline/current/deploy/compose.yml ps
      exit 0
    fi
  else
    healthy=0
  fi
  sleep 2
done
echo 'Snapline health check failed' >&2
if [ -n "$previous_release" ]; then
  sudo ln -sfn "$previous_release" /opt/snapline/current
  if sudo docker image inspect snapline-api:rollback >/dev/null 2>&1; then
    sudo docker image tag snapline-api:rollback snapline-api:latest
  fi
  if [ -f "/opt/snapline/backups/Caddyfile.$release" ]; then
    sudo install -o root -g root -m 644 "/opt/snapline/backups/Caddyfile.$release" /etc/caddy/Caddyfile
    sudo systemctl reload caddy
  fi
  sudo docker compose --env-file /opt/snapline/.env -f /opt/snapline/current/deploy/compose.yml up -d --force-recreate
fi
exit 1
'@
    if ($LASTEXITCODE -ne 0) { throw "Remote deployment failed" }
}
finally {
    if (Test-Path $archive) { Remove-Item -LiteralPath $archive -Force }
    while ((Get-Location).Path -ne $projectRoot.Path) { Pop-Location }
}
