param([string]$SshHost = "myserver", [int]$Lines = 200, [switch]$Follow)
$ErrorActionPreference = "Stop"
if ($Lines -lt 1 -or $Lines -gt 10000) { throw "Lines must be between 1 and 10000" }
$followFlag = if ($Follow) { "--follow" } else { "" }
ssh $SshHost "cd /opt/snapline/current && sudo docker compose --env-file /opt/snapline/.env -f deploy/compose.yml logs --tail=$Lines $followFlag api"
if ($LASTEXITCODE -ne 0) { throw "Unable to read Snapline logs" }
