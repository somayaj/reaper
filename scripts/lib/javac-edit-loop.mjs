/** Shared edit → save → javac loop helpers (editor parity). */

export const EDIT_COUNT_DEFAULT = 25;
export const MAX_JAVAC_RETRIES = 3;
export const JAVA_SAVE_DIAG_DELAY_MS = 300;
export const AUTO_SAVE_DELAY_MS = 2000;

export const DEFAULT_JAVA_REL =
  'src/main/java/com/example/EditLoopApp.java';

export function editLoopJavaContent(marker) {
  return `package com.example;

public class EditLoopApp {
  public static void main(String[] args) {
    int step = ${marker};
    int bad = undeclaredVar${marker};
    System.out.println(step + bad);
  }
}
`;
}

export function springEditContent(base, marker) {
  const inject = `\n    int reaperLoop${marker} = undeclaredSym${marker};\n`;
  const idx = base.indexOf('SpringApplication.run');
  if (idx >= 0) return `${base.slice(0, idx)}${inject}${base.slice(idx)}`;
  return `${base}${inject}`;
}

export function normalizeDiag(body) {
  if (Array.isArray(body)) return { diagnostics: body, cancelled: false };
  return {
    diagnostics: Array.isArray(body?.diagnostics) ? body.diagnostics : [],
    cancelled: body?.cancelled === true,
  };
}

export function diagsJoin(result) {
  const diags = result?.diagnostics ?? [];
  return diags.map((d) => String(d.message || '').toLowerCase()).join('\n');
}

/** Avoid undeclaredvar9 matching inside undeclaredvar99. */
export function messageContainsSymbol(message, symbol) {
  const re = new RegExp(`\\b${symbol.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\b`, 'i');
  return re.test(message);
}

export function markerNeedles(marker) {
  return [`undeclaredvar${marker}`, `undeclaredsym${marker}`];
}

export function assertMarkerDiagnostic(result, marker, ok, fail) {
  const joined = diagsJoin(result);
  if (result.cancelled && !result.diagnostics.length) {
    fail(`edit ${marker}: javac cancelled with no diagnostics`);
    return false;
  }
  const needles = markerNeedles(marker);
  const hasErr = needles.some((n) => messageContainsSymbol(joined, n))
    || joined.includes('cannot find symbol');
  return ok(hasErr, `edit ${marker}: javac reports current error (${needles.join(' / ')})`);
}

export function assertNoStale(result, staleMarker, ok, fail) {
  const stale = markerNeedles(staleMarker);
  const joined = diagsJoin(result);
  const staleHit = stale.some((s) => messageContainsSymbol(joined, s));
  return ok(!staleHit, `edit ${staleMarker + 1}+: no stale squiggle for edit ${staleMarker}`);
}

/** Latest-buffer response must not mention symbols from earlier edits. */
export function assertLatestHasNoStaleMarkers(result, latestMarker, ok, fail) {
  const joined = diagsJoin(result);
  for (let stale = 0; stale < latestMarker; stale += 1) {
    for (const needle of markerNeedles(stale)) {
      if (messageContainsSymbol(joined, needle)) {
        fail(`latest marker ${latestMarker}: stale diagnostic for edit ${stale} (${needle})`);
        return false;
      }
    }
  }
  return ok(
    true,
    `latest marker ${latestMarker}: no stale symbols from edits 0–${latestMarker - 1}`,
  );
}

export function summarizeDiagnosticBurst(settled) {
  const httpOk = settled.filter((s) => s.httpOk);
  const cancelledEmpty = httpOk.filter(
    (s) => s.result.cancelled && !s.result.diagnostics.length,
  ).length;
  const cancelledWithDiags = httpOk.filter(
    (s) => s.result.cancelled && s.result.diagnostics.length,
  ).length;
  const completed = httpOk.filter(
    (s) => !s.result.cancelled && s.result.diagnostics.length,
  ).length;
  const withDiags = httpOk.filter((s) => s.result.diagnostics.length).length;
  return {
    httpOk: httpOk.length,
    cancelledEmpty,
    cancelledWithDiags,
    completed,
    withDiags,
  };
}

