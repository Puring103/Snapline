param([string]$SshHost = "myserver")
$ErrorActionPreference = "Stop"
ssh $SshHost 'sudo sh /opt/snapline/current/deploy/backup.sh'
if ($LASTEXITCODE -ne 0) { throw "Snapline backup failed" }
