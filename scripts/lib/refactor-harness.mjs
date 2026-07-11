/**
 * Refactoring regression — find usages, rename, change all, format across languages.
 */

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

function extractRustFnBody(src, name) {
  const block = src.match(new RegExp(`pub fn ${name}[\\s\\S]*?(?=\\npub fn )`))?.[0] || '';
  const brace = block.indexOf('{');
  if (brace === -1) return '';
  let depth = 1;
  for (let i = brace + 1; i < block.length; i += 1) {
    const ch = block[i];
    if (ch === '{') depth += 1;
    else if (ch === '}') {
      depth -= 1;
      if (depth === 0) return block.slice(brace + 1, i);
    }
  }
  return '';
}

export function testRefactorRegression(
  appSrc,
  langSrc,
  modRsSrc,
  symbolsRsSrc,
  ok,
  jdtlsRsSrc = '',
) {
  ok(appSrc.includes('runEditorMonacoAction'), 'refactor: editor action runner in app.js');
  ok(appSrc.includes("'find-usages'"), 'refactor: find usages in command palette');
  ok(appSrc.includes("'rename-symbol'"), 'refactor: rename in command palette');
  ok(appSrc.includes("'java-refactor'"), 'refactor: Java Refactor… in command palette');
  ok(appSrc.includes("'change-all'"), 'refactor: change all in command palette');

  ok(langSrc.includes('reaper.changeAllOccurrences'), 'refactor: change all monaco action');
  ok(langSrc.includes('editor.action.changeAll'), 'refactor: delegates to monaco changeAll');
  const changeAllBlock = langSrc.match(
    /id: 'reaper\.changeAllOccurrences'[\s\S]*?run: \(\) => editor\.getAction\('editor\.action\.changeAll'\)/,
  )?.[0] || '';
  ok(
    !changeAllBlock.includes('contextMenuGroupId'),
    'refactor: change all not duplicated in context menu (uses monaco built-in)',
  );
  ok(langSrc.includes('reaper.findUsages'), 'refactor: find usages action');
  ok(langSrc.includes('helpers.promptRename'), 'refactor: symbol rename uses in-app prompt');

  ok(modRsSrc.includes('prepare_rename_word_fallback'), 'refactor: text rename prepare fallback');
  ok(modRsSrc.includes('rename_word_fallback'), 'refactor: text rename fallback');
  ok(modRsSrc.includes('is_java_source_path'), 'refactor: jdtls limited to .java');
  ok(modRsSrc.includes('jdtls::workspace_ready'), 'refactor: rename skips cold jdtls');
  ok(
    modRsSrc.includes('workspace_prepare_rename_java_returns_instant_word_range'),
    'refactor: rust test for instant java prepare rename',
  );
  ok(
    modRsSrc.includes('workspace_rename_java_uses_text_fallback_when_jdtls_cold'),
    'refactor: rust test for java text rename when jdtls cold',
  );

  const prepareRenameFn = modRsSrc.match(
    /pub fn workspace_prepare_rename[\s\S]*?(?=\npub fn )/,
  )?.[0] || '';
  ok(
    prepareRenameFn.includes('prepare_rename_word_fallback')
      && prepareRenameFn.indexOf('prepare_rename_word_fallback')
        < (prepareRenameFn.indexOf('jdtls::prepare_rename') === -1
          ? Infinity
          : prepareRenameFn.indexOf('jdtls::prepare_rename')),
    'refactor: prepare rename uses word fallback before jdtls',
  );
  ok(
    prepareRenameFn.includes('jdtls::workspace_ready'),
    'refactor: prepare rename only calls jdtls when session warm',
  );

  const renameFn = extractRustFnBody(modRsSrc, 'workspace_rename');
  ok(
    renameFn.includes('jdtls::rename_symbol')
      && renameFn.includes('rename_word_fallback')
      && renameFn.indexOf('jdtls::rename_symbol')
        < renameFn.indexOf('rename_word_fallback'),
    'refactor: workspace_rename prefers jdtls when ready before text fallback',
  );
  ok(renameFn.includes('java_class_file_rename_candidate'), 'refactor: java class rename includes file rename');
  ok(renameFn.includes('WorkspaceRenameResult'), 'refactor: workspace_rename returns path rename metadata');

  ok(langSrc.includes('reaper.javaRefactor'), 'refactor: Monaco Java Refactor action');
  ok(langSrc.includes('runJavaRefactor'), 'refactor: runJavaRefactor helper');
  ok(
    langSrc.includes('refactor.extract')
      && langSrc.includes('/workspace/java/code-actions'),
    'refactor: Java refactor requests jdtls refactor/source kinds',
  );
  ok(
    langSrc.includes('applyJavaWorkspaceEdits')
      && extractFunctionBody(langSrc, 'runJavaRefactor').includes('applyJavaWorkspaceEdits'),
    'refactor: Java refactor applies workspace edits',
  );
  ok(
    extractFunctionBody(langSrc, 'runJavaRefactor').includes('getScrolledVisiblePosition'),
    'refactor: Java refactor picker anchors near cursor',
  );
  ok(
    langSrc.includes("contextMenuGroupId: '1_modification'"),
    'refactor: Java Refactor uses Monaco 1_modification context group',
  );
  ok(
    appSrc.includes('showRefactorStaircaseMenu')
      && (appSrc.includes('ij-cascade-step')
        || appSrc.includes('ij-quickfix-heading')
        || appSrc.includes('refactorMenuCategory')),
    'refactor: staircase cascading refactor picker',
  );

  ok(langSrc.includes('RENAME_FETCH_TIMEOUT_MS'), 'refactor: rename fetch timeout');
  ok(langSrc.includes('DEFINITION_FETCH_TIMEOUT_MS'), 'refactor: definition fetch timeout');
  ok(langSrc.includes('raceWithTimeout'), 'refactor: timeout helper works without setTimeout in tests');
  const postLspBody = extractFunctionBody(langSrc, 'postWorkspaceLsp');
  ok(postLspBody.includes('raceWithTimeout'), 'refactor: postWorkspaceLsp supports timeout race');
  ok(postLspBody.includes('timeoutMs'), 'refactor: postWorkspaceLsp accepts timeoutMs');

  const runRenameBody = extractFunctionBody(langSrc, 'runRename');
  ok(
    !runRenameBody.includes("'/workspace/prepare-rename'"),
    'refactor: rename skips prepare-rename round trip',
  );
  ok(
    runRenameBody.includes("'/workspace/rename'")
      && runRenameBody.includes('RENAME_FETCH_TIMEOUT_MS'),
    'refactor: rename apply uses fetch timeout',
  );
  ok(
    runRenameBody.includes('path_rename')
      && runRenameBody.includes('renameWorkspacePath'),
    'refactor: symbol rename applies file rename when backend requests it',
  );
  ok(
    runRenameBody.includes('Renaming…'),
    'refactor: rename spinner label is Renaming',
  );

  ok(
    modRsSrc.includes('jdtls::workspace_ready(ws)')
      && /find_definition_with_content[\s\S]*?jdtls::workspace_ready/.test(modRsSrc),
    'refactor: definition skips cold jdtls',
  );

  if (jdtlsRsSrc) {
    ok(jdtlsRsSrc.includes('jdtls rename failed'), 'refactor: jdtls rename errors fall through');
    const lspRequestFn = jdtlsRsSrc.match(
      /fn lsp_request\([\s\S]*?^fn /m,
    )?.[0] || '';
    ok(
      lspRequestFn.includes('wait_for_id(stdout, id, deadline')
        && !lspRequestFn.includes('Instant::now() + QUERY_TIMEOUT'),
      'refactor: jdtls lsp_request honors caller deadline',
    );
    ok(jdtlsRsSrc.includes('pub fn workspace_ready'), 'refactor: jdtls workspace_ready exported');
  }

  ok(modRsSrc.includes('merge_reference_locations_dedupes_by_path_line_column'), 'refactor: reference merge helper tested');
  const refsFn = extractRustFnBody(modRsSrc, 'workspace_references');
  ok(
    refsFn.includes('find_word_references_fallback')
      && refsFn.includes('if !fallback_refs.is_empty()'),
    'refactor: find usages returns text fallback without waiting on jdtls',
  );
  ok(
    refsFn.includes('jdtls::workspace_ready'),
    'refactor: find usages only calls jdtls when text fallback is empty',
  );
  ok(modRsSrc.includes('workspace_references_java_uses_text_fallback_when_jdtls_cold'), 'refactor: rust test for java find usages fallback');
  ok(modRsSrc.includes('rename_path_symbol_plan'), 'refactor: file tree rename plans symbol edits');
  ok(modRsSrc.includes('java_file_tree_symbol_edits'), 'refactor: java file rename updates class references');
  ok(langSrc.includes('REFERENCES_FETCH_TIMEOUT_MS'), 'refactor: find usages fetch timeout');
  const lookupRefsBody = extractFunctionBody(langSrc, 'lookupReferences');
  ok(lookupRefsBody.includes('REFERENCES_FETCH_TIMEOUT_MS'), 'refactor: lookupReferences uses timeout');

  ok(symbolsRsSrc.includes('word_range_at'), 'refactor: word range helper');
  ok(symbolsRsSrc.includes('format_trim_trailing_whitespace'), 'refactor: universal format fallback');
  ok(symbolsRsSrc.includes('clang-format') || symbolsRsSrc.includes('clang'), 'refactor: C/C++ formatter');
}
