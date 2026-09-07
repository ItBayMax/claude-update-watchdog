<#
.SYNOPSIS
    Installs the "Claude Update Watchdog" scheduled task.

.DESCRIPTION
    Default: installs for the current user only (no admin rights needed).
      - copies Claude-UpdateWatchdog.ps1 to %USERPROFILE%\ClaudeUpdateWatchdog
      - registers a task that runs the script (-FromEvent) 3 seconds after
        Microsoft-Windows-AppModel-Runtime/Admin event 208 is logged for Claude.
      - runs the task once as a self-test (skip with -NoTest).

    -AllUsers: needs an elevated PowerShell. Copies the script to
      %ProgramData%\ClaudeUpdateWatchdog and registers the task for BUILTIN\Users,
      so it runs in whichever interactive session triggers the event.

    Why not %LOCALAPPDATA%: if this installer is run from a terminal inside the Claude
    desktop app, MSIX file-system virtualization redirects AppData writes into the
    package's LocalCache and the scheduled task (outside the container) cannot find
    the script (Task Scheduler then reports 0xFFFD0000).

    Uninstall with Uninstall-ClaudeUpdateWatchdog.ps1.
#>
[CmdletBinding()]
param(
    [switch]$AllUsers,
    [switch]$NoTest,
    [string]$TaskName = 'Claude Update Watchdog'
)

$ErrorActionPreference = 'Stop'

$source = Join-Path $PSScriptRoot 'Claude-UpdateWatchdog.ps1'
if (-not (Test-Path $source)) { throw 'Claude-UpdateWatchdog.ps1 must be in the same folder as this installer.' }

$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if ($AllUsers -and -not $isAdmin) { throw '-AllUsers requires an elevated (Administrator) PowerShell.' }

if ($AllUsers) { $targetDir = Join-Path $env:ProgramData 'ClaudeUpdateWatchdog' }
else           { $targetDir = Join-Path ([Environment]::GetFolderPath('UserProfile')) 'ClaudeUpdateWatchdog' }
New-Item -ItemType Directory -Path $targetDir -Force | Out-Null
$script = Join-Path $targetDir 'Claude-UpdateWatchdog.ps1'
Copy-Item -Path $source -Destination $script -Force

# Remove a copy left by an earlier version of this installer under %LOCALAPPDATA%.
$legacy = Join-Path $env:LOCALAPPDATA 'ClaudeUpdateWatchdog'
if (Test-Path (Join-Path $legacy 'Claude-UpdateWatchdog.ps1')) {
    Remove-Item -Path $legacy -Recurse -Force -ErrorAction SilentlyContinue
}

# AUMID of Claude Desktop (MSIX). The family-name suffix is derived from the publisher
# certificate, so it is the same on every machine.
$aumid = 'Claude_pzs8sxrjxfjjc!Claude'
$pkg = Get-AppxPackage -Name 'Claude' -ErrorAction SilentlyContinue | Select-Object -First 1
if ($pkg) { $aumid = '{0}!Claude' -f $pkg.PackageFamilyName }
else      { Write-Warning ('Claude MSIX package is not registered for this user; using default AUMID {0}.' -f $aumid) }

$xpath = "*[System[Provider[@Name='Microsoft-Windows-AppModel-Runtime'] and EventID=208]] and *[EventData[Data[@Name='ApplicationName']='$aumid']]"
$subscription = "<QueryList><Query Id='0' Path='Microsoft-Windows-AppModel-Runtime/Admin'><Select Path='Microsoft-Windows-AppModel-Runtime/Admin'>$xpath</Select></Query></QueryList>"

$triggerClass = Get-CimClass -ClassName MSFT_TaskEventTrigger -Namespace Root/Microsoft/Windows/TaskScheduler
$trigger = New-CimInstance -CimClass $triggerClass -ClientOnly
$trigger.Subscription = $subscription
$trigger.Enabled = $true
$trigger.Delay = 'PT3S'

$psExe  = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
$action = New-ScheduledTaskAction -Execute $psExe -Argument ('-NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File "{0}" -FromEvent' -f $script)

$settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -StartWhenAvailable `
            -MultipleInstances IgnoreNew -ExecutionTimeLimit (New-TimeSpan -Minutes 5)

if ($AllUsers) {
    $principal = New-ScheduledTaskPrincipal -GroupId 'BUILTIN\Users' -RunLevel Limited
} else {
    $principal = New-ScheduledTaskPrincipal -UserId ([Security.Principal.WindowsIdentity]::GetCurrent().Name) -LogonType Interactive -RunLevel Limited
}

$description = 'Workaround for Claude Desktop self-update failures: when Windows logs AppModel-Runtime event 208 (0x80070020) for Claude, stop leftover Claude processes and relaunch Claude.'

$task = Register-ScheduledTask -TaskName $TaskName -TaskPath '\' -Action $action -Trigger $trigger `
        -Settings $settings -Principal $principal -Description $description -Force

$scope = 'current user'
if ($AllUsers) { $scope = 'all users' }
Write-Output ('Installed scheduled task "{0}" for {1}.' -f $task.TaskName, $scope)
Write-Output ('Script : {0}' -f $script)
Write-Output  'Log    : %USERPROFILE%\ClaudeUpdateWatchdog\watchdog.log (per user)'
Write-Output ('Trigger: Microsoft-Windows-AppModel-Runtime/Admin, EventID 208, ApplicationName = {0}, delay 3 s' -f $aumid)
Write-Output  'Remove : Uninstall-ClaudeUpdateWatchdog.ps1'

if (-not $NoTest) {
    Write-Output ''
    Write-Output 'Self-test: running the task once (no recent failure, so it should only log "nothing to do")...'
    Start-ScheduledTask -TaskName $TaskName
    $deadline = (Get-Date).AddSeconds(90)
    do {
        Start-Sleep -Seconds 3
        $state = (Get-ScheduledTask -TaskName $TaskName).State
    } while (($state -eq 'Running') -and ((Get-Date) -lt $deadline))
    $info = Get-ScheduledTaskInfo -TaskName $TaskName
    $verdict = 'FAILED - see the log'
    if ($info.LastTaskResult -eq 0) { $verdict = 'OK' }
    Write-Output ('Self-test result: LastTaskResult = 0x{0:X8} ({1})' -f $info.LastTaskResult, $verdict)
    $logPath = Join-Path (Join-Path ([Environment]::GetFolderPath('UserProfile')) 'ClaudeUpdateWatchdog') 'watchdog.log'
    if (Test-Path $logPath) { Write-Output ('Last log line   : {0}' -f (Get-Content $logPath -Tail 1)) }
    else { Write-Output 'Log file was not created - the task did not run the script.' }
}
