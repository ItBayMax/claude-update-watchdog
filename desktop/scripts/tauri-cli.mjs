#!/usr/bin/env node
/**
 * Thin wrapper around the `tauri` CLI.
 *
 * `npm run tauri dev` (or `npm run tauri:dev`) is routed through
 * scripts/dev-server.js so it gets a random port in [50001, 59999].
 * Every other subcommand (build / icon / signer / migrate / info / …)
 * passes through to @tauri-apps/cli unchanged.
 */

import { spawn } from "node:child_process";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const require = createRequire(import.meta.url);
const __dirname = dirname(fileURLToPath(import.meta.url));
const rootDir = join(__dirname, "..");
const args = process.argv.slice(2);

let cmd;
let cmdArgs;

if (args[0] === "dev") {
  // Route through our random-port orchestrator. Extra args after `dev` are
  // forwarded so e.g. `npm run tauri dev -- --verbose` still works.
  cmd = process.execPath;
  cmdArgs = [join(__dirname, "dev-server.js"), ...args.slice(1)];
} else {
  // Pass through to @tauri-apps/cli. Resolve and invoke the JS entry
  // directly with node to avoid Node's `.cmd`-with-`shell:false` block
  // (CVE-2024-27980 mitigation) when calling `npx.cmd` on Windows.
  cmd = process.execPath;
  cmdArgs = [require.resolve("@tauri-apps/cli/tauri.js"), ...args];
}

const child = spawn(cmd, cmdArgs, {
  stdio: "inherit",
  cwd: rootDir,
  shell: false,
});

const forward = (sig) => () => {
  if (!child.killed) child.kill(sig);
};
process.on("SIGINT", forward("SIGINT"));
process.on("SIGTERM", forward("SIGTERM"));

child.on("exit", (code, signal) => {
  if (signal) process.kill(process.pid, signal);
  else process.exit(code ?? 0);
});
