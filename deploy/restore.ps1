param(
    [Parameter(Mandatory = $true)][ValidatePattern('^[0-9]{8}T[0-9]{6}Z$')][string]$Backup,
    [string]$SshHost = "myserver",
    [switch]$ConfirmRestore
)
$ErrorActionPreference = "Stop"
if (-not $ConfirmRestore) { throw "Restore replaces the current database and object volume. Re-run with -ConfirmRestore." }
ssh $SshHost "sudo sh /opt/snapline/current/deploy/restore.sh '$Backup'"
if ($LASTEXITCODE -ne 0) { throw "Snapline restore failed" }
