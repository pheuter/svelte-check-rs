#!/usr/bin/env node
"use strict";

const fs = require("node:fs");
const path = require("node:path");
const { spawn } = require("node:child_process");

const PLATFORM_MAP = {
  "darwin-x64": "@svelte-check-rs/darwin-x64",
  "darwin-arm64": "@svelte-check-rs/darwin-arm64",
  "linux-x64": "@svelte-check-rs/linux-x64",
  "linux-arm64": "@svelte-check-rs/linux-arm64",
  "win32-x64": "@svelte-check-rs/win32-x64"
};

function fail(message) {
  console.error(`[svelte-check-rs] ${message}`);
  process.exit(1);
}

const key = `${process.platform}-${process.arch}`;
const pkgName = PLATFORM_MAP[key];
if (!pkgName) {
  fail(`Unsupported platform/arch: ${key}.`);
}

let pkgJsonPath;
try {
  pkgJsonPath = require.resolve(`${pkgName}/package.json`);
} catch (err) {
  fail(
    `Optional dependency ${pkgName} is not installed. ` +
      "Reinstall with optional dependencies enabled (e.g. npm install, without --no-optional) " +
      "or use the shell/PowerShell installer from GitHub Releases."
  );
}

const pkgRoot = path.dirname(pkgJsonPath);
const binName = process.platform === "win32" ? "svelte-check-rs.exe" : "svelte-check-rs";
const binPath = path.join(pkgRoot, "bin", binName);

if (!fs.existsSync(binPath)) {
  fail(`Binary not found at ${binPath}. Please reinstall ${pkgName}.`);
}

const child = spawn(binPath, process.argv.slice(2), { stdio: "inherit" });
child.on("error", (err) => {
  fail(`Failed to start binary: ${err.message}`);
});
child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
  } else {
    process.exit(code == null ? 1 : code);
  }
});
