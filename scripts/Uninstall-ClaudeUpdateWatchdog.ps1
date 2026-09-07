<#
.SYNOPSIS
    Removes the "Claude Update Watchdog" scheduled task and its files.
    -KeepLogs keeps watchdog.log / state.json and removes only the script.
#>
[CmdletBinding()]
param(
    [string]$TaskName = 'Claude Update Watchdog',
    [switch]$KeepLogs
)

$ErrorActionPreference = 'Stop'

$task = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
if ($task) {
    Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
    Write-Output ('Removed scheduled task "{0}".' -f $TaskName)
} else {
    Write-Output ('Scheduled task "{0}" was not found.' -f $TaskName)
}

$dirs = @(
    (Join-Path ([Environment]::GetFolderPath('UserProfile')) 'ClaudeUpdateWatchdog'),
    (Join-Path $env:LOCALAPPDATA 'ClaudeUpdateWatchdog'),   # location used by an earlier installer version
    (Join-Path $env:ProgramData 'ClaudeUpdateWatchdog')
)
foreach ($d in $dirs) {
    if (-not (Test-Path $d)) { continue }
    if ($KeepLogs) {
        Remove-Item -Path (Join-Path $d 'Claude-UpdateWatchdog.ps1') -Force -ErrorAction SilentlyContinue
        Write-Output ('Removed the script from {0} (logs kept).' -f $d)
    } else {
        try { Remove-Item -Path $d -Recurse -Force; Write-Output ('Removed {0}' -f $d) }
        catch { Write-Warning ('Could not remove {0}: {1}' -f $d, $_.Exception.Message) }
    }
}
