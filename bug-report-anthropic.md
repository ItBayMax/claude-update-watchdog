# Claude Desktop for Windows (MSIX): silent self-update leaves the app unlaunchable until reboot

**Summary.** After Claude Desktop's idle-time "stealth update" quits and relaunches itself, the relaunch of the new package version fails with `0x80070020` (ERROR_SHARING_VIOLATION). Windows cannot create the new version's Desktop AppX container because a process of the *old* version is still alive. Users see a dialog titled with the new `Claude.exe` path saying the file is in use, and Claude will not start again until every leftover `claude.exe` is terminated. Most users end up rebooting. On the machine below this happened on 4 of the last 6 stealth updates; the two that succeeded (09-03 10:37 and 09-05 11:55) show the shutdown/relaunch race is intermittent rather than deterministic.

## Environment

| Item | Value |
|---|---|
| OS | Windows 11 Pro for Workstations 10.0.26200 (VMware guest, used over RDP) |
| Package family | `Claude_pzs8sxrjxfjjc` (MSIX, `SignatureKind=Developer`, installed from claude.ai) |
| Versions involved | 1.44121.2 → 1.44121.4 → 1.46388.1 → 1.46388.2 → 1.46388.3 |
| Update cadence seen | 10 package versions between 2026-08-26 and 2026-09-05 |
| Workload | Claude Code sessions inside the desktop app, browser preview, dev servers started via "Launch" |

## What the logs show

**Claude `main.log`** (`%LOCALAPPDATA%\Claude\logs\main.log`), identical on every incident:

```
[updater] Found an update, downloading
[updater] Update downloaded and ready to install { releaseName: 'Claude 1.46388.3' }
[updater] Staged version 1.46388.3 is still current (latest: 1.46388.3, lastTarget: null)
[stealth-update] Triggering stealth update after idle timeout
[stealth-relaunch] Saved z-order anchor ... / Saved navigation history ...
[CCD] Killing 3 PTY process tree(s) on quit
beforeQuitForUpdate handler fired, going down for update
Windows session ending (close-app) - quitting the app
<no further lines from the old instance; next line is "Starting app" hours later>
```

**Microsoft-Windows-AppXDeploymentServer/Operational** (the deployment itself succeeds):

- 658 (warning): new package marked for *deferred registration* because the old version is still running.
- 603: `RegisterByPackageFamilyName` with `ForceApplicationShutdownOption` at the moment of the stealth quit.
- 9648/9650: packaged service `CoworkVMService` terminated for the update.
- 400: Register completed; 472: old package folder moved to `WindowsApps\Deleted\...`. Result 0x0.

**Microsoft-Windows-AppModel-Runtime/Admin** (the relaunch fails):

- 210/211: Desktop AppX container created for the *new* package, one process added.
- 215 ×2 (error): `0x80070020: cannot create Desktop AppX container for <new package> — error during the conversion job`.
- 208 (error): `0x80070020: cannot create process for <new package> ... [LaunchProcess]`.
- 217 for the *old* package's container arrives only much later — 15 minutes, 2–3 hours, or at the next reboot — i.e. a process of the old version outlived the quit the whole time. The new container disappears at the same moment, so the half-started new `Claude.exe` also sits there (no window) until then.

## Timeline of the four incidents (local time, 2026)

| Update to | Stealth quit (main.log) | New-version launch fails (208, 0x80070020) | Old-version container destroyed (217) | Claude usable again |
|---|---|---|---|---|
| 1.44121.4 | 09-03 14:57:52 | 14:58:28, 15:25:37 (user retry) | 17:51:27 | 17:52:38 |
| 1.46388.1 | 09-04 13:30:46 | 13:30:50, 15:27:22 (user retry) | 15:31:17 | 15:32:09 |
| 1.46388.2 | 09-04 18:00:58 | 18:01:00, 18:10:55 (user retry) | 18:15:44 | 18:16:36 |
| 1.46388.3 | 09-05 01:31:41 | 01:31:43 | 08:32:15 (**reboot**) | 08:43 |

The stealth updates of 09-03 10:37 (1.40609.1 → 1.44121.2) and 09-05 11:55 (1.46388.3 → 1.46388.4, window minimized, user idle) succeeded: both old containers were destroyed at 11:55:50 and the new one created at 11:55:51 with no 215/208. The failure therefore looks like a race between the old process tree's shutdown and the relaunch, not a deterministic defect.

Additional observations:

- 30 × `0x80070020` events (208/215) for the Claude package in three days.
- Event 1230 lists orphan hard links from 15 old package folders (1.13576.4 … 1.25927.0) that were never cleaned up, which suggests in-use files have blocked cleanup for months.
- Processes that the app spawns without package identity (the Claude Code CLI `claude.exe` from `%APPDATA%\Claude\claude-code\...`, shells) are *not* members of the container, so the survivor must be one of the app's own `claude.exe` processes (main or a `--type=...` helper such as crashpad-handler, GPU or a utility NodeService).
- On Windows the relaunch error surfaces as the generic shell dialog "The file is being used by another program" with the new exe path in the title, which gives users no hint that a hidden old process is the cause.

## Suggested fixes

1. Before relaunching the new version, wait until no process with the old `PackageFullName` remains (e.g. `GetPackageFullName` over the process list, or wait for AppModel-Runtime event 217), with a timeout, then force-terminate the leftovers — they are the app's own processes.
2. If activation of the new version fails with `0x80070020`, terminate the half-started process, clean up, and retry instead of leaving it hanging; show a notification if the retry also fails.
3. Log the surviving process tree at `beforeQuitForUpdate` so the specific helper that outlives the quit can be identified.
4. Reconsider the silent quit-and-relaunch while Claude Code sessions, preview servers or browser sessions are active, or at least make it visible and deferrable.

## How to reproduce

Windows 11, Claude Desktop MSIX, a few Claude Code sessions and a preview server running inside the app. Let an update download, then leave the machine idle until `[stealth-update] Triggering stealth update after idle timeout` appears in `main.log`. Check `Microsoft-Windows-AppModel-Runtime/Admin` for events 215/208 with `ErrorCode 2147942432` and note that a `claude.exe` of the previous version is still alive.

## Commands used to collect the evidence

```powershell
Get-WinEvent -LogName 'Microsoft-Windows-AppModel-Runtime/Admin' -MaxEvents 3000 | Where-Object { $_.Message -like '*Claude*' } | Sort-Object TimeCreated | Format-Table TimeCreated, Id, LevelDisplayName, Message -Wrap
Get-WinEvent -LogName 'Microsoft-Windows-AppXDeploymentServer/Operational' -MaxEvents 6000 | Where-Object { $_.Message -like '*Claude*' } | Sort-Object TimeCreated | Format-Table TimeCreated, Id, LevelDisplayName, Message -Wrap
Select-String -Path "$env:LOCALAPPDATA\Claude\logs\main.log" -Pattern 'stealth-update|beforeQuitForUpdate|\[updater\]'
tasklist /apps /fo csv | ConvertFrom-Csv | Where-Object { $_.'Package Name' -like 'Claude_*' }
```
