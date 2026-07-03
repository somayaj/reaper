#!/usr/bin/env node
/**
 * Editor language regression suite — run on every build.
 * Verifies JS bundles parse, ReaperLang initializes, and Monaco providers
 * (hover, completion, inline completion, go-to-definition) work for all languages.
 */
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const STATIC = path.join(ROOT, 'static');

const JS_FILES = [
  'reaper-lang-core.js',
  'monaco-languages.js',
  'app.js',
];

const LANG_PATH_FIXTURES = [
  ['src/App.java', 'java'],
  ['src/main.kt', 'kotlin'],
  ['build.gradle', 'groovy'],
  ['src/lib.rs', 'rust'],
  ['app.js', 'javascript'],
  ['app.ts', 'typescript'],
  ['app.py', 'python'],
  ['main.go', 'go'],
  ['Program.cs', 'csharp'],
  ['app.rb', 'ruby'],
  ['index.php', 'php'],
  ['main.swift', 'swift'],
  ['main.c', 'cpp'],
  ['main.cpp', 'cpp'],
  ['script.sh', 'shell'],
  ['query.sql', 'sql'],
  ['index.html', 'html'],
  ['style.css', 'css'],
  ['data.json', 'json'],
  ['README.md', 'markdown'],
  ['Makefile', 'makefile'],
  ['CMakeLists.txt', 'cmake'],
  ['Dockerfile', 'dockerfile'],
  ['config.yaml', 'yaml'],
  ['Cargo.toml', 'toml'],
  ['app.ini', 'ini'],
];

const SAMPLE_CONTENT = {
  java: 'public class App {\n  void run() {\n    System.out.println("hi");\n  }\n}\n',
  kotlin: 'class App {\n  fun run() { println("hi") }\n}\n',
  groovy: 'class App {\n  void run() { println "hi" }\n}\n',
  rust: 'fn main() {\n    println!("hi");\n}\n',
  javascript: 'function run() {\n  console.log("hi");\n}\n',
  typescript: 'function run(): void {\n  console.log("hi");\n}\n',
  python: 'def run():\n    print("hi")\n',
  go: 'package main\n\nfunc main() {\n\tprintln("hi")\n}\n',
  cpp: '#include <stdio.h>\n\nint main() {\n  printf("hi\\n");\n  return 0;\n}\n',
  sql: 'SELECT id FROM users WHERE name = $1;\n',
  plaintext: 'hello world\n',
};

let passed = 0;
let failed = 0;

function ok(cond, msg) {
  if (cond) {
    passed += 1;
    return;
  }
  failed += 1;
  console.error(`  FAIL: ${msg}`);
}

function section(title) {
  console.log(`\n== ${title} ==`);
}

function nodeBinary() {
  if (process.execPath && fs.existsSync(process.execPath)) return process.execPath;
  return 'node';
}

function syntaxCheck(file) {
  const abs = path.join(STATIC, file);
  const r = spawnSync(nodeBinary(), ['--check', abs], { encoding: 'utf8' });
  ok(r.status === 0, `${file} syntax (${(r.stderr || r.stdout || '').trim()})`);
}

function scanForMalformedIf(file) {
  const src = fs.readFileSync(path.join(STATIC, file), 'utf8');
  const lines = src.split('\n');
  const bad = [];
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i];
    if (/^\s*if\s+[a-zA-Z_$][\w$]*\s*\)/.test(line)) {
      bad.push(`${file}:${i + 1}: ${line.trim()}`);
    }
  }
  ok(bad.length === 0, bad.length ? `malformed if statements:\n    ${bad.join('\n    ')}` : `${file} if-scan clean`);
}

function selectorMatches(selector, langId) {
  if (!selector) return true;
  if (typeof selector === 'string') return selector === langId;
  if (Array.isArray(selector)) return selector.includes(langId);
  if (typeof selector === 'object' && selector.language) return selector.language === langId;
  return true;
}

function findProvider(providers, langId) {
  return providers.find((p) => selectorMatches(p.selector, langId));
}

