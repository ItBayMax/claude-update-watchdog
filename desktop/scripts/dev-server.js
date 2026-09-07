#!/usr/bin/env node
/**
 * Dev orchestrator for `npm run tauri:dev`.
 *
 *   1. Pick a free TCP port in [50001, 59999].
 *   2. Write a Tauri config override (under src-tauri/target/, which is
 *      gitignored) that pins `build.devUrl` to that port — this is what
 *      Tauri uses to know where to load the webview from.
 *   3. Export VITE_DEV_PORT so vite.config.ts can read it.
 *   4. Spawn `tauri dev --config <override>` and forward stdio + signals.
 *
 * The override file is regenerated every run, so port choice is fresh on
 * each `npm run tauri:dev`.
 */

import { spawn } from "node:child_process";
import net from "node:net";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { mkdirSync, writeFileSync } from "node:fs";

const require = createRequire(import.meta.url);

const __dirname = dirname(fileURLToPath(import.meta.url));
const rootDir = join(__dirname, "..");

const MIN_PORT = 50_001;
const MAX_PORT = 59_999;

function isPortAvailable(port) {
  return new Promise((resolve) => {
    const server = net.createServer();
    server.once("error", () => resolve(false));
    server.once("listening", () => {
      server.close(() => resolve(true));
    });
    server.listen(port, "127.0.0.1");
  });
}

async function pickPort() {
  const range = MAX_PORT - MIN_PORT + 1;
  // Random starting offset so concurrent shells don't always collide.
  const start = MIN_PORT + Math.floor(Math.random() * range);
  for (let i = 0; i < range; i++) {
    const port = MIN_PORT + ((start - MIN_PORT + i) % range);
    if (await isPortAvailable(port)) return port;
  }
  throw new Error(`no available port in [${MIN_PORT}, ${MAX_PORT}]`);
}

function writeTauriOverride(port) {
  // Put the ephemeral config under src-tauri/target/ — already gitignored,
  // doesn't dirty the working tree.
  const dir = join(rootDir, "src-tauri", "target");
  mkdirSync(dir, { recursive: true });
  const file = join(dir, "dev-server.conf.json");
  const payload = { build: { devUrl: `http://localhost:${port}` } };
  writeFileSync(file, JSON.stringify(payload, null, 2));
  return file;
}

async function main() {
  const port = await pickPort();
  const devUrl = `http://localhost:${port}`;
  const overridePath = writeTauriOverride(port);

  console.log(`[dev-server] port=${port}  url=${devUrl}`);
  console.log(`[dev-server] tauri config override: ${overridePath}`);

  const env = {
    ...process.env,
    VITE_DEV_PORT: String(port),
    TAURI_DEV_URL: devUrl,
  };

  // Invoke the Tauri CLI's JS entry directly with node, bypassing the
  // platform shim (`npx.cmd`). Node 20.12+ on Windows refuses to spawn
  // .cmd files with `shell:false` (CVE-2024-27980 mitigation), which is
  // what caused the spawn EINVAL we used to see here.
  const tauriCli = require.resolve("@tauri-apps/cli/tauri.js");
  const child = spawn(
    process.execPath,
    [tauriCli, "dev", "--config", overridePath],
    {
      stdio: "inherit",
      cwd: rootDir,
      env,
      shell: false,
    }
  );

  const forward = (sig) => () => {
    if (!child.killed) child.kill(sig);
  };
  process.on("SIGINT", forward("SIGINT"));
  process.on("SIGTERM", forward("SIGTERM"));
  child.on("exit", (code, signal) => {
    if (signal) process.kill(process.pid, signal);
    else process.exit(code ?? 0);
  });
}

main().catch((err) => {
  console.error("[dev-server]", err);
  process.exit(1);
});
