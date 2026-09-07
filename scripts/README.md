# Claude 桌面版（Windows / MSIX）更新后"文件被占用"的自愈方案（脚本版）

这是第一版，纯 PowerShell，零依赖。可视化的桌面版在同仓库的 `../desktop/`，两者可并存，桌面版安装的计划任务会覆盖这里的同名任务。

## 问题是什么

Claude 桌面版会在后台下载新版本，等用户空闲一段时间后自动退出并重启自己（日志里叫 `stealth update`）。
退出时旧版本至少有一个 `claude.exe` 进程没有结束，旧版本的应用容器因此还活着。
Windows 不允许在旧版本容器存活时创建新版本容器，于是新版本启动失败，错误码 `0x80070020`，
弹窗标题是新版本的 `Claude.exe` 路径，内容是"另一程序正在使用此文件"。
残留进程没有窗口，用户看不到，直到它被结束（任务管理器或重启）Claude 才能再打开。

根因在 Claude 桌面版的更新流程里，只有 Anthropic 能改。下面是三种可以自己做的事。

## 方案 A：自愈看门狗（保留自动更新，推荐先做）

原理：任务计划程序订阅事件日志 `Microsoft-Windows-AppModel-Runtime/Admin` 的事件 208
（`ApplicationName = Claude_pzs8sxrjxfjjc!Claude`）。事件出现 3 秒后运行脚本：

1. 确认最近 5 分钟内确实有 Claude 启动失败且错误码为 `0x80070020`，否则什么都不做。
2. 记录所有带 Claude 包身份的进程（含旧版本残留和卡住的新版本），写入日志。
3. 结束这些进程，等容器销毁，再通过 AUMID 重新启动 Claude。
4. 15 分钟内最多尝试 3 次，避免死循环。

只会结束 Claude 桌面版自己的进程。Claude Code 命令行、MCP 服务器、开发服务器没有包身份，不受影响。

### 文件

| 文件 | 作用 |
|---|---|
| `Claude-UpdateWatchdog.ps1` | 看门狗脚本，支持 `-Status` `-DryRun` `-Force` `-FromEvent` |
| `Install-ClaudeUpdateWatchdog.ps1` | 安装计划任务（默认当前用户，无需管理员） |
| `Uninstall-ClaudeUpdateWatchdog.ps1` | 卸载 |
| `bug-report-anthropic.md` | 提交给 Anthropic 的英文 bug 报告，含完整证据 |

### 安装

当前用户，无需管理员权限：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\Install-ClaudeUpdateWatchdog.ps1
```

全员安装需要管理员 PowerShell，任务以 `BUILTIN\Users` 身份注册，脚本放在 `%ProgramData%\ClaudeUpdateWatchdog`：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\Install-ClaudeUpdateWatchdog.ps1 -AllUsers
```

`-AllUsers` 这条路径没有在本机实测过，请先在一台测试机验证。

### 日常使用

```powershell
# 看当前状态：注册的版本、带包身份的进程、是否有残留
.\Claude-UpdateWatchdog.ps1 -Status

# 预演一次修复，不做任何改动
.\Claude-UpdateWatchdog.ps1 -Force -DryRun

# 手动修复：结束所有 Claude 进程并重启 Claude
.\Claude-UpdateWatchdog.ps1 -Force
```

`-Force` 要在普通的 PowerShell 窗口里运行，不要在 Claude 桌面版内置的终端里跑，因为它会把 Claude 连同那个终端一起结束。

脚本和日志都在 `%USERPROFILE%\ClaudeUpdateWatchdog`，日志文件是 `watchdog.log`。每次触发都会记下残留进程的 PID、版本和启动时间，这也是给 Anthropic 的证据。安装脚本结束时会自动把任务跑一次做自检，正常应显示 `LastTaskResult = 0x00000000 (OK)`。

故意不放在 AppData 下：如果在 Claude 桌面版内置的终端里运行安装脚本，MSIX 文件系统虚拟化会把 AppData 下的写入悄悄重定向到 `%LOCALAPPDATA%\Packages\Claude_pzs8sxrjxfjjc\LocalCache`，而计划任务在容器外运行，看不到这些文件，任务会以 0xFFFD0000 失败。第一版安装脚本就踩了这个坑。

