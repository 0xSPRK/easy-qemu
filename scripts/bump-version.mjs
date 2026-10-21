#!/usr/bin/env node
// Keep the version in one place: Cargo.toml, package.json, ui/package.json
// and src-tauri/tauri.conf.json.
//
// Usage:
//   node scripts/bump-version.mjs 0.3.0     set an explicit version
//   node scripts/bump-version.mjs patch     0.2.0 -> 0.2.1
//   node scripts/bump-version.mjs minor     0.2.0 -> 0.3.0
//   node scripts/bump-version.mjs major     0.2.0 -> 1.0.0
//   node scripts/bump-version.mjs --check   print the current version

import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

const TARGETS = [
  { file: "Cargo.toml", pattern: /^version = "[^"]+"$/m },
  { file: "package.json", pattern: /^\s*"version":\s*"[^"]+"/m },
  { file: "ui/package.json", pattern: /^\s*"version":\s*"[^"]+"/m },
  { file: "src-tauri/tauri.conf.json", pattern: /^\s*"version":\s*"[^"]+"/m },
];

const current = readVersion();

function readVersion() {
  const cargo = readFileSync(join(root, "Cargo.toml"), "utf8");
  const match = cargo.match(/^version = "([^"]+)"$/m);
  if (!match) {
    throw new Error("could not read the workspace version from Cargo.toml");
  }
  return match[1];
}

function nextVersion(bump) {
  if (/^\d+\.\d+\.\d+$/.test(bump)) {
    return bump;
  }
  const parts = current.split(".").map((n) => Number.parseInt(n, 10));
  if (parts.length !== 3 || parts.some(Number.isNaN)) {
    throw new Error(`cannot bump the non-semver version ${current}`);
  }
  const [major, minor, patch] = parts;
  if (bump === "major") return `${major + 1}.0.0`;
  if (bump === "minor") return `${major}.${minor + 1}.0`;
  if (bump === "patch") return `${major}.${minor}.${patch + 1}`;
  throw new Error(`unknown bump "${bump}"`);
}

function rewrite(file, pattern, version) {
  const path = join(root, file);
  const source = readFileSync(path, "utf8");
  if (!pattern.test(source)) {
    throw new Error(`no version field found in ${file}`);
  }
  const updated = source.replace(pattern, (match) => match.replace(current, version));
  writeFileSync(path, updated, "utf8");
}

const arg = process.argv[2];

if (!arg || arg === "--check") {
  console.log(current);
  process.exit(0);
}

const version = nextVersion(arg);

for (const target of TARGETS) {
  rewrite(target.file, target.pattern, version);
}

console.log(`${current} -> ${version}`);