export function reportDiagnosticBurstStats(label, settled, burstMs, ok) {
  const stats = summarizeDiagnosticBurst(settled);
  ok(
    true,
    `${label}: ${stats.httpOk} client-parallel POSTs in ${burstMs}ms — `
      + `cancelled_empty=${stats.cancelledEmpty} cancelled_with_diags=${stats.cancelledWithDiags} `
      + `completed=${stats.completed} with_diags=${stats.withDiags} `
      + '(server serializes javac per workspace)',
  );
  return stats;
}

export async function sleep(ms) {
  await new Promise((r) => setTimeout(r, ms));
}

/** Mirror app.js fetchDiagnosticsForPath body for full javac. */
export async function postFullDiagnostics(baseUrl, repo, filePath, content, overlays = []) {
  const res = await fetch(`${baseUrl}/api/repos/${encodeURIComponent(repo)}/workspace/diagnostics`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      path: filePath,
      content,
      scope: 'full',
      overlays,
    }),
  });
  const text = await res.text();
  if (!res.ok) {
    throw new Error(`diagnostics HTTP ${res.status}: ${text.slice(0, 300)}`);
  }
  return normalizeDiag(text ? JSON.parse(text) : {});
}

export async function diagnoseWithRetries(baseUrl, repo, filePath, content, overlays = []) {
  let last = { diagnostics: [], cancelled: true };
  for (let attempt = 0; attempt <= MAX_JAVAC_RETRIES; attempt += 1) {
    last = await postFullDiagnostics(baseUrl, repo, filePath, content, overlays);
    if (!last.cancelled || last.diagnostics.length) return last;
    if (attempt < MAX_JAVAC_RETRIES) await sleep(400 * (attempt + 1));
  }
  return last;
}

/** Mirror writeTabToDisk / saveFile PUT. */
export async function putSaveFile(baseUrl, repo, filePath, content) {
  const res = await fetch(`${baseUrl}/api/repos/${encodeURIComponent(repo)}/workspace/file`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ path: filePath, content }),
  });
  if (res.status !== 204) {
    const text = await res.text();
    throw new Error(`save HTTP ${res.status} (expected 204): ${text.slice(0, 300)}`);
  }
}

export async function readDiskFile(baseUrl, repo, filePath) {
  const q = new URLSearchParams({ path: filePath });
  const res = await fetch(`${baseUrl}/api/repos/${encodeURIComponent(repo)}/workspace/file?${q}`);
  const text = await res.text();
  if (!res.ok) throw new Error(`read HTTP ${res.status}: ${text.slice(0, 300)}`);
  const body = text ? JSON.parse(text) : {};
  return body.content ?? '';
}

/**
 * Editor-parity loop: auto-save delay → PUT save → debounced javac (scope full).
 * @param {object} opts
 * @param {(cond: boolean, msg: string) => boolean} opts.ok
 * @param {(msg: string) => void} opts.fail
 */
export async function runEditorParityEditLoop(opts) {
  const {
    baseUrl,
    repo,
    javaFile,
    editCount = EDIT_COUNT_DEFAULT,
    baseContent,
    useSpringShape = false,
    simulateTypingDelay = true,
    ok,
    fail,
  } = opts;

  const backup = baseContent;
  let failures = 0;

  for (let marker = 0; marker < editCount; marker += 1) {
    if (simulateTypingDelay) await sleep(AUTO_SAVE_DELAY_MS);

    const content = useSpringShape
      ? springEditContent(baseContent, marker)
      : editLoopJavaContent(marker);

    if (simulateTypingDelay) await sleep(JAVA_SAVE_DIAG_DELAY_MS);

    const t0 = Date.now();
    await putSaveFile(baseUrl, repo, javaFile, content);
    const saveMs = Date.now() - t0;

    const disk = await readDiskFile(baseUrl, repo, javaFile);
    if (!ok(disk === content, `edit ${marker}: save persisted to disk (${saveMs}ms)`)) failures += 1;

    const t1 = Date.now();
    const result = await diagnoseWithRetries(baseUrl, repo, javaFile, content, []);
    const diagMs = Date.now() - t1;

    if (!ok(!result.cancelled || result.diagnostics.length,
      `edit ${marker}: full javac completed (${diagMs}ms, ${result.diagnostics.length} diag(s))`)) {
      failures += 1;
    }
    if (!assertMarkerDiagnostic(result, marker, ok, fail)) failures += 1;
    if (marker > 0 && !assertNoStale(result, marker - 1, ok, fail)) failures += 1;
  }

  await putSaveFile(baseUrl, repo, javaFile, backup);
  return failures;
}