### 卸载

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\Uninstall-ClaudeUpdateWatchdog.ps1
```

### 局限

- 只处理 `0x80070020` 这一类失败，其它启动失败不会动。
- 触发时 Claude 已经不可用，结束进程不会造成额外损失，但当时未保存的界面状态以 App 自己的恢复机制为准。
- 这是绕过症状，不是修根因。

## 方案 B：用官方策略关闭自动更新，由 IT 控制更新节奏

Claude 桌面版支持企业托管配置，Windows 上从注册表读取，路径 `SOFTWARE\Policies\Claude`，
HKLM 和 HKCU 都会读，官方以 HKLM（GPO）为准。相关键：

| 键 | 含义 |
|---|---|
| `disableAutoUpdates` | 为真时更新器完全不启动，不下载也不静默重启 |
| `autoUpdaterEnforcementHours` | 已下载的更新多少小时后强制安装，默认 72 |

官方文档：<https://support.claude.com/en/articles/12622667-enterprise-configuration>，值的类型以文档为准，一般是 `REG_DWORD 1`。

注意：策略在下次启动 App 时生效。如果策略下发时 App 正在运行且已经下载好一个更新，那一个仍会安装一次。
关闭后需要 IT 自己分发新版 MSIX，安装前让用户先彻底退出 Claude。

## 方案 C：向 Anthropic 反馈

`bug-report-anthropic.md` 是整理好的英文报告，附带事件 ID、时间线和错误码。
提交渠道：桌面版 Help 菜单里的 Report a bug，或 <https://support.claude.com>。

## 清理历史残留版本

1.300xx 之前的更新器不会删旧版本目录，`C:\Program Files\WindowsApps` 下会堆着一串 `Claude_1.xxxxx.x.0_x64__pzs8sxrjxfjjc`。
部署引擎自己在警告 1230 里把它们列为"仓库里没有对应程序包的硬链接"，也就是孤儿目录，`Remove-AppxPackage` 删不掉，只能接管权限后删。

`Remove-StaleClaudePackages.ps1` 做这件事，安全规则：

- 只看名字形如 `Claude_<版本>_x64__pzs8sxrjxfjjc` 的目录，别的一律不碰。
- 任何用户名下已注册或已暂存的版本、当前版本、有进程正在运行的版本，一律受保护。
- 删之前读目录里的 `AppxManifest.xml`，必须是 Anthropic 的 Claude 才动手。
- 只改这些目录自身的所有者和权限，不碰 `WindowsApps` 根目录。
- 不加 `-Delete` 只出报告。

用户数据不在这里：登录令牌、会话、MCP 配置在 `%APPDATA%\Claude` 和 `%LOCALAPPDATA%\Packages\Claude_pzs8sxrjxfjjc`，Claude Code 的会话在 `%USERPROFILE%\.claude`。删旧版本目录不会导致重新登录。

先出报告（不需要管理员）：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\Remove-StaleClaudePackages.ps1
```

确认清单后，在管理员 PowerShell 里删除：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\Remove-StaleClaudePackages.ps1 -Delete
```

日志在 `%USERPROFILE%\ClaudeUpdateWatchdog\stale-package-cleanup.log`。报告里的体积把跨版本硬链接的文件重复计算了，实际释放会略少。

## 不装任何东西时的手动处理

出现弹窗时打开任务管理器的"详细信息"页，结束所有 `claude.exe`，再从开始菜单打开 Claude。
或者用 PowerShell 结束所有带 Claude 包身份的进程：

```powershell
tasklist /apps /fo csv | ConvertFrom-Csv | Where-Object { $_.'Package Name' -like 'Claude_*' } | ForEach-Object { Stop-Process -Id $_.PID -Force }
```

中文系统上 `tasklist` 的列名是中文，这条命令需要把 `'Package Name'` 换成 `'程序包名称'`。看门狗脚本不依赖列名，没有这个问题。
