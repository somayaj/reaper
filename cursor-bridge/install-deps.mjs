/**
 * Install cursor-bridge dependencies using Node fetch + system tar (no npm CLI required).
 */
import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const ROOT = path.dirname(fileURLToPath(import.meta.url));
const NODE_MODULES = path.join(ROOT, "node_modules");
const REGISTRY = "https://registry.npmjs.org";

function readPackageJson() {
  return JSON.parse(fs.readFileSync(path.join(ROOT, "package.json"), "utf8"));
}

function registryUrl(name) {
  const encoded = encodeURIComponent(name).replace(/^%40/, "@");
  return `${REGISTRY}/${encoded}`;
}

function packageDir(name) {
  return path.join(NODE_MODULES, ...name.split("/"));
}

function isInstalled(name) {
  return fs.existsSync(path.join(packageDir(name), "package.json"));
}

async function fetchJson(url) {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`registry fetch failed (${res.status}): ${url}`);
  return res.json();
}

function compareSemver(a, b) {
  const pa = a.split(".").map(Number);
  const pb = b.split(".").map(Number);
  for (let i = 0; i < 3; i++) {
    const diff = (pa[i] || 0) - (pb[i] || 0);
    if (diff !== 0) return diff;
  }
  return 0;
}

function isStableVersion(version) {
  return !version.includes("-");
}

function resolveVersion(meta, range) {
  if (range === "latest") return meta["dist-tags"]?.latest;
  const versions = Object.keys(meta.versions || {})
    .filter(isStableVersion)
    .sort(compareSemver);
  if (!versions.length) return null;

  if (/^\d+\.\d+\.\d+(?:[-+].*)?$/.test(range)) {
    return meta.versions?.[range] ? range : null;
  }

  if (range.startsWith("^")) {
    const [maj, min = 0, pat = 0] = range.slice(1).split(".").map(Number);
    const matches = versions.filter((v) => {
      const [vm, vn = 0, vp = 0] = v.split(".").map(Number);
      if (vm !== maj) return false;
      if (vn > min) return true;
      if (vn < min) return false;
      return vp >= pat;
    });
    return matches.at(-1) || null;
  }

  if (range.startsWith("~")) {
    const [maj, min = 0, pat = 0] = range.slice(1).split(".").map(Number);
    const matches = versions.filter((v) => {
      const [vm, vn = 0, vp = 0] = v.split(".").map(Number);
      return vm === maj && vn === min && vp >= pat;
    });
    return matches.at(-1) || null;
  }

  const tag = meta["dist-tags"]?.[range];
  if (tag) return tag;

  return versions.at(-1) || null;
}

async function downloadAndExtract(name, version) {
  const dir = packageDir(name);
  if (isInstalled(name)) {
    const existing = JSON.parse(fs.readFileSync(path.join(dir, "package.json"), "utf8"));
    if (existing.version === version) {
      return;
    }
    fs.rmSync(dir, { recursive: true, force: true });
  }

  const meta = await fetchJson(registryUrl(name));
  const info = meta.versions?.[version];
  if (!info?.dist?.tarball) {
    throw new Error(`no tarball for ${name}@${version}`);
  }

  fs.mkdirSync(dir, { recursive: true });
  const tgz = path.join(dir, ".package.tgz");
  const res = await fetch(info.dist.tarball);
  if (!res.ok) throw new Error(`download failed for ${name}@${version}`);
  fs.writeFileSync(tgz, Buffer.from(await res.arrayBuffer()));
  execFileSync("tar", ["-xzf", tgz, "-C", dir, "--strip-components=1"], { stdio: "inherit" });
  fs.unlinkSync(tgz);
}

async function install(name, range, seen = new Set()) {
  const key = `${name}@${range}`;
  if (seen.has(key)) return;
  seen.add(key);

  const meta = await fetchJson(registryUrl(name));
  const version = resolveVersion(meta, range);
  if (!version) throw new Error(`cannot resolve ${name}@${range}`);

  await downloadAndExtract(name, version);

  const pkg = JSON.parse(fs.readFileSync(path.join(packageDir(name), "package.json"), "utf8"));
  const deps = { ...pkg.dependencies, ...pkg.optionalDependencies };
  for (const [dep, depRange] of Object.entries(deps || {})) {
    await install(dep, depRange, seen);
  }
}

const pkg = readPackageJson();
for (const [name, range] of Object.entries(pkg.dependencies || {})) {
  await install(name, range);
}

console.log("cursor-bridge dependencies installed");

if (process.platform === "win32") {
  await import("./patch-sdk-windows-hide.mjs");
}
