# 截图

仓库 `README.md` 与 `desktop/README.md` 引用的图片，均用相对路径 `docs/images/<文件名>` 引用，推到 GitHub 后直接显示。

| 文件名 | 内容 | 来源 |
|---|---|---|
| `error-dialog-1.46388.2.png` | 更新到 1.46388.2 后的弹窗：「另一程序正在使用此文件。」 | 用户截图，2026-09-04 |
| `error-dialog-1.46388.1.png` | 更新到 1.46388.1 后的同样弹窗 | 用户截图，2026-09-04 |
| `stale-versions-explorer.png` | 资源管理器搜索结果：WindowsApps 下 15 个历史版本的 claude.exe 加当前版本 | 用户截图，2026-09-05 |
| `desktop-dashboard.png` | 桌面版 0.1.4 仪表盘 | 前端演示模式渲染，见下 |
| `desktop-processes.png` | 进程页 | 同上 |
| `desktop-events.png` | 事件时间线页 | 同上 |
| `desktop-stale.png` | 残留版本页 | 同上 |

桌面版界面截图的生成方式：前端在非 Tauri 环境（普通浏览器）运行时自动切换到演示数据（`desktop/src/mock.ts`，
数值取自真实机器上的一次运行，不含个人信息），用 `npm run dev` 起 Vite，再用 Edge 无头模式截图：

```powershell
cd desktop; npm run dev
& "C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe" --headless=new --disable-gpu --hide-scrollbars --force-device-scale-factor=1 --window-size=1180,880 --virtual-time-budget=8000 --user-data-dir="$env:TEMP\edge-shot" --screenshot="docs\images\desktop-dashboard.png" "http://127.0.0.1:1420/?screenshot=1&tab=dashboard"
```

`?screenshot=1` 隐藏"演示模式"横幅，`?tab=processes|events|stale|task|settings|logs` 预选页签。
