<#
.SYNOPSIS
    Claude Desktop (MSIX) update watchdog - workaround for the "another program is
    using this file" (0x80070020) launch failure after Claude's silent self-update.

.DESCRIPTION
    Observed on Windows 11 with Claude Desktop 1.44121 - 1.46388:
      1. Claude Desktop stages a new MSIX version, then quits and relaunches itself
         once the user has been idle for a while ("stealth update").
      2. At least one process of the OLD version survives that quit, so the old
         version's Desktop AppX container stays alive.
      3. Windows refuses to create the NEW version's container while the old one
         exists -> Microsoft-Windows-AppModel-Runtime/Admin events 215/208 with
         error 0x80070020 (ERROR_SHARING_VIOLATION). The user sees a dialog that
         says the file is in use and Claude cannot start until the leftover
         processes are gone (usually only after a reboot).

    This script stops every process that carries Claude package identity (old and
    new version), waits for the containers to disappear, then relaunches Claude via
    its AUMID. It is meant to be run by a scheduled task triggered by event 208.

    Modes
      -FromEvent   Act only if a Claude launch failure (event 208, 0x80070020) was
                   logged within the last -RecentMinutes minutes. Used by the task.
      -Force       Manual repair: stop all Claude-identity processes and relaunch.
      -DryRun      Print what would be done; change nothing.
      -Status      (default) List Claude-identity processes and the registered package.

    Log file : %USERPROFILE%\ClaudeUpdateWatchdog\watchdog.log
    Exit code: 0 = ok or nothing to do, 1 = relaunch failed, 2 = rate-limited,
               4 = Claude package not registered for this user
#>
[CmdletBinding()]
param(
    [switch]$FromEvent,
    [switch]$Force,
    [switch]$DryRun,
    [switch]$Status,
    [int]$RecentMinutes = 5,
    [int]$MaxAttemptsPer15Min = 3
)

$ErrorActionPreference = 'Stop'

# Per-user data folder. Deliberately NOT under %LOCALAPPDATA%: when this script runs
# from a terminal inside the Claude desktop app, MSIX file-system virtualization
# silently redirects AppData writes into the package's LocalCache, where the scheduled
# task (which runs outside the container) cannot see them. The profile root is not
# virtualized.
$profileDir = [Environment]::GetFolderPath('UserProfile')
if (-not $profileDir) { $profileDir = $env:USERPROFILE }
if (-not $profileDir) { $profileDir = $PSScriptRoot }
$StateDir  = Join-Path $profileDir 'ClaudeUpdateWatchdog'
$LogFile   = Join-Path $StateDir 'watchdog.log'
$StateFile = Join-Path $StateDir 'state.json'
$SharingViolation = [int64]2147942432   # HRESULT 0x80070020

# Any unhandled terminating error ends up in the log instead of vanishing
# into Task Scheduler's 0xFFFD0000.
trap {
    $msg = '{0} [ERROR] Unhandled: {1} | {2}' -f (Get-Date -Format 'yyyy-MM-dd HH:mm:ss'), $_.Exception.Message, (($_.InvocationInfo.PositionMessage) -replace '\s+', ' ')
    try {
        if (-not (Test-Path $StateDir)) { New-Item -ItemType Directory -Path $StateDir -Force | Out-Null }
        Add-Content -Path $LogFile -Value $msg -Encoding UTF8
    } catch { }
    Write-Output $msg
    exit 1
}

function Write-Log {
    param([string]$Message, [string]$Level = 'INFO')
    $line = '{0} [{1}] {2}' -f (Get-Date -Format 'yyyy-MM-dd HH:mm:ss'), $Level, $Message
    Write-Host $line   # Write-Host so log lines never leak into a function's return value (e.g. Start-Claude)
    try {
        if (-not (Test-Path $StateDir)) { New-Item -ItemType Directory -Path $StateDir -Force | Out-Null }
        Add-Content -Path $LogFile -Value $line -Encoding UTF8
    } catch { }
}

