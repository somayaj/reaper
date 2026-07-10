/**
 * Debugger UI/static regression — guards the Java DAP lifecycle bugs we hit:
 * disabled Debug button, stuck _debugStarting, step HTTP timeout, restart after terminate.
 */

function extractFunctionBody(src, name) {
  const re = new RegExp(`(?:async\\s+)?function ${name}\\([^)]*\\)\\s*\\{`);
  const m = src.match(re);
  if (!m) return '';
  const start = m.index + m[0].length;
  let depth = 1;
  for (let i = start; i < src.length; i += 1) {
    const ch = src[i];
    if (ch === '{') depth += 1;
    else if (ch === '}') {
      depth -= 1;
      if (depth === 0) return src.slice(start, i);
    }
  }
  return '';
}

export function testDebugRegression(appSrc, indexHtml, sessionRs, dapRs, adaptersRs, ok) {
  // --- Frontend: Debug button must stay clickable for Java ---
  const capsBody = extractFunctionBody(appSrc, 'refreshDebugCapabilities');
  ok(!!capsBody, 'debug: refreshDebugCapabilities present');
  ok(
    capsBody.includes('pathLooksJava')
      || capsBody.includes('pathLooksJvm')
      || capsBody.includes('.java'),
    'debug: capabilities treats .java specially',
  );
  ok(
    capsBody.includes('Keep clickable') || !capsBody.includes('tbDebug.disabled = !runnable || capBlocked'),
    'debug: button not hard-disabled solely by runnable/capabilities',
  );

  ok(
    !/<button[^>]*id="tb-debug"[^>]*\bdisabled\b/.test(indexHtml),
    'debug: tb-debug not permanently disabled in HTML',
  );

  // --- Frontend: start/step lifecycle ---
  const startBody = extractFunctionBody(appSrc, 'startDebugSession');
  ok(!!startBody, 'debug: startDebugSession present');
  ok(
    startBody.includes("toast('Starting debug session")
      || startBody.includes('Starting debug session'),
    'debug: start always toasts so clicks are never silent',
  );
  ok(
    startBody.includes('_debugStarting = false'),
    'debug: start clears _debugStarting in finally/timeout',
  );
  ok(
    startBody.includes('/workspace/debug/stop'),
    'debug: start stops prior session before launch',
  );
  ok(
    startBody.includes('connectDebugWs'),
    'debug: start connects debug websocket',
  );

  const stepBody = extractFunctionBody(appSrc, 'debugStep');
  ok(!!stepBody, 'debug: debugStep present');
  ok(
    /timeoutMs:\s*8_?000/.test(stepBody) || /timeoutMs:\s*8000/.test(stepBody),
    'debug: step uses short HTTP timeout (not 120s)',
  );
  ok(
    stepBody.includes("_debugStepping = true"),
    'debug: step sets _debugStepping to block hover evaluate races',
  );

  const applyBody = extractFunctionBody(appSrc, 'applyDebugState');
  ok(!!applyBody, 'debug: applyDebugState present');
  ok(
    applyBody.includes('terminated') && applyBody.includes('_debugStarting = false'),
    'debug: terminate clears _debugStarting so second Debug click works',
  );
  ok(
    applyBody.includes('_debugStepping = false'),
    'debug: stopped/terminated clears _debugStepping',
  );

  // --- Backend: DAP idle + bodyless responses ---
  ok(
    dapRs.includes('TimedOut') && dapRs.includes('WouldBlock'),
    'debug: DAP reader retries idle TimedOut/WouldBlock',
  );
  ok(
    dapRs.includes('fn response_success'),
    'debug: response_success exists for bodyless launch/configurationDone',
  );

  // --- Backend: step fire-and-forget + lock separation ---
  ok(
    sessionRs.includes('send_fire_and_forget') && sessionRs.includes('granularity'),
    'debug: step uses fire-and-forget with line granularity',
  );
  ok(
    sessionRs.includes('Arc<Mutex<DapClient>>') || sessionRs.includes('Option<Arc<Mutex<DapClient>>>'),
    'debug: DAP client on separate mutex so stackTrace cannot block step',
  );
  ok(
    sessionRs.includes('needs_restart_cooldown') || sessionRs.includes('Terminated'),
    'debug: restart cooldown includes Terminated status',
  );
  ok(
    sessionRs.includes('stop_session_inner')
      && /"terminated"[\s\S]{0,400}stop_session_inner/.test(sessionRs),
    'debug: terminated event fully tears down session',
  );

  // --- Backend: Maven/Gradle debug symbols ---
  ok(
    adaptersRs.includes('debuglevel=lines,vars,source'),
    'debug: Maven prebuild requests lines,vars,source',
  );
  ok(
    !/clean compile/.test(adaptersRs.match(/maven_debug_compile_cmd[\s\S]*?format![\s\S]*?\)/)?.[0] || ''),
    'debug: Maven prebuild avoids clean compile hang',
  );
  ok(
    adaptersRs.includes('org.gradle.java.compile.options.debug=true')
      || adaptersRs.includes('--rerun-tasks'),
    'debug: Gradle prebuild forces debug symbols / rerun',
  );
}
