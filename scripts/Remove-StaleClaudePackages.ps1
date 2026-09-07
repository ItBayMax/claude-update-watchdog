<#
.SYNOPSIS
    Removes orphaned Claude Desktop MSIX package folders that older versions of the
    updater left under C:\Program Files\WindowsApps.

.DESCRIPTION
    Safety model
      * Only folders named Claude_<version>_x64__pzs8sxrjxfjjc (Anthropic's package
        family) are considered; nothing else under WindowsApps is looked at.
      * A folder is never touched if its package is registered or staged for ANY user
        (Get-AppxPackage -AllUsers), if it is the current version, or if a running
        process has an executable inside it.
      * Nothing is deleted unless -Delete is given; by default the script only reports.
      * Ownership/ACL changes are applied to the stale folders only, never to
        C:\Program Files\WindowsApps itself.
      * Before deleting, the folder's AppxManifest.xml (once readable) must name the
        package "Claude" by "Anthropic"; otherwise the folder is skipped.

    Candidate sources
      1. AppX deployment warnings 1230 ("hard links without a package in the
         repository") - the deployment engine's own list of orphans.
      2. A directory listing of WindowsApps, when the account is allowed to list it.
      3. Folders passed explicitly with -Folder (name or full path).
      4. With -IncludeDeleted: folders that deployment events 472 moved to
         WindowsApps\Deleted and that still exist (Windows normally removes these on
         its own at the next boot).

    Modes
      (default)   Report candidates and what -Delete would remove.
      -Delete     Take ownership (Administrators), grant full control, remove.
                  Requires an elevated PowerShell.
      -SelfTest   Exercise the removal routine on a throw-away temp folder.

    Log: %USERPROFILE%\ClaudeUpdateWatchdog\stale-package-cleanup.log
#>
[CmdletBinding()]
param(
    [switch]$Delete,
    [switch]$IncludeDeleted,
    [string[]]$Folder,
    [switch]$SelfTest
)

$ErrorActionPreference = 'Stop'

$WindowsApps    = Join-Path $env:ProgramFiles 'WindowsApps'
$DeletedRoot    = Join-Path $WindowsApps 'Deleted'
$Family         = 'pzs8sxrjxfjjc'
$NamePattern    = '^Claude_[0-9]+(\.[0-9]+){3}_x64__' + $Family + '$'
$DeletedPattern = '^Claude_[0-9]+(\.[0-9]+){3}_x64__' + $Family + '[0-9a-f-]{36}$'
$AdminSid       = '*S-1-5-32-544'

$profileDir = [Environment]::GetFolderPath('UserProfile')
if (-not $profileDir) { $profileDir = $env:USERPROFILE }
$LogDir  = Join-Path $profileDir 'ClaudeUpdateWatchdog'
$LogFile = Join-Path $LogDir 'stale-package-cleanup.log'

function Write-Log {
    param([string]$Message, [string]$Level = 'INFO')
    $line = '{0} [{1}] {2}' -f (Get-Date -Format 'yyyy-MM-dd HH:mm:ss'), $Level, $Message
    Write-Host $line   # Write-Host so log lines never leak into a function's return value
    try {
        if (-not (Test-Path $LogDir)) { New-Item -ItemType Directory -Path $LogDir -Force | Out-Null }
        Add-Content -Path $LogFile -Value $line -Encoding UTF8
    } catch { }
}

$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)

function Get-FolderState {
    param([string]$Path)
    try {
        $null = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
        return 'Exists'
    } catch [System.Management.Automation.ItemNotFoundException] {
        return 'Missing'
    } catch [System.UnauthorizedAccessException] {
        return 'Denied'
    } catch {
        if ($_.Exception -is [System.IO.DirectoryNotFoundException]) { return 'Missing' }
        if ($_.Exception.Message -match 'denied') { return 'Denied' }
        return 'Missing'
    }
}