# Package identity of running processes. Locale independent (unlike "tasklist /apps").
if (-not ('ClaudeWatchdog.Native' -as [type])) {
    Add-Type -Namespace ClaudeWatchdog -Name Native -MemberDefinition @'
[DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
public static extern int GetPackageFullName(IntPtr hProcess, ref uint packageFullNameLength, System.Text.StringBuilder packageFullName);
'@
}

function Get-ClaudeIdentityProcesses {
    foreach ($p in Get-Process) {
        try { $h = $p.Handle } catch { continue }
        if (-not $h) { continue }
        $len = [uint32]1024
        $sb  = New-Object System.Text.StringBuilder 1024
        $rc  = [ClaudeWatchdog.Native]::GetPackageFullName($h, [ref]$len, $sb)
        if ($rc -ne 0) { continue }
        $pkg = $sb.ToString()
        if ($pkg -notlike 'Claude_*') { continue }
        $start = $null; try { $start = $p.StartTime } catch { }
        [pscustomobject]@{ PID = $p.Id; Name = $p.ProcessName; Package = $pkg; StartTime = $start }
    }
}

function Get-RegisteredClaude {
    $pkg = Get-AppxPackage -Name 'Claude' -ErrorAction SilentlyContinue |
           Sort-Object { [version]$_.Version } -Descending | Select-Object -First 1
    if (-not $pkg) { return $null }
    [pscustomobject]@{
        FullName   = $pkg.PackageFullName
        FamilyName = $pkg.PackageFamilyName
        Aumid      = '{0}!Claude' -f $pkg.PackageFamilyName
    }
}

function Get-RecentLaunchFailure {
    param([string]$Aumid, [int]$Minutes)
    $ms = $Minutes * 60000
    # "&lt;=" because the XPath is embedded in XML.
    $xpath = "*[System[Provider[@Name='Microsoft-Windows-AppModel-Runtime'] and EventID=208 and TimeCreated[timediff(@SystemTime) &lt;= $ms]]] and *[EventData[Data[@Name='ApplicationName']='$Aumid']]"
    $xml = "<QueryList><Query Id='0' Path='Microsoft-Windows-AppModel-Runtime/Admin'><Select Path='Microsoft-Windows-AppModel-Runtime/Admin'>$xpath</Select></Query></QueryList>"
    $ev = Get-WinEvent -FilterXml $xml -MaxEvents 1 -ErrorAction SilentlyContinue
    if (-not $ev) { return $null }
    [pscustomobject]@{
        Time      = $ev.TimeCreated
        Package   = [string]$ev.Properties[0].Value
        ErrorCode = [int64]$ev.Properties[3].Value
    }
}

function Get-State {
    if (Test-Path $StateFile) {
        try { return (Get-Content $StateFile -Raw | ConvertFrom-Json) } catch { }
    }
    [pscustomobject]@{ attempts = @() }
}

function Save-Attempt {
    $st   = Get-State
    $list = @($st.attempts) + @((Get-Date).ToString('o'))
    if ($list.Count -gt 20) { $list = $list[($list.Count - 20)..($list.Count - 1)] }
    [pscustomobject]@{ attempts = $list } | ConvertTo-Json | Set-Content -Path $StateFile -Encoding UTF8
}

function Get-RecentAttemptCount {
    $st  = Get-State
    $cut = (Get-Date).AddMinutes(-15)
    $n = 0
    foreach ($a in @($st.attempts)) {
        try { if ([datetime]$a -gt $cut) { $n++ } } catch { }
    }
    $n
}

function Start-Claude {
    param([string]$Aumid)
    $uri = 'shell:AppsFolder\{0}' -f $Aumid
    try { Start-Process -FilePath $uri -ErrorAction Stop; return $true } catch { }
    try {
        Start-Process -FilePath (Join-Path $env:SystemRoot 'explorer.exe') -ArgumentList $uri -ErrorAction Stop
        return $true
    } catch {
        Write-Log ('Relaunch call failed: {0}' -f $_.Exception.Message) 'ERROR'
        return $false
    }
}

# ------------------------------------------------------------------ main ----

$reg = Get-RegisteredClaude
if (-not $reg) {
    Write-Log 'Claude MSIX package is not registered for this user; nothing to do.' 'WARN'
    exit 4
}
$aumid = $reg.Aumid

$mode = 'Status'
if ($Force) { $mode = 'Force' } elseif ($FromEvent) { $mode = 'FromEvent' }

$procs = @(Get-ClaudeIdentityProcesses | Sort-Object Package, StartTime)

if ($mode -eq 'Status') {
    Write-Output ('Registered package : {0}' -f $reg.FullName)
    Write-Output ('AUMID              : {0}' -f $aumid)
    Write-Output ('Processes with Claude package identity: {0}' -f $procs.Count)
    $procs |
        Select-Object PID, Name, Package, StartTime, @{ n = 'Orphan'; e = { $_.Package -ne $reg.FullName } } |
        Format-Table -AutoSize | Out-String -Width 200 | Write-Output
    exit 0
}

if ($mode -eq 'FromEvent') {
    $fail = Get-RecentLaunchFailure -Aumid $aumid -Minutes $RecentMinutes
    if (-not $fail) {
        Write-Log ('FromEvent: no Claude launch failure (event 208) in the last {0} min; nothing to do.' -f $RecentMinutes)
        exit 0
    }
    if ($fail.ErrorCode -ne $SharingViolation) {
        Write-Log ('FromEvent: latest launch failure at {0} has error 0x{1:X8}, not 0x80070020; nothing to do.' -f $fail.Time, $fail.ErrorCode) 'WARN'
        exit 0
    }
    Write-Log ('FromEvent: launch of {0} failed at {1} with 0x80070020 (sharing violation).' -f $fail.Package, $fail.Time)
} else {
    Write-Log 'Force: manual repair requested.'
}

if (-not $DryRun) {
    $recent = Get-RecentAttemptCount
    if ($recent -ge $MaxAttemptsPer15Min) {
        Write-Log ('Rate limit: {0} repair attempts in the last 15 min; giving up until the window passes.' -f $recent) 'WARN'
        exit 2
    }
}

Write-Log ('Registered package: {0}' -f $reg.FullName)
if ($procs.Count -eq 0) {
    Write-Log 'No process with Claude package identity is running.'
} else {
    foreach ($p in $procs) {
        $tag = 'current version'
        if ($p.Package -ne $reg.FullName) { $tag = 'ORPHAN - old version' }
        Write-Log ('  PID {0,-6} {1,-10} {2}  started {3:yyyy-MM-dd HH:mm:ss}  [{4}]' -f $p.PID, $p.Name, $p.Package, $p.StartTime, $tag)
    }
}

if ($DryRun) {
    Write-Log ('DRY RUN: would stop {0} process(es) and relaunch {1}. No changes made.' -f $procs.Count, $aumid)
    exit 0
}

Save-Attempt

foreach ($p in $procs) {
    try {
        Stop-Process -Id $p.PID -Force -ErrorAction Stop
        Write-Log ('Stopped PID {0} ({1})' -f $p.PID, $p.Package)
    } catch {
        Write-Log ('Could not stop PID {0}: {1}' -f $p.PID, $_.Exception.Message) 'WARN'
    }
}

# Wait for every Claude-identity process to disappear so the old container is torn down.
$deadline = (Get-Date).AddSeconds(20)
do {
    Start-Sleep -Milliseconds 500
    $left = @(Get-ClaudeIdentityProcesses)
} while (($left.Count -gt 0) -and ((Get-Date) -lt $deadline))

if ($left.Count -gt 0) {
    Write-Log ('{0} Claude-identity process(es) still alive after 20 s: {1}' -f $left.Count, (($left | ForEach-Object { $_.PID }) -join ', ')) 'WARN'
} else {
    Write-Log 'All Claude-identity processes are gone.'
}
Start-Sleep -Seconds 2

for ($attempt = 1; $attempt -le 2; $attempt++) {
    Write-Log ('Relaunching Claude via {0} (attempt {1})' -f $aumid, $attempt)
    if (Start-Claude -Aumid $aumid) {
        $deadline = (Get-Date).AddSeconds(25)
        do {
            Start-Sleep -Seconds 1
            $now = @(Get-ClaudeIdentityProcesses | Where-Object { $_.Package -eq $reg.FullName })
        } while (($now.Count -eq 0) -and ((Get-Date) -lt $deadline))
        if ($now.Count -gt 0) {
            Write-Log ('Claude {0} is running ({1} process(es)). Repair complete.' -f $reg.FullName, $now.Count)
            exit 0
        }
        Write-Log 'No Claude process appeared within 25 s.' 'WARN'
    }
    Start-Sleep -Seconds 5
}

Write-Log 'Relaunch failed twice. Sign out and back in, or reboot.' 'ERROR'
exit 1
