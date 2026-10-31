/**
 * Edit-path smoothness regression — avoid jarring gutter/toolbar/coverage/diagnostic churn while typing.
 */
import vm from 'node:vm';

function extractEditorChangeHandlerBody(appSrc) {
  const m = appSrc.match(
    /state\.editor\.onDidChangeModelContent\(\(\) => \{([\s\S]*?)\n    \}\);\n    state\.editor\.onDidChangeCursorPosition/,
  );
  return m?.[1] ?? '';
}

function extractCursorHandlerBody(appSrc) {
  const m = appSrc.match(
    /state\.editor\.onDidChangeCursorPosition\(\(e\) => \{([\s\S]*?)\n    \}\);\n    window\.ReaperThemes/,
  );
  return m?.[1] ?? '';
}

/**
 * @param {string} appSrc
 * @param {(pass: boolean, label: string) => void} ok
 */
export function testEditSmoothnessStaticRegression(appSrc, ok) {
  const onChangeBody = extractEditorChangeHandlerBody(appSrc);
  ok(!!onChangeBody, 'edit: onDidChangeModelContent handler found');

  ok(!onChangeBody.includes('scheduleRefreshRunInfo'), 'edit: typing does not schedule run-info API refresh');
  ok(!onChangeBody.includes('refreshRunInfo('), 'edit: typing does not call refreshRunInfo directly');
  ok(!onChangeBody.includes('scheduleUpdateRunButtons'), 'edit: typing does not refresh run toolbar (all file types)');
  ok(!onChangeBody.includes('scheduleCoverageClear'), 'edit: typing does not strip inline coverage decorations');
  ok(!onChangeBody.includes('updateRunButtons('), 'edit: typing does not rebuild toolbar DOM directly');
  ok(onChangeBody.includes('scheduleTestRunDecorations()'), 'edit: test gutter widgets debounced on content change');

  const onCursorBody = extractCursorHandlerBody(appSrc);
  ok(!!onCursorBody, 'edit: onDidChangeCursorPosition handler found');
  ok(
    onCursorBody.includes('runToolbarCursorLine'),
    'edit: run toolbar updates only when cursor line changes',
  );
  ok(
    onCursorBody.includes('scheduleUpdateRunButtons'),
    'edit: run toolbar refresh on cursor line change (not every keystroke)',
  );

  ok(appSrc.includes('function testGutterPaintKey'), 'edit: gutter layout keyed before repaint');
  ok(
    appSrc.includes('if (key === state.testGutterPaintKey) return'),
    'edit: skip glyph margin rebuild when gutter layout unchanged',
  );
  ok(
    /function scheduleTestRunDecorations\(\)[\s\S]*?isNativeSourcePath\(path\)/.test(appSrc),
    'edit: gutter debounce covers native/C++ test sources not only Java',
  );
  ok(
    !/function applyTestRunDecorations\(\)[\s\S]{0,1200}requestAnimationFrame\(\(\) => requestAnimationFrame\(paint\)\)/.test(appSrc),
    'edit: gutter paint uses single rAF (less layout thrash)',
  );
  ok(
    appSrc.includes('state.testGutterPaintKey = null'),
    'edit: gutter key reset when clearing test widgets',
  );

  ok(appSrc.includes('function scheduleRefreshRunInfo'), 'edit: run-info API refresh is explicitly schedulable');
  ok(
    appSrc.includes('RUN_INFO_REFRESH_DELAY_MS'),
    'edit: run-info API refresh debounced',
  );
  ok(
    /async function refreshRunInfo\(\)[\s\S]*?if \(key !== state\.testGutterPaintKey\) applyTestRunDecorations\(\)/.test(appSrc),
    'edit: refreshRunInfo skips redundant gutter repaint',
  );

  ok(appSrc.includes('runToolbarSig'), 'edit: run toolbar DOM keyed to avoid redundant updates');
  ok(
    /function updateRunButtons\([\s\S]*?if \(runToolbarSig === state\.runToolbarSig\)/.test(appSrc),
    'edit: updateRunButtons short-circuits when toolbar state unchanged',
  );

  ok(appSrc.includes('function diagnosticsSignature'), 'edit: diagnosticsSignature helper defined');
  ok(
    /function applyDiagnostics\([\s\S]*?if \(sig === lastAppliedDiagSig\) return/.test(appSrc),
    'edit: applyDiagnostics skips identical marker/bulb repaints',
  );
  ok(
    appSrc.includes("lastAppliedDiagSig = ''") && appSrc.includes('runToolbarCursorLine = null'),
    'edit: tab switch resets diag/toolbar cache keys',
  );

  ok(
    /function applyTestRunDecorations\(\)[\s\S]*requestAnimationFrame\(paint\)/.test(appSrc),
    'edit: gutter widget updates deferred to animation frame',
  );
  ok(
    /const paint = \(\) => \{[\s\S]*testGutterPaintKey\([\s\S]*clearTestRunWidgets\(\)/.test(appSrc),
    'edit: gutter widgets cleared inside paint only after layout key changes',
  );
}

/**
 * @param {(pass: boolean, label: string) => void} ok
 */
export function testDiagnosticsSignatureUnit(ok) {
  const sandbox = { console };
  vm.createContext(sandbox);
  vm.runInContext(
    `function diagnosticsSignature(diags) {
  if (!diags?.length) return '';
  return diags.map((d) => \`\${d.line}:\${d.column}:\${d.severity}:\${d.message}\`).join('|');
}`,
    sandbox,
  );

  const a = [{ line: 1, column: 2, severity: 'error', message: 'x' }];
  const b = [{ line: 1, column: 2, severity: 'error', message: 'x' }];
  const c = [{ line: 1, column: 2, severity: 'error', message: 'y' }];
  ok(sandbox.diagnosticsSignature(a) === sandbox.diagnosticsSignature(b), 'diagnosticsSignature: stable for identical diags');
  ok(sandbox.diagnosticsSignature(a) !== sandbox.diagnosticsSignature(c), 'diagnosticsSignature: changes when message changes');
  ok(sandbox.diagnosticsSignature([]) === '', 'diagnosticsSignature: empty input → empty sig');
}