function Get-FolderSize {
    param([string]$Path)
    try {
        $sum = (Get-ChildItem -LiteralPath $Path -Recurse -Force -File -ErrorAction Stop | Measure-Object -Property Length -Sum).Sum
        if ($null -eq $sum) { return 0 }
        return [int64]$sum
    } catch { return $null }
}

function Format-Size {
    param($Bytes)
    if ($null -eq $Bytes) { return 'n/a' }
    if ($Bytes -ge 1GB) { return ('{0:N2} GB' -f ($Bytes / 1GB)) }
    if ($Bytes -ge 1MB) { return ('{0:N1} MB' -f ($Bytes / 1MB)) }
    return ('{0} B' -f $Bytes)
}

function Get-ProtectedFullNames {
    $names = New-Object 'System.Collections.Generic.HashSet[string]' ([StringComparer]::OrdinalIgnoreCase)
    foreach ($p in @(Get-AppxPackage -Name 'Claude' -ErrorAction SilentlyContinue)) { [void]$names.Add($p.PackageFullName) }
    if ($isAdmin) {
        foreach ($p in @(Get-AppxPackage -AllUsers -Name 'Claude' -ErrorAction SilentlyContinue)) { [void]$names.Add($p.PackageFullName) }
    } else {
        Write-Log 'Not elevated: other users'' registrations cannot be checked (Get-AppxPackage -AllUsers). Run elevated before using -Delete.' 'WARN'
    }
    return ,$names   # unary comma keeps the HashSet intact instead of unrolling it
}

function Get-CandidatesFromEvents {
    $set = New-Object 'System.Collections.Generic.HashSet[string]' ([StringComparer]::OrdinalIgnoreCase)
    $evs = Get-WinEvent -FilterHashtable @{ LogName = 'Microsoft-Windows-AppXDeploymentServer/Operational'; Id = 1230 } -MaxEvents 200 -ErrorAction SilentlyContinue
    foreach ($e in @($evs)) {
        foreach ($m in [regex]::Matches($e.Message, '\\WindowsApps\\(Claude_[^\\;]+?)\\')) { [void]$set.Add($m.Groups[1].Value) }
    }
    return $set
}

function Get-DeletedCandidatesFromEvents {
    $set = New-Object 'System.Collections.Generic.HashSet[string]' ([StringComparer]::OrdinalIgnoreCase)
    $evs = Get-WinEvent -FilterHashtable @{ LogName = 'Microsoft-Windows-AppXDeploymentServer/Operational'; Id = 472 } -MaxEvents 500 -ErrorAction SilentlyContinue
    foreach ($e in @($evs)) {
        foreach ($m in [regex]::Matches($e.Message, 'WindowsApps\\Deleted\\(Claude_[^\\\s\u3002]+)')) { [void]$set.Add($m.Groups[1].Value) }
    }
    return $set
}

function Test-ManifestIsClaude {
    # $true  = manifest says Name="Claude" by Anthropic
    # $false = manifest present but for something else (never delete)
    # $null  = manifest missing/unreadable
    param([string]$Path)
    $manifest = Join-Path $Path 'AppxManifest.xml'
    try {
        if (-not (Test-Path -LiteralPath $manifest)) { return $null }
        $xml = [xml](Get-Content -LiteralPath $manifest -Raw -ErrorAction Stop)
        $id = $xml.Package.Identity
        return (($id.Name -eq 'Claude') -and ($id.Publisher -like '*Anthropic*'))
    } catch { return $null }
}