function createMockMonaco() {
  const providers = {
    hover: [],
    completion: [],
    definition: [],
    inline: [],
    editorOpener: null,
  };

  const monaco = {
    Uri: {
      parse(s) {
        const m = String(s).match(/^([^:/?#]+):\/\/([^/?#]*)([^?#]*)?/);
        return {
          scheme: m?.[1] || '',
          authority: m?.[2] || '',
          path: m?.[3] || '',
          toString: () => s,
        };
      },
      from(o) {
        return { ...o, toString: () => `${o.scheme}://${o.authority}${o.path || ''}` };
      },
    },
    Range: class {
      constructor(startLineNumber, startColumn, endLineNumber, endColumn) {
        this.startLineNumber = startLineNumber;
        this.startColumn = startColumn;
        this.endLineNumber = endLineNumber;
        this.endColumn = endColumn;
      }
    },
    languages: {
      CompletionItemKind: new Proxy({}, { get: (_, k) => k }),
      SymbolKind: new Proxy({}, { get: (_, k) => k }),
      CompletionTriggerKind: {
        Invoke: 0,
        TriggerCharacter: 1,
        TriggerForIncompleteCompletions: 2,
      },
      register: () => ({ dispose: () => {} }),
      setMonarchTokensProvider: () => {},
      setLanguageConfiguration: () => {},
      registerHoverProvider(selector, provider) {
        providers.hover.push({ selector, provider });
        return { dispose: () => {} };
      },
      registerCompletionItemProvider(selector, provider) {
        providers.completion.push({ selector, provider });
        return { dispose: () => {} };
      },
      registerDefinitionProvider(selector, provider) {
        providers.definition.push({ selector, provider });
        return { dispose: () => {} };
      },
      registerInlineCompletionsProvider(selector, provider) {
        providers.inline.push({ selector, provider });
        return { dispose: () => {} };
      },
      registerDocumentSymbolProvider: () => ({ dispose: () => {} }),
      registerCodeActionProvider: () => ({ dispose: () => {} }),
      registerDocumentFormattingEditProvider: () => ({ dispose: () => {} }),
      registerOnTypeFormattingEditProvider: () => ({ dispose: () => {} }),
      CodeActionKind: { QuickFix: 'quickfix', Empty: '' },
    },
    editor: {
      registerEditorOpener(handler) {
        providers.editorOpener = handler;
        return { dispose: () => {} };
      },
      getModelMarkers: () => [],
      setModelLanguage: () => {},
      ShowLightbulbIconMode: { Off: 0 },
      EditorOption: { lineHeight: 18 },
      MouseTargetType: { GUTTER_GLYPH_MARGIN: 1, OVERVIEW_RULER: 2 },
    },
    KeyCode: new Proxy({}, { get: (_, k) => k }),
    KeyMod: new Proxy({}, { get: (_, k) => 1 }),
    MarkerSeverity: { Error: 8, Warning: 4 },
  };

  return { monaco, providers };
}

function createMockModel(langId, content, filePath) {
  const lines = content.split('\n');
  return {
    uri: { scheme: 'inmemory', authority: 'model', path: '/1' },
    getLanguageId: () => langId,
    getValue: () => content,
    getLineContent: (n) => lines[n - 1] || '',
    getLineCount: () => lines.length,
    getWordAtPosition: (pos) => {
      const line = lines[pos.lineNumber - 1] || '';
      const col = Math.max(0, pos.column - 1);
      if (col >= line.length || !/\w/.test(line[col])) return null;
      let start = col;
      let end = col + 1;
      while (start > 0 && /\w/.test(line[start - 1])) start -= 1;
      while (end < line.length && /\w/.test(line[end])) end += 1;
      return { word: line.slice(start, end), startColumn: start + 1, endColumn: end + 1 };
    },
    getWordUntilPosition: (pos) => {
      const w = createMockModel(langId, content, filePath).getWordAtPosition(pos);
      return w ? { word: w.word, startColumn: w.startColumn, endColumn: w.endColumn } : { word: '', startColumn: pos.column, endColumn: pos.column };
    },
    getValueInRange: () => '',
    getFullModelRange: () => new (createMockMonaco().monaco.Range)(1, 1, lines.length, 1),
    _path: filePath,
  };
}

function loadEditorBundles(monaco) {
  const sandbox = {
    monaco,
    window: { ReaperLang: {} },
    URLSearchParams,
    document: {
      querySelector: () => ({ content: { trim: () => fs.readFileSync(path.join(STATIC, 'BUILD'), 'utf8').trim() } }),
      getElementById: () => null,
      addEventListener: () => {},
      removeEventListener: () => {},
    },
    localStorage: { getItem: () => null, setItem: () => {} },
    require: undefined,
    console,
  };

  vm.createContext(sandbox);
  for (const file of ['reaper-lang-core.js', 'monaco-languages.js']) {
    const src = fs.readFileSync(path.join(STATIC, file), 'utf8');
    vm.runInContext(src, sandbox, { filename: path.join(STATIC, file) });
  }
  return sandbox.window;
}

async function runProviderTests(monaco, providers, helpersState, editor) {
  ok(providers.hover.length >= 1, 'hover providers registered');
  ok(providers.completion.length >= 1, 'completion providers registered');
  ok(providers.definition.length >= 1, 'definition providers registered');
  ok(providers.inline.length >= 1, 'inline completion providers registered');
  ok(providers.editorOpener?.openCodeEditor, 'editor opener registered');

  const invokeCtx = { triggerKind: 0, triggerCharacter: undefined };
  const cancelToken = { isCancellationRequested: false };

  for (const [filePath, langId] of LANG_PATH_FIXTURES) {
    const content = SAMPLE_CONTENT[langId] || `// ${langId} sample\n`;
    const model = createMockModel(langId, content, filePath);
    const position = langId === 'java'
      ? { lineNumber: 3, column: 18 }
      : { lineNumber: 1, column: Math.min(8, content.length + 1) };

    helpersState.activePath = filePath;
    editor.getModel = () => model;
    editor.getPosition = () => position;

    const hoverP = findProvider(providers.hover, langId);
    ok(!!hoverP, `${langId}: hover provider registered`);

    if (hoverP) {
      try {
        const hover = await Promise.resolve(hoverP.provider.provideHover(model, position));
        ok(hover === null || (hover.contents && hover.contents.length > 0), `${langId}: hover returns null or content`);
      } catch (e) {
        ok(false, `${langId}: hover threw: ${e.message}`);
      }
    }

    const defP = findProvider(providers.definition, langId);
    if (defP) {
      try {
        const loc = await Promise.resolve(defP.provider.provideDefinition(model, position));
        if (langId === 'java') {
          ok(!!loc?.uri, 'java: definition returns location');
          if (loc?.uri) ok(loc.uri.scheme === 'reaper', 'java: definition uses reaper:// uri');
        } else {
          ok(loc === null || loc?.uri, `${langId}: definition returns null or location`);
        }
      } catch (e) {
        ok(false, `${langId}: definition threw: ${e.message}`);
      }
    }

    const compP = findProvider(providers.completion, langId);
    if (compP) {
      try {
        const result = await Promise.resolve(
          compP.provider.provideCompletionItems(model, position, invokeCtx),
        );
        ok(result && Array.isArray(result.suggestions), `${langId}: completion returns suggestions array`);
      } catch (e) {
        ok(false, `${langId}: completion threw: ${e.message}`);
      }
    }

    const inlineP = findProvider(providers.inline, langId);
    if (inlineP) {
      try {
        const result = await Promise.resolve(
          inlineP.provider.provideInlineCompletions(model, position, {}, cancelToken),
        );
        ok(result && Array.isArray(result.items), `${langId}: inline completion returns items array`);
      } catch (e) {
        ok(false, `${langId}: inline completion threw: ${e.message}`);
      }
    }
  }

  // Editor opener round-trip for navigation
  const testUri = monaco.Uri.parse('reaper://workspace/src%2FTarget.java');
  let opened = false;
  helpersState.openFileAt = async (p, line, col) => {
    opened = true;
    ok(p === 'src/Target.java', 'editor opener decodes path');
    ok(line === 10 && col === 5, 'editor opener passes line/column');
  };
  const handled = providers.editorOpener.openCodeEditor(
    editor,
    testUri,
    { startLineNumber: 10, startColumn: 5 },
  );
  ok(handled === true && opened, 'editor opener handles reaper:// navigation');
}

function extractFunctionSource(src, name) {
  const start = src.indexOf(`function ${name}`);
  if (start < 0) return null;
  let depth = 0;
  let started = false;
  for (let i = start; i < src.length; i += 1) {
    const ch = src[i];
    if (ch === '{') {
      depth += 1;
      started = true;
    } else if (ch === '}') {
      depth -= 1;
      if (started && depth === 0) return src.slice(start, i + 1);
    }
  }
  return null;
}

function testRunButtonUiRegression() {
  const css = fs.readFileSync(path.join(STATIC, 'reaper-ui.css'), 'utf8');
  const appJs = fs.readFileSync(path.join(STATIC, 'app.js'), 'utf8');
  const indexHtml = fs.readFileSync(path.join(STATIC, 'index.html'), 'utf8');

  ok(/(--ij-toolbar-btn-size:\s*calc\(var\(--ij-ui-font-size\)[^;]+;)/.test(css), 'toolbar button size scales with --ij-ui-font-size');
  ok(/(--ij-toolbar-run-icon-size:\s*calc\(var\(--ij-ui-font-size\)[^;]+;)/.test(css), 'toolbar run icon size scales with --ij-ui-font-size');
  ok(/(--ij-editor-line-height:\s*calc\(var\(--ij-ui-font-size\)[^;]+;)/.test(css), 'editor line height scales with --ij-ui-font-size');
  ok(/(--ij-gutter-run-icon-size:\s*calc\(var\(--ij-ui-font-size\)[^;]+;)/.test(css), 'gutter run icon size scales with --ij-ui-font-size');

  ok(/\.ij-toolbar-btn\.run\s*\{[\s\S]*?color:\s*var\(--ij-run\)/.test(css), 'toolbar run button uses theme run color');
  ok(/\.ij-toolbar-btn\.run svg[\s\S]*?var\(--ij-toolbar-run-icon-size/.test(css), 'toolbar run svg uses scaled icon size');
  ok(/\.monaco-editor \.ij-gutter-run-btn[\s\S]*?color:\s*var\(--ij-run\)/.test(css), 'gutter run button uses theme run color');
  ok(/\.monaco-editor \.ij-gutter-run-btn[\s\S]*?border-radius:\s*0/.test(css), 'gutter run button is icon-only (no circular chrome)');
  ok(/\.monaco-editor \.ij-gutter-run-btn \.ij-gutter-run-icon[\s\S]*?var\(--ij-gutter-run-icon-size/.test(css), 'gutter run icon uses scaled size var');
  ok(!/\.monaco-editor \.ij-test-run-widget[\s\S]*?linear-gradient/.test(css), 'gutter run widgets avoid gradient fills');
  ok(!/\.monaco-editor \.ij-spring-run-widget[\s\S]*?linear-gradient/.test(css), 'spring app gutter run avoids gradient fills');

  ok(/id="tb-run"[\s\S]*?class="[^"]*\bij-toolbar-btn run\b[^"]*"/.test(indexHtml), 'toolbar run button uses ij-toolbar-btn run classes');
  ok(/id="tb-run"[\s\S]*?ij-toolbar-run-icon/.test(indexHtml), 'toolbar run svg uses ij-toolbar-run-icon class');

  for (const sym of ['gutterRunPlayIconHtml', 'findNativeMainLine', 'createNativeRunWidget']) {
    ok(appJs.includes(`function ${sym}`), `app.js defines ${sym}`);
  }
  ok(/function gutterRunPlayIconHtml\(\)[\s\S]*?fill="currentColor"/.test(appJs), 'gutter play icon uses currentColor fill');
  ok(/function applyTestRunDecorations\(\)[\s\S]*?isNativeSourcePath\(path\)/.test(appJs), 'gutter run decorations include native sources');

  const findNativeMainLineSrc = extractFunctionSource(appJs, 'findNativeMainLine');
  ok(!!findNativeMainLineSrc, 'extract findNativeMainLine from app.js');
  if (findNativeMainLineSrc) {
    const findNativeMainLine = vm.runInNewContext(`${findNativeMainLineSrc}; findNativeMainLine;`);
    ok(findNativeMainLine(SAMPLE_CONTENT.cpp) === 3, 'findNativeMainLine locates C++ main');
    ok(findNativeMainLine(SAMPLE_CONTENT.rust) === 1, 'findNativeMainLine locates Rust main');
    ok(findNativeMainLine('// int main()\n') === -1, 'findNativeMainLine ignores commented main');
  }
}

async function main() {
  console.log('Reaper editor regression suite');
  console.log(`Root: ${ROOT}`);

  section('JavaScript syntax');
  for (const file of JS_FILES) syntaxCheck(file);

  section('Static scans');
  scanForMalformedIf('monaco-languages.js');
  scanForMalformedIf('app.js');

  section('Bundle load');
  const { monaco, providers } = createMockMonaco();
  const win = loadEditorBundles(monaco);
  ok(win.__reaperLangBundleLoaded === true, 'monaco-languages.js sets __reaperLangBundleLoaded');
  ok(typeof win.ReaperLang?.setupEditorFeatures === 'function', 'setupEditorFeatures exported');
  ok(typeof win.ReaperLang?.langForPath === 'function', 'langForPath exported');

  section('Language detection');
  for (const [filePath, expected] of LANG_PATH_FIXTURES) {
    ok(win.ReaperLang.langForPath(filePath) === expected, `langForPath(${filePath}) === ${expected}`);
  }

  section('setupEditorFeatures + providers');
  const editor = {
    getModel: () => null,
    getPosition: () => ({ lineNumber: 1, column: 1 }),
    onDidChangeModelContent: () => ({ dispose: () => {} }),
    onDidChangeCursorPosition: () => ({ dispose: () => {} }),
    onDidBlurEditorWidget: () => ({ dispose: () => {} }),
    onKeyDown: () => ({ dispose: () => {} }),
    addAction: () => {},
    getAction: () => null,
    trigger: () => {},
    deltaDecorations: () => [],
    revealLineInCenter: () => {},
    setPosition: () => {},
    focus: () => {},
    getOption: () => 18,
    _contextKeyService: { getContextKeyValue: () => false },
  };

  const helpersState = {
    openFileAt: async () => {},
    activePath: 'src/App.java',
  };

  const helpers = {
    getRepo: () => 'testrepo',
    getActivePath: () => helpersState.activePath,
    getEditor: () => editor,
    repoApi: (_r, p) => `/api/repos/testrepo${p}`,
    api: async (url) => {
      if (url.includes('/workspace/definition')) {
        return { path: 'src/Other.java', line: 10, column: 5, name: 'other' };
      }
      if (url.includes('/workspace/hover')) {
        return { name: 'Symbol', kind: 'method', signature: 'void symbol()', documentation: 'doc' };
      }
      if (url.includes('/workspace/completions')) {
        return [{ label: 'println', kind: 'method', detail: 'println(String)' }];
      }
      return {};
    },
    openFileAt: (...args) => helpersState.openFileAt(...args),
    isFileDirty: () => false,
    getJavaLanguageLevel: () => 17,
    getLanguageContext: () => ({}),
    getAiInlineComplete: () => false,
    getGeminiConfigured: () => false,
    toast: () => {},
    setCompleteDebugStatus: () => {},
    setStatusMessage: () => {},
    scheduleDiagnostics: () => {},
    getDiagnosticsInRange: () => [],
    diagnosticSpan: () => ({}),
    diagnosticFriendlyHint: () => '',
    setLanguageStatus: () => {},
    getDbSchema: () => null,
    getJavaSourceOverlays: () => [],
    hideQuickFixMenu: () => {},
    showQuickFixMenu: () => {},
    isQuickFixMenuOpen: () => false,
    terminalLog: () => {},
  };

  let activeHelpers = null;
  try {
    activeHelpers = win.ReaperLang.setupEditorFeatures(editor, helpers);
    ok(!!activeHelpers?.getActivePath, 'setupEditorFeatures returns helpers');
    ok(true, 'setupEditorFeatures completed');
  } catch (e) {
    ok(false, `setupEditorFeatures threw: ${e.stack || e.message}`);
  }

  await runProviderTests(monaco, providers, helpersState, editor);

  section('Run button UI scaling');
  testRunButtonUiRegression();

  section('Summary');
  console.log(`  ${passed} passed, ${failed} failed`);
  if (failed > 0) {
    process.exitCode = 1;
    console.error('\nEditor regression suite FAILED');
  } else {
    console.log('\nEditor regression suite OK');
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
