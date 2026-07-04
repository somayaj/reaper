/**
 * Minimal mirror of app.js queueJavaFullDiagnostics / flushJavaFullDiagnosticQueue
 * for integration tests (one compile at a time, latest buffer wins).
 */
import {
  EDIT_COUNT_DEFAULT,
  JAVA_SAVE_DIAG_DELAY_MS,
  assertLatestHasNoStaleMarkers,
  assertMarkerDiagnostic,
  diagsJoin,
  editLoopJavaContent,
  markerNeedles,
  messageContainsSymbol,
  postFullDiagnostics,
  putSaveFile,
  sleep,
} from './javac-edit-loop.mjs';

export function createJavaFullDiagCoalescer({
  javaSaveDiagDelayMs = JAVA_SAVE_DIAG_DELAY_MS,
  getActiveTab,
  getEditorValue,
  runFullDiagnosticsForPath,
} = {}) {
  let timer = null;
  let running = false;
  let pending = null;
  let maxInFlight = 0;
  let compileCount = 0;
  /** @type {string[]} */
  const compiledContents = [];
  /** @type {unknown[]} */
  const appliedResults = [];

  async function flushQueue() {
    timer = null;
    if (running) return;
    while (pending) {
      const job = pending;
      pending = null;
      if (job.path !== getActiveTab()) continue;
      running = true;
      maxInFlight = Math.max(maxInFlight, 1);
      try {
        const latest = getEditorValue() ?? job.content;
        compileCount += 1;
        compiledContents.push(latest);
        const result = await runFullDiagnosticsForPath(job.path, latest, job.opts ?? {});
        appliedResults.push(result);
      } finally {
        running = false;
      }
    }
  }

  function queueJavaFullDiagnostics(path, content, opts = {}) {
    const { immediate = false, fromSave = false, force = false } = opts;
    pending = { path, content, opts: { fromSave, force } };
    clearTimeout(timer);
    timer = setTimeout(() => {
      void flushQueue();
    }, immediate ? 0 : javaSaveDiagDelayMs);
  }

  async function drain() {
    clearTimeout(timer);
    timer = null;
    await flushQueue();
    for (let i = 0; i < 200; i += 1) {
      if (!running && !pending) break;
      await flushQueue();
      await sleep(5);
    }
  }

  return {
    queueJavaFullDiagnostics,
    drain,
    stats: () => ({ maxInFlight, compileCount, compiledContents, appliedResults }),
  };
}

/** Unit test: coalescer without HTTP — one in-flight compile, latest buffer wins. */
export async function testCoalescerUnit({ burstCount = EDIT_COUNT_DEFAULT, ok, fail }) {
  let inFlight = 0;
  let maxConcurrent = 0;
  let editorValue = editLoopJavaContent(0);
  const path = 'src/main/java/com/example/EditLoopApp.java';

  const coalescer = createJavaFullDiagCoalescer({
    javaSaveDiagDelayMs: 5,
    getActiveTab: () => path,
    getEditorValue: () => editorValue,
    runFullDiagnosticsForPath: async (_path, content) => {
      inFlight += 1;
      maxConcurrent = Math.max(maxConcurrent, inFlight);
      await sleep(15);
      inFlight -= 1;
      return { content, marker: markerFromContent(content) };
    },
  });

  for (let marker = 0; marker < burstCount; marker += 1) {
    editorValue = editLoopJavaContent(marker);
    coalescer.queueJavaFullDiagnostics(path, editorValue, {
      immediate: marker === burstCount - 1,
    });
  }
  await coalescer.drain();

  const { compileCount, compiledContents } = coalescer.stats();
  let failures = 0;
  if (!ok(maxConcurrent === 1, `coalescer unit: at most one compile in flight (max=${maxConcurrent})`)) failures += 1;
  if (!ok(compileCount < burstCount,
    `coalescer unit: coalesced ${burstCount} queue calls to ${compileCount} compile(s)`)) {
    failures += 1;
  }
  const lastCompiled = compiledContents.at(-1);
  const lastMarker = markerFromContent(lastCompiled);
  if (!ok(lastMarker === burstCount - 1,
    `coalescer unit: final compile uses latest buffer (marker ${burstCount - 1})`)) {
    failures += 1;
  }
  return failures;
}

function markerFromContent(content) {
  const m = String(content).match(/undeclaredVar(\d+)/);
  return m ? Number.parseInt(m[1], 10) : -1;
}

/**
 * Client-parallel burst through coalescer + real HTTP (editor parity under load).
 * Server may serialize javac; client must still coalesce to one in-flight compile.
 */
export async function runCoalescedClientBurst(opts) {
  const {
    baseUrl,
    repo,
    javaFile,
    burstCount = EDIT_COUNT_DEFAULT,
    baseContent,
    ok,
    fail,
  } = opts;

  let editorValue = baseContent;
  let clientInFlight = 0;
  let clientMaxInFlight = 0;
  const latestMarker = burstCount - 1;

  const coalescer = createJavaFullDiagCoalescer({
    javaSaveDiagDelayMs: JAVA_SAVE_DIAG_DELAY_MS,
    getActiveTab: () => javaFile,
    getEditorValue: () => editorValue,
    runFullDiagnosticsForPath: async (path, content) => {
      clientInFlight += 1;
      clientMaxInFlight = Math.max(clientMaxInFlight, clientInFlight);
      try {
        return await postFullDiagnostics(baseUrl, repo, path, content, []);
      } finally {
        clientInFlight -= 1;
      }
    },
  });

  const t0 = Date.now();
  const enqueue = Array.from({ length: burstCount }, (_, marker) => async () => {
    editorValue = editLoopJavaContent(marker);
    await putSaveFile(baseUrl, repo, javaFile, editorValue);
    coalescer.queueJavaFullDiagnostics(javaFile, editorValue, {
      immediate: marker === latestMarker,
      fromSave: true,
      force: true,
    });
  });
  await Promise.all(enqueue.map((fn) => fn()));
  await coalescer.drain();
  const burstMs = Date.now() - t0;

  let failures = 0;
  const { compileCount, appliedResults } = coalescer.stats();
  ok(
    true,
    `coalesced client burst: ${burstCount} client-parallel save+queue → ${compileCount} compile(s) (${burstMs}ms)`,
  );
  if (!ok(clientMaxInFlight === 1,
    `coalesced client burst: one compile in flight on client (max=${clientMaxInFlight})`)) {
    failures += 1;
  }
  if (!ok(compileCount < burstCount,
    `coalesced client burst: fewer compiles than queue calls (${compileCount}/${burstCount})`)) {
    failures += 1;
  }

  const finalResult = appliedResults.at(-1);
  if (!finalResult) {
    fail('coalesced client burst: no compile result applied');
    failures += 1;
  } else {
    if (!ok(!finalResult.cancelled || finalResult.diagnostics.length,
      `coalesced client burst: final javac returned diagnostics (cancelled=${finalResult.cancelled})`)) {
      failures += 1;
    }
    if (!assertMarkerDiagnostic(finalResult, latestMarker, ok, fail)) failures += 1;
    if (!assertLatestHasNoStaleMarkers(finalResult, latestMarker, ok, fail)) failures += 1;
  }

  await putSaveFile(baseUrl, repo, javaFile, baseContent);
  return failures;
}

export function staleMarkersInResult(result, latestMarker) {
  const joined = diagsJoin(result);
  const hits = [];
  for (let stale = 0; stale < latestMarker; stale += 1) {
    for (const needle of markerNeedles(stale)) {
      if (messageContainsSymbol(joined, needle)) hits.push({ stale, needle });
    }
  }
  return hits;
}
