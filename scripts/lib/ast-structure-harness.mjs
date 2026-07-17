/**
 * Multi-language Structure / AST regression — API + Structure tool window wiring.
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

export function testAstStructureRegression(appSrc, indexHtml, apiRs, astRs, languagesRs, cargoToml, ok) {
  // --- Backend: API + module ---
  ok(
    apiRs.includes('workspace/ast')
      && apiRs.includes('workspace_ast_get')
      && apiRs.includes('workspace_ast_post'),
    'ast api: GET/POST /workspace/ast routes registered',
  );
  ok(
    apiRs.includes('AstBody') && apiRs.includes('content'),
    'ast api: POST accepts dirty buffer content',
  );
  ok(
    astRs.includes('pub fn parse_ast') && astRs.includes('AstMode'),
    'ast rust: parse_ast + AstMode exported',
  );
  ok(
    astRs.includes('tree_sitter_java')
      && astRs.includes('tree_sitter_python')
      && astRs.includes('tree_sitter_javascript')
      && astRs.includes('tree_sitter_typescript')
      && astRs.includes('tree_sitter_go')
      && astRs.includes('tree_sitter_rust')
      && astRs.includes('tree_sitter_c')
      && astRs.includes('tree_sitter_cpp')
      && astRs.includes('tree_sitter_json')
      && astRs.includes('tree_sitter_yaml'),
    'ast rust: MVP language grammars wired',
  );
  ok(
    astRs.includes('LANGUAGE_TYPESCRIPT') && astRs.includes('LANGUAGE_TSX'),
    'ast rust: TypeScript + TSX grammars',
  );
  ok(
    languagesRs.includes('fn has_ast_grammar')
      && languagesRs.includes('"java"')
      && languagesRs.includes('"python"')
      && languagesRs.includes('"typescript"')
      && languagesRs.includes('"yaml"'),
    'ast languages: has_ast_grammar covers MVP set',
  );
  ok(
    cargoToml.includes('tree-sitter')
      && cargoToml.includes('tree-sitter-java')
      && cargoToml.includes('tree-sitter-typescript'),
    'ast cargo: tree-sitter dependencies present',
  );

  // --- Frontend shell ---
  ok(indexHtml.includes('id="panel-structure"'), 'ast ui: panel-structure present');
  ok(indexHtml.includes('data-panel="structure"'), 'ast ui: toolstrip Structure button');
  ok(indexHtml.includes('data-action="panel-structure"'), 'ast ui: View menu Structure item');
  ok(indexHtml.includes('id="structure-tree"'), 'ast ui: structure tree container');
  ok(indexHtml.includes('id="structure-filter"'), 'ast ui: filter input');
  ok(
    indexHtml.includes('data-ast-mode="structure"') && indexHtml.includes('data-ast-mode="full"'),
    'ast ui: Structure / AST mode toggle',
  );
  ok(indexHtml.includes('Alt+7'), 'ast ui: Alt+7 shortcut advertised');

  // --- Frontend behavior ---
  const refreshBody = extractFunctionBody(appSrc, 'refreshStructurePanel');
  ok(!!refreshBody, 'ast ui: refreshStructurePanel present');
  ok(
    refreshBody.includes('/workspace/ast') && refreshBody.includes("method: 'POST'"),
    'ast ui: refresh posts to /workspace/ast',
  );
  ok(
    refreshBody.includes('content') && refreshBody.includes('getValue()'),
    'ast ui: refresh sends dirty editor buffer',
  );
  ok(
    refreshBody.includes("mode === 'full'") || refreshBody.includes("structureMode === 'full'"),
    'ast ui: refresh respects structure/full mode',
  );

  const scheduleBody = extractFunctionBody(appSrc, 'scheduleStructureRefresh');
  ok(!!scheduleBody, 'ast ui: scheduleStructureRefresh present');
  ok(
    scheduleBody.includes("activePanel !== 'structure'")
      || scheduleBody.includes('activePanel !== "structure"'),
    'ast ui: debounce refresh only while Structure panel open',
  );

  const clickBody = extractFunctionBody(appSrc, 'onStructureTreeClick');
  ok(!!clickBody, 'ast ui: onStructureTreeClick present');
  ok(clickBody.includes('openFileAt'), 'ast ui: node click navigates via openFileAt');

  const caretBody = extractFunctionBody(appSrc, 'highlightStructureUnderCaret');
  ok(!!caretBody, 'ast ui: highlightStructureUnderCaret present');
  ok(
    caretBody.includes('findDeepestAstNode') || appSrc.includes('findDeepestAstNode'),
    'ast ui: caret maps to deepest AST node',
  );

  const supportedBody = extractFunctionBody(appSrc, 'isAstSupportedPath');
  ok(!!supportedBody, 'ast ui: isAstSupportedPath present');
  ok(appSrc.includes('AST_LANG_EXTS'), 'ast ui: AST_LANG_EXTS language extension set');
  const langExtsMatch = appSrc.match(/const AST_LANG_EXTS = new Set\(\[([\s\S]*?)\]\)/);
  const langExtsBlock = langExtsMatch?.[1] || '';
  ok(!!langExtsBlock, 'ast ui: AST_LANG_EXTS Set literal found');
  for (const ext of ['java', 'py', 'ts', 'tsx', 'go', 'rs', 'cpp', 'json', 'yaml']) {
    ok(
      langExtsBlock.includes(`'${ext}'`) || langExtsBlock.includes(`"${ext}"`),
      `ast ui: AST_LANG_EXTS includes ${ext}`,
    );
  }

  ok(
    appSrc.includes("scheduleStructureRefresh()")
      && extractFunctionBody(appSrc, 'activateTab').includes('scheduleStructureRefresh'),
    'ast ui: tab activate refreshes Structure when open',
  );
  ok(
    appSrc.includes('onDidChangeModelContent')
      && appSrc.includes('scheduleStructureRefresh()'),
    'ast ui: editor edits schedule Structure refresh',
  );
  ok(
    appSrc.includes('scheduleStructureCaretHighlight'),
    'ast ui: cursor moves schedule caret highlight',
  );

  const switchBody = extractFunctionBody(appSrc, 'switchPanel');
  ok(
    switchBody.includes("'structure'") && switchBody.includes('refreshStructurePanel'),
    'ast ui: switchPanel(structure) loads tree',
  );
  ok(
    appSrc.includes("'panel-structure'") || appSrc.includes('"panel-structure"'),
    'ast ui: menu action panel-structure wired',
  );
  ok(
    appSrc.includes("Digit7") || appSrc.includes("key === '7'"),
    'ast ui: Alt+7 keyboard shortcut wired',
  );
}