/**
 * Fire N client-parallel full-javac POSTs with distinct buffer versions (stale + latest).
 * Client fires simultaneously; server serializes javac per workspace.
 */
export async function runConcurrentDiagnosticsBurst(opts) {
  const {
    baseUrl,
    repo,
    javaFile,
    burstCount = EDIT_COUNT_DEFAULT,
    ok,
    fail,
  } = opts;

  const latestMarker = burstCount - 1;
  const latestContent = editLoopJavaContent(latestMarker);
  await putSaveFile(baseUrl, repo, javaFile, latestContent);

  const t0 = Date.now();
  const settled = await Promise.all(
    Array.from({ length: burstCount }, (_, marker) =>
      postFullDiagnostics(baseUrl, repo, javaFile, editLoopJavaContent(marker), [])
        .then((result) => ({ marker, result, httpOk: true }))
        .catch((err) => ({ marker, err, httpOk: false })),
    ),
  );
  const burstMs = Date.now() - t0;

  let failures = 0;
  const httpOkCount = settled.filter((s) => s.httpOk).length;
  if (!ok(httpOkCount === burstCount,
    `client-parallel diag: ${burstCount} POSTs all returned HTTP 200`)) {
    failures += 1;
    for (const s of settled.filter((x) => !x.httpOk)) {
      fail(`client-parallel diag: marker ${s.marker} HTTP failed: ${s.err?.message || s.err}`);
    }
  }

  reportDiagnosticBurstStats('client-parallel diag', settled, burstMs, ok);

  const latest = settled.find((s) => s.marker === latestMarker && s.httpOk);
  if (latest) {
    const usable = !latest.result.cancelled || latest.result.diagnostics.length;
    if (!ok(usable, `client-parallel diag: latest buffer (marker ${latestMarker}) returned diagnostics`)) {
      failures += 1;
    }
    if (!assertMarkerDiagnostic(latest.result, latestMarker, ok, fail)) failures += 1;
    if (!assertLatestHasNoStaleMarkers(latest.result, latestMarker, ok, fail)) failures += 1;
  }

  const final = await diagnoseWithRetries(baseUrl, repo, javaFile, latestContent, []);
  if (!ok(!final.cancelled || final.diagnostics.length,
    'client-parallel diag: post-burst javac on latest buffer completes')) {
    failures += 1;
  }
  if (!assertMarkerDiagnostic(final, latestMarker, ok, fail)) failures += 1;
  if (!assertLatestHasNoStaleMarkers(final, latestMarker, ok, fail)) failures += 1;

  return failures;
}

/**
 * Fire N client-parallel PUT saves with distinct content; disk should settle, then javac succeeds.
 */
