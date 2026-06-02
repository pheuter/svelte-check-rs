#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";

function parseArgs(argv) {
  const args = { tag: null, outDir: null };
  for (let i = 2; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--tag") {
      args.tag = argv[++i];
    } else if (arg === "--out-dir") {
      args.outDir = argv[++i];
    }
  }
  return args;
}

function fail(message) {
  console.error(`[publish-packages] ${message}`);
  process.exit(1);
}

// npm returns a 403 with this message when the exact version already exists on
// the registry. This also surfaces after a transient retry where the first PUT
// actually succeeded but the client retried and saw the version now present.
// Treat it as success so a re-run (or a partially-published release) is
// idempotent and never strands later packages in publish-order.json.
function isAlreadyPublished(output) {
  return (
    /cannot publish over the previously published versions?/i.test(output) ||
    /EPUBLISHCONFLICT/.test(output)
  );
}

const { tag, outDir } = parseArgs(process.argv);
if (!tag || !outDir) {
  fail("Usage: node scripts/npm/publish-packages.mjs --tag <tag> --out-dir <dir>");
}

const distTag = tag.includes("-") ? "beta" : "latest";
const orderPath = path.join(outDir, "publish-order.json");
const { packages } = JSON.parse(fs.readFileSync(orderPath, "utf8"));

const skipped = [];
const failures = [];

for (const pkg of packages) {
  console.log(`Publishing ${pkg} (tag ${distTag})`);
  const result = spawnSync(
    "npm",
    ["publish", "--access", "public", "--tag", distTag, pkg],
    { encoding: "utf8" }
  );

  if (result.stdout) process.stdout.write(result.stdout);
  if (result.stderr) process.stderr.write(result.stderr);

  if (result.status === 0) continue;

  const output = `${result.stdout || ""}\n${result.stderr || ""}`;
  if (isAlreadyPublished(output)) {
    console.log(`  -> already published, skipping ${pkg}`);
    skipped.push(pkg);
    continue;
  }

  failures.push(pkg);
}

console.log(
  `\nPublish summary: ${packages.length - skipped.length - failures.length} published, ` +
    `${skipped.length} already present, ${failures.length} failed`
);

if (failures.length > 0) {
  fail(`Failed to publish: ${failures.join(", ")}`);
}
