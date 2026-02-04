#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, "..", "..");

function parseArgs(argv) {
  const args = { tag: null, assetsDir: null, outDir: null };
  for (let i = 2; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--tag") {
      args.tag = argv[++i];
    } else if (arg === "--assets-dir") {
      args.assetsDir = argv[++i];
    } else if (arg === "--out-dir") {
      args.outDir = argv[++i];
    }
  }
  return args;
}

function fail(message) {
  console.error(`[build-packages] ${message}`);
  process.exit(1);
}

function parseTargets(tomlText) {
  const match = tomlText.match(/targets\s*=\s*\[([^\]]+)\]/m);
  if (!match) {
    throw new Error("Could not find targets in dist-workspace.toml");
  }
  return match[1]
    .split(",")
    .map((entry) => entry.trim())
    .filter(Boolean)
    .map((entry) => entry.replace(/^"|"$/g, ""));
}

function mapTarget(target) {
  const map = {
    "aarch64-apple-darwin": {
      os: "darwin",
      cpu: "arm64",
      name: "@svelte-check-rs/darwin-arm64",
      platform: "darwin-arm64"
    },
    "x86_64-apple-darwin": {
      os: "darwin",
      cpu: "x64",
      name: "@svelte-check-rs/darwin-x64",
      platform: "darwin-x64"
    },
    "aarch64-unknown-linux-gnu": {
      os: "linux",
      cpu: "arm64",
      name: "@svelte-check-rs/linux-arm64",
      platform: "linux-arm64"
    },
    "x86_64-unknown-linux-gnu": {
      os: "linux",
      cpu: "x64",
      name: "@svelte-check-rs/linux-x64",
      platform: "linux-x64"
    },
    "x86_64-pc-windows-msvc": {
      os: "win32",
      cpu: "x64",
      name: "@svelte-check-rs/win32-x64",
      platform: "win32-x64"
    }
  };
  const mapped = map[target];
  if (!mapped) {
    throw new Error(`Unsupported target in dist config: ${target}`);
  }
  return mapped;
}

function ensureDir(dir) {
  fs.mkdirSync(dir, { recursive: true });
}

function copyFileIfExists(src, dest) {
  if (fs.existsSync(src)) {
    fs.copyFileSync(src, dest);
  }
}

function copyDir(src, dest) {
  ensureDir(dest);
  for (const entry of fs.readdirSync(src, { withFileTypes: true })) {
    const srcPath = path.join(src, entry.name);
    const destPath = path.join(dest, entry.name);
    if (entry.isDirectory()) {
      copyDir(srcPath, destPath);
    } else if (entry.isFile()) {
      fs.copyFileSync(srcPath, destPath);
    }
  }
}

function findArchive(assetsDir, target) {
  const entries = fs.readdirSync(assetsDir);
  const matches = entries.filter((name) =>
    name.includes(target) &&
    (name.endsWith(".tar.xz") || name.endsWith(".tar.gz") || name.endsWith(".zip"))
  );
  if (matches.length === 0) {
    throw new Error(`No archive found for target ${target} in ${assetsDir}`);
  }
  const preferred = matches.filter((name) => name.includes("svelte-check-rs"));
  if (preferred.length === 1) {
    return preferred[0];
  }
  if (matches.length === 1) {
    return matches[0];
  }
  throw new Error(`Multiple archives found for target ${target}: ${matches.join(", ")}`);
}

function extractArchive(archivePath, destDir) {
  const name = path.basename(archivePath);
  let result;
  if (name.endsWith(".zip")) {
    result = spawnSync("unzip", ["-q", archivePath, "-d", destDir], { stdio: "inherit" });
  } else if (name.endsWith(".tar.xz")) {
    result = spawnSync("tar", ["-xJf", archivePath, "-C", destDir], { stdio: "inherit" });
  } else if (name.endsWith(".tar.gz")) {
    result = spawnSync("tar", ["-xzf", archivePath, "-C", destDir], { stdio: "inherit" });
  } else {
    throw new Error(`Unsupported archive format: ${name}`);
  }
  if (result.status !== 0) {
    throw new Error(`Failed to extract ${name}`);
  }
}

function findFileRecursive(dir, filename) {
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  for (const entry of entries) {
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      const found = findFileRecursive(fullPath, filename);
      if (found) {
        return found;
      }
    } else if (entry.isFile() && entry.name === filename) {
      return fullPath;
    }
  }
  return null;
}