export async function runConcurrentSaveBurst(opts) {
  const {
    baseUrl,
    repo,
    javaFile,
    burstCount = EDIT_COUNT_DEFAULT,
    baseContent,
    ok,
    fail,
  } = opts;

  const payloads = Array.from({ length: burstCount }, (_, marker) => editLoopJavaContent(marker));
  const t0 = Date.now();
  const settled = await Promise.all(
    payloads.map((content, marker) =>
      putSaveFile(baseUrl, repo, javaFile, content)
        .then(() => ({ marker, ok: true }))
        .catch((err) => ({ marker, ok: false, err })),
    ),
  );
  const burstMs = Date.now() - t0;

  let failures = 0;
  const saveOkCount = settled.filter((s) => s.ok).length;
  if (!ok(saveOkCount === burstCount,
    `client-parallel save: ${burstCount} PUTs all returned 204 (${burstMs}ms)`)) {
    failures += 1;
    for (const s of settled.filter((x) => !x.ok)) {
      fail(`client-parallel save: marker ${s.marker} failed: ${s.err?.message || s.err}`);
    }
  }

  const disk = await readDiskFile(baseUrl, repo, javaFile);
  const diskMarker = payloads.findIndex((p) => p === disk);
  if (!ok(diskMarker >= 0, 'client-parallel save: disk content matches one burst payload')) failures += 1;

  if (diskMarker >= 0) {
    const result = await diagnoseWithRetries(baseUrl, repo, javaFile, disk, []);
    if (!ok(!result.cancelled || result.diagnostics.length,
      `client-parallel save: javac after burst completes for disk marker ${diskMarker}`)) {
      failures += 1;
    }
    if (!assertMarkerDiagnostic(result, diskMarker, ok, fail)) failures += 1;
  }

  await putSaveFile(baseUrl, repo, javaFile, baseContent);
  return failures;
}

/**
 * Fire N client-parallel save→full-javac pairs (storm without client coalescing).
 */
export async function runConcurrentSaveDiagnosticBurst(opts) {
  const {
    baseUrl,
    repo,
    javaFile,
    burstCount = EDIT_COUNT_DEFAULT,
    baseContent,
    ok,
    fail,
  } = opts;

  const t0 = Date.now();
  const settled = await Promise.all(
    Array.from({ length: burstCount }, (_, marker) => {
      const content = editLoopJavaContent(marker);
      return putSaveFile(baseUrl, repo, javaFile, content)
        .then(() => postFullDiagnostics(baseUrl, repo, javaFile, content, []))
        .then((result) => ({ marker, result, ok: true }))
        .catch((err) => ({ marker, ok: false, err }));
    }),
  );
  const burstMs = Date.now() - t0;

  let failures = 0;
  const pairOkCount = settled.filter((s) => s.ok).length;
  if (!ok(pairOkCount === burstCount,
    `client-parallel save+diag: ${burstCount} pairs completed (${burstMs}ms)`)) {
    failures += 1;
    for (const s of settled.filter((x) => !x.ok)) {
      fail(`client-parallel save+diag: marker ${s.marker} failed: ${s.err?.message || s.err}`);
    }
  }

  reportDiagnosticBurstStats(
    'client-parallel save+diag',
    settled.map((s) => ({
      httpOk: s.ok,
      result: s.result ?? { cancelled: true, diagnostics: [] },
    })),
    burstMs,
    ok,
  );

  const disk = await readDiskFile(baseUrl, repo, javaFile);
  const diskMarker = Array.from({ length: burstCount }, (_, m) => m)
    .find((m) => editLoopJavaContent(m) === disk);
  if (!ok(diskMarker != null && diskMarker >= 0, 'client-parallel save+diag: disk matches one pair payload')) {
    failures += 1;
  } else {
    const pair = settled.find((s) => s.ok && s.marker === diskMarker);
    if (pair && !assertMarkerDiagnostic(pair.result, diskMarker, ok, fail)) failures += 1;
    if (pair && !assertLatestHasNoStaleMarkers(pair.result, diskMarker, ok, fail)) failures += 1;
    const final = await diagnoseWithRetries(baseUrl, repo, javaFile, disk, []);
    if (!ok(!final.cancelled || final.diagnostics.length,
      `client-parallel save+diag: post-burst javac for disk marker ${diskMarker}`)) {
      failures += 1;
    }
    if (!assertMarkerDiagnostic(final, diskMarker, ok, fail)) failures += 1;
    if (!assertLatestHasNoStaleMarkers(final, diskMarker, ok, fail)) failures += 1;
  }

  await putSaveFile(baseUrl, repo, javaFile, baseContent);
  return failures;
}
