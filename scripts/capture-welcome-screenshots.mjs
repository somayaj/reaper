#!/usr/bin/env node
/**
 * Capture Reaper UI screenshots for the welcome home page.
 * Usage: node scripts/capture-welcome-screenshots.mjs [port]
 */
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, '..');
const OUT = join(ROOT, 'static', 'screenshots');
const PORT = Number(process.argv[2] || process.env.REAPER_PORT || 0) || await readDefaultPort();
const BASE = `https://127.0.0.1:${PORT}`;

async function readDefaultPort() {
  try {
    const home = process.env.HOME || '';
    const raw = await readFile(join(home, 'reaper', 'reaper.port'), 'utf8');
    const n = Number(String(raw).trim());
    if (n > 0) return n;
  } catch {
    /* fall through */
  }
  return 63319;
}

async function padBlack(src, dest, pad = 14) {
  const { createRequire } = await import('node:module');
  const require = createRequire(import.meta.url);
  let sharp;
  try {
    sharp = require('sharp');
  } catch {
    await writeFile(dest, src);
    return;
  }
  const meta = await sharp(src).metadata();
  const w = (meta.width || 0) + pad * 2;
  const h = (meta.height || 0) + pad * 2;
  await sharp({
    create: {
      width: w,
      height: h,
      channels: 3,
      background: { r: 10, g: 10, b: 10 },
    },
  })
    .composite([{ input: src, top: pad, left: pad }])
    .png()
    .toFile(dest);
}

async function capture(page, url, selector, outName) {
  await page.goto(url, { waitUntil: 'domcontentloaded', ignoreHTTPSErrors: true });
  if (selector) {
    await page.waitForSelector(selector, { timeout: 45000 }).catch(() => {});
  }
  await page.waitForTimeout(1200);
  const raw = join(OUT, `_raw-${outName}.png`);
  const final = join(OUT, outName);
  await page.screenshot({ path: raw, fullPage: false });
  await padBlack(raw, final);
  console.log('wrote', final);
}

async function main() {
  await mkdir(OUT, { recursive: true });
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1440, height: 920 } });
  page.setDefaultTimeout(60000);

  await capture(page, `${BASE}/`, '#empty-state.ij-welcome', 'welcome-home.png');
  await capture(page, `${BASE}/?repo=hello-world-java`, '#editor-container:not(.hidden)', 'editor-java.png');
  await page.goto(`${BASE}/?repo=hello-world-java`, { waitUntil: 'domcontentloaded', ignoreHTTPSErrors: true });
  await page.waitForTimeout(4000);
  await page.evaluate(() => document.querySelector('[data-panel="git"]')?.click());
  await page.waitForTimeout(1500);
  const raw = join(OUT, '_raw-git-commit.png');
  const final = join(OUT, 'git-commit.png');
  await page.screenshot({ path: raw, fullPage: false });
  await padBlack(raw, final);
  console.log('wrote', final);

  await browser.close();
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