function Remove-StaleFolder {
    param([string]$Path, [string]$AllowedRoot)
    if (-not $Path.StartsWith($AllowedRoot, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to remove '$Path': outside '$AllowedRoot'."
    }
    # takeown/icacls write warnings to stderr; inside this function they must not turn
    # into terminating errors (function-scoped preference).
    $ErrorActionPreference = 'Continue'
    $sys = Join-Path $env:SystemRoot 'System32'
    # 1. take ownership (Administrators) - takeown first, icacls /setowner as fallback
    & (Join-Path $sys 'takeown.exe') /F $Path /R /A /D Y 2>$null | Out-Null
    if ($LASTEXITCODE -ne 0) {
        & (Join-Path $sys 'icacls.exe') $Path /setowner $AdminSid /T /C /Q 2>$null | Out-Null
        if ($LASTEXITCODE -ne 0) { Write-Log ('  ownership change reported errors on {0} (continuing)' -f $Path) 'WARN' }
    }
    # 2. grant Administrators full control
    & (Join-Path $sys 'icacls.exe') $Path /grant ("{0}:(OI)(CI)F" -f $AdminSid) /T /C /Q 2>$null | Out-Null
    if ($LASTEXITCODE -ne 0) { Write-Log ('  ACL grant reported errors on {0} (continuing)' -f $Path) 'WARN' }
    # 3. manifest sanity check now that the folder is readable
    $isClaude = Test-ManifestIsClaude -Path $Path
    if ($isClaude -eq $false) {
        Write-Log ('  {0}: AppxManifest.xml is not Claude/Anthropic - skipped.' -f $Path) 'ERROR'
        return $false
    }
    # 4. delete; fall back to rd with the long-path prefix
    Remove-Item -LiteralPath $Path -Recurse -Force -ErrorAction SilentlyContinue
    if (Test-Path -LiteralPath $Path) {
        & (Join-Path $sys 'cmd.exe') /d /c rd /s /q "\\?\$Path" 2>$null | Out-Null
    }
    return (-not (Test-Path -LiteralPath $Path))
}

# ----------------------------------------------------------------- self test --

if ($SelfTest) {
    $root = Join-Path $env:TEMP ('stale-selftest-' + [guid]::NewGuid().ToString('N'))
    $fake = Join-Path $root ('Claude_0.0.0.1_x64__' + $Family)
    # Windows PowerShell 5.1 cannot create paths beyond 260 chars, so keep the test tree
    # moderately deep; the rd "\\?\" fallback for genuinely long paths is not exercisable here.
    $deep = Join-Path $fake ('app\' + ('sub\' * 8) + 'leaf')
    New-Item -ItemType Directory -Path $deep -Force | Out-Null
    if (-not (Test-Path -LiteralPath $deep)) { Write-Log 'SelfTest: could not create the test tree.' 'ERROR'; exit 1 }
    Set-Content -LiteralPath (Join-Path $fake 'AppxManifest.xml') -Value '<Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"><Identity Name="Claude" Publisher="CN=Anthropic, PBC" Version="0.0.0.1" ProcessorArchitecture="x64" /></Package>'
    Set-Content -LiteralPath (Join-Path $deep 'file.txt') -Value 'x'
    Set-ItemProperty -LiteralPath (Join-Path $deep 'file.txt') -Name IsReadOnly -Value $true
    Write-Log ('SelfTest: created {0} (path length {1})' -f $fake, (Join-Path $deep 'file.txt').Length)
    $ok = Remove-StaleFolder -Path $fake -AllowedRoot $root
    Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
    if ($ok) { Write-Log 'SelfTest: removal routine OK (ownership steps may warn when not elevated).'; exit 0 }
    Write-Log 'SelfTest: removal routine FAILED.' 'ERROR'; exit 1
}

# ---------------------------------------------------------------------- main --

if ($Delete -and -not $isAdmin) {
    Write-Log '-Delete requires an elevated (Administrator) PowerShell. Re-run from an elevated prompt.' 'ERROR'
    exit 2
}

Write-Log ('Mode: {0}; elevated: {1}' -f $(if ($Delete) { 'DELETE' } else { 'report only' }), $isAdmin)

$protected = Get-ProtectedFullNames
Write-Log ('Registered/staged Claude packages (never touched): {0}' -f (($protected | Sort-Object) -join ', '))

# collect candidates: name -> full path
$cands = @{}
foreach ($n in Get-CandidatesFromEvents) { if ($n -match $NamePattern) { $cands[$n] = Join-Path $WindowsApps $n } }
$fromEvents = $cands.Count

$listed = $false
try {
    foreach ($d in Get-ChildItem -LiteralPath $WindowsApps -Directory -Force -ErrorAction Stop) {
        if ($d.Name -match $NamePattern) { $cands[$d.Name] = $d.FullName }
    }
    $listed = $true
} catch { }

foreach ($f in @($Folder)) {
    if (-not $f) { continue }
    $leaf = Split-Path -Leaf $f
    if ($leaf -notmatch $NamePattern) { Write-Log ('Ignoring -Folder {0}: not a Claude package folder name.' -f $f) 'WARN'; continue }
    $cands[$leaf] = Join-Path $WindowsApps $leaf
}

if ($IncludeDeleted) {
    foreach ($n in Get-DeletedCandidatesFromEvents) { if ($n -match $DeletedPattern) { $cands["Deleted\$n"] = Join-Path $DeletedRoot $n } }
}

Write-Log ('Candidates: {0} ({1} from deployment warnings 1230; WindowsApps listing {2})' -f $cands.Count, $fromEvents, $(if ($listed) { 'succeeded' } else { 'not permitted - relying on event log and -Folder' }))

# running processes inside WindowsApps\Claude_*
$running = @{}
foreach ($p in Get-Process) {
    try { $path = $p.Path } catch { continue }
    if ($path -and $path.StartsWith($WindowsApps, [StringComparison]::OrdinalIgnoreCase)) {
        $rel = $path.Substring($WindowsApps.Length).TrimStart('\')
        $top = $rel.Split('\')[0]
        if ($top -eq 'Deleted') { $top = 'Deleted\' + $rel.Split('\')[1] }
        $running[$top] = $true
    }
}

$plan = @()
foreach ($name in ($cands.Keys | Sort-Object)) {
    $path  = $cands[$name]
    $state = Get-FolderState -Path $path
    $bare  = $name -replace '^Deleted\\', ''
    $reason = ''
    if ($state -eq 'Missing')                       { $reason = 'already gone' }
    elseif ($protected.Contains($bare))             { $reason = 'PROTECTED: registered/staged package' }
    elseif ($running.ContainsKey($name))            { $reason = 'PROTECTED: process running from it' }
    $size = $null
    if ($state -eq 'Exists') { $size = Get-FolderSize -Path $path }
    $plan += [pscustomobject]@{ Folder = $name; State = $state; Size = (Format-Size $size); Bytes = $size; Action = $(if ($reason) { 'skip: ' + $reason } else { 'remove' }) }
}

$plan | Select-Object Folder, State, Size, Action | Format-Table -AutoSize | Out-String -Width 220 | Write-Output
$toRemove = @($plan | Where-Object { $_.Action -eq 'remove' })
$known = @($toRemove | Where-Object { $null -ne $_.Bytes } | Measure-Object -Property Bytes -Sum).Sum
Write-Log ('{0} folder(s) would be removed, apparent size {1} (hard-linked files counted in full; sizes marked n/a were not readable before taking ownership).' -f $toRemove.Count, (Format-Size $known))

if (-not $Delete) {
    Write-Log 'Report only. Re-run from an elevated PowerShell with -Delete to remove the folders listed as "remove".'
    exit 0
}

$removed = 0; $failed = 0; $freed = [int64]0
foreach ($item in $toRemove) {
    $path = $cands[$item.Folder]
    $root = $WindowsApps
    Write-Log ('Removing {0} ...' -f $path)
    $size = Get-FolderSize -Path $path
    $ok = Remove-StaleFolder -Path $path -AllowedRoot $root
    if ($ok) {
        $removed++
        if ($null -ne $size) { $freed += $size }
        Write-Log ('  removed ({0})' -f (Format-Size $size))
    } else {
        $failed++
        Write-Log ('  FAILED - folder still present' -f $path) 'ERROR'
    }
}
Write-Log ('Done: removed {0}, failed {1}, apparent size freed {2}.' -f $removed, $failed, (Format-Size $freed))
if ($failed -gt 0) { exit 1 }
exit 0
