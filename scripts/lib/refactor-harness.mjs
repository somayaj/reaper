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

export function testRefactorRegression(appSrc, langSrc, modRsSrc, symbolsRsSrc, ok) {
  ok(appSrc.includes('runEditorMonacoAction'), 'refactor: editor action runner in app.js');
  ok(appSrc.includes("'find-usages'"), 'refactor: find usages in command palette');
  ok(appSrc.includes("'rename-symbol'"), 'refactor: rename in command palette');
  ok(appSrc.includes("'change-all'"), 'refactor: change all in command palette');

  ok(langSrc.includes('reaper.changeAllOccurrences'), 'refactor: change all monaco action');
  ok(langSrc.includes('editor.action.changeAll'), 'refactor: delegates to monaco changeAll');
  ok(langSrc.includes('reaper.findUsages'), 'refactor: find usages action');
  ok(langSrc.includes('reaper.renameSymbol'), 'refactor: rename action');

  ok(modRsSrc.includes('prepare_rename_word_fallback'), 'refactor: text rename prepare fallback');
  ok(modRsSrc.includes('rename_word_fallback'), 'refactor: text rename fallback');
  ok(modRsSrc.includes('is_java_source_path'), 'refactor: jdtls limited to .java');
  ok(modRsSrc.includes('merge_reference_locations(refs, fallback_refs)'), 'refactor: C/Ruby refs merge fallback');

  ok(symbolsRsSrc.includes('word_range_at'), 'refactor: word range helper');
  ok(symbolsRsSrc.includes('format_trim_trailing_whitespace'), 'refactor: universal format fallback');
  ok(symbolsRsSrc.includes('clang-format') || symbolsRsSrc.includes('clang'), 'refactor: C/C++ formatter');
}