function writeJson(filePath, data) {
  fs.writeFileSync(filePath, JSON.stringify(data, null, 2));
}

function npmPack(pkgDir) {
  const result = spawnSync("npm", ["pack"], { cwd: pkgDir, encoding: "utf8" });
  if (result.status !== 0) {
    console.error(result.stdout);
    console.error(result.stderr);
    throw new Error(`npm pack failed for ${pkgDir}`);
  }
  const lines = result.stdout.trim().split("\n");
  const tgzName = lines[lines.length - 1].trim();
  return path.join(pkgDir, tgzName);
}

const args = parseArgs(process.argv);
if (!args.tag || !args.assetsDir || !args.outDir) {
  fail("Usage: node scripts/npm/build-packages.mjs --tag <tag> --assets-dir <dir> --out-dir <dir>");
}

const version = args.tag.startsWith("v") ? args.tag.slice(1) : args.tag;
const assetsDir = path.resolve(args.assetsDir);
const outDir = path.resolve(args.outDir);

if (!fs.existsSync(assetsDir)) {
  fail(`Assets dir does not exist: ${assetsDir}`);
}

const distConfigPath = path.join(ROOT, "dist-workspace.toml");
const targets = parseTargets(fs.readFileSync(distConfigPath, "utf8"));
const mappedTargets = targets.map(mapTarget);

ensureDir(outDir);

const readmePath = path.join(ROOT, "README.md");
const licensePath = path.join(ROOT, "LICENSE");

const platformTgzs = [];

for (const target of targets) {
  const mapped = mapTarget(target);
  const archiveName = findArchive(assetsDir, target);
  const archivePath = path.join(assetsDir, archiveName);
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "svelte-check-rs-"));
  extractArchive(archivePath, tempDir);

  const binName = mapped.os === "win32" ? "svelte-check-rs.exe" : "svelte-check-rs";
  const binaryPath = findFileRecursive(tempDir, binName);
  if (!binaryPath) {
    throw new Error(`Binary ${binName} not found in ${archiveName}`);
  }

  const pkgDir = path.join(outDir, "@svelte-check-rs", mapped.platform);
  const binDir = path.join(pkgDir, "bin");
  ensureDir(binDir);

  const destBinary = path.join(binDir, binName);
  fs.copyFileSync(binaryPath, destBinary);
  if (mapped.os !== "win32") {
    fs.chmodSync(destBinary, 0o755);
  }

  copyFileIfExists(readmePath, path.join(pkgDir, "README.md"));
  copyFileIfExists(licensePath, path.join(pkgDir, "LICENSE"));

  const pkgJson = {
    name: mapped.name,
    version,
    description: "svelte-check-rs platform binary",
    license: "MIT",
    repository: {
      type: "git",
      url: "https://github.com/pheuter/svelte-check-rs"
    },
    homepage: "https://svelte-check-rs.vercel.app/",
    os: [mapped.os],
    cpu: [mapped.cpu],
    files: ["bin/**", "README.md", "LICENSE"]
  };

  writeJson(path.join(pkgDir, "package.json"), pkgJson);
  platformTgzs.push(npmPack(pkgDir));
}

const baseTemplateDir = path.join(ROOT, "npm", "base");
const baseOutDir = path.join(outDir, "svelte-check-rs");
copyDir(baseTemplateDir, baseOutDir);
const wrapperPath = path.join(baseOutDir, "bin", "svelte-check-rs.js");
if (fs.existsSync(wrapperPath)) {
  fs.chmodSync(wrapperPath, 0o755);
}
copyFileIfExists(readmePath, path.join(baseOutDir, "README.md"));
copyFileIfExists(licensePath, path.join(baseOutDir, "LICENSE"));

const basePkgPath = path.join(baseOutDir, "package.json");
const basePkg = JSON.parse(fs.readFileSync(basePkgPath, "utf8"));
basePkg.version = version;
basePkg.optionalDependencies = Object.fromEntries(
  mappedTargets.map((target) => [target.name, version])
);
writeJson(basePkgPath, basePkg);

const baseTgz = npmPack(baseOutDir);

const publishOrder = {
  version,
  packages: [...platformTgzs, baseTgz]
};

writeJson(path.join(outDir, "publish-order.json"), publishOrder);

console.log(`Built ${publishOrder.packages.length} packages for ${version}`);
