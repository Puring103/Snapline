param([string]$SshHost = "myserver")
$ErrorActionPreference = "Stop"
ssh $SshHost 'set -eu; cd /opt/snapline/current; sudo docker compose --env-file /opt/snapline/.env -f deploy/compose.yml ps -a; curl --fail --silent http://127.0.0.1/snapline/health/ready'
if ($LASTEXITCODE -ne 0) { throw "Snapline status check failed" }
