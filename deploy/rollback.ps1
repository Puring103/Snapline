param(
    [string]$Release = "previous",
    [string]$SshHost = "myserver",
    [switch]$ConfirmRollback
)
$ErrorActionPreference = "Stop"
if ($Release -ne "previous" -and $Release -notmatch '^[0-9]{8}T[0-9]{6}Z$') { throw "Release must be 'previous' or a release timestamp" }
if (-not $ConfirmRollback) { throw "Rollback changes the active server release. Re-run with -ConfirmRollback." }
ssh $SshHost "sudo sh /opt/snapline/current/deploy/rollback.sh '$Release'"
if ($LASTEXITCODE -ne 0) { throw "Snapline rollback failed" }
