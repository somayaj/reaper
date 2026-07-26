/**
 * Static regression checks for Elide (elide.pkl) Build Tasks UI wiring.
 * elide.pkl opens Build Tasks only (not Package Manifest).
 */
export function testElideManifestRegression(appSrc, monacoSrc, ok) {
  ok(typeof appSrc === 'string' && appSrc.length > 0, 'elide ui: app.js loaded');
  ok(typeof monacoSrc === 'string' && monacoSrc.length > 0, 'elide ui: monaco-languages.js loaded');

  ok(
    monacoSrc.includes("base === 'elide.pkl'") || monacoSrc.includes('base.endsWith(\'.pkl\')'),
    'elide lang: langForPath recognizes .pkl / elide.pkl',
  );
  ok(
    monacoSrc.includes("id: 'pkl'") || monacoSrc.includes('function registerPkl'),
    'elide lang: Monaco pkl language registered',
  );
  const pklRegister = extractFunctionBody(monacoSrc, 'registerPkl');
  ok(!!pklRegister, 'elide lang: registerPkl present');
  ok(
    !pklRegister.includes('/@keywords/'),
    'elide lang: pkl tokenizer does not use invalid /@keywords/ regex',
  );
  ok(
    pklRegister.includes("'@keywords': 'keyword'")
      || pklRegister.includes('"@keywords": "keyword"'),
    'elide lang: pkl keywords matched via monarch cases',
  );
  ok(
    appSrc.includes("lower === 'elide.pkl'") || appSrc.includes("endsWith('.pkl')"),
    'elide ui: fileIcon recognizes elide.pkl',
  );

  ok(
    appSrc.includes("if (base === 'elide.pkl') return true"),
    'elide ui: isProjectBuildFile / build-tasks support includes elide.pkl',
  );
  ok(
    appSrc.includes("elide: 'Elide tasks'") || appSrc.includes('elide: "Elide tasks"'),
    'elide ui: buildTasksPanelTitle labels Elide tasks',
  );
  ok(
    appSrc.includes("'elide.pkl'") && appSrc.includes('buildTaskWorkdir'),
    'elide ui: buildTaskWorkdir treats elide.pkl as a manifest',
  );

  const workdirBody = extractFunctionBody(appSrc, 'buildTaskWorkdir');
  ok(
    workdirBody.includes('elide.pkl'),
    'elide ui: buildTaskWorkdir Set includes elide.pkl',
  );

  const kindBody = extractFunctionBody(appSrc, 'packageManifestKindForPath');
  ok(
    kindBody.includes('elide.pkl') && kindBody.includes('build-tasks only'),
    'elide ui: packageManifestKindForPath treats elide.pkl as build-tasks only',
  );
  ok(
    !kindBody.includes("return 'elide'") && !kindBody.includes('return "elide"'),
    'elide ui: packageManifestKindForPath does not open Package Manifest for elide.pkl',
  );

  const titleBody = extractFunctionBody(appSrc, 'buildTasksPanelTitle');
  ok(
    titleBody.includes('elide') && titleBody.includes('Elide tasks'),
    'elide ui: buildTasksPanelTitle has elide entry',
  );
}

function extractFunctionBody(src, name) {
  const re = new RegExp(`function ${name}\\([^)]*\\)\\s*\\{`);
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
