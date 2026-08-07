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

/** Parsed from monaco-languages.js so provider coverage stays in sync with the editor. */
function extractAllEditorLangs() {
  const src = fs.readFileSync(path.join(STATIC, 'monaco-languages.js'), 'utf8');
  const m = src.match(/const ALL_EDITOR_LANGS = \[([\s\S]*?)\];/);
  if (!m) throw new Error('ALL_EDITOR_LANGS not found in monaco-languages.js');
  return [...m[1].matchAll(/'([^']+)'/g)].map((match) => match[1]);
}

const ALL_EDITOR_LANGS = extractAllEditorLangs();

/** One path per language detectable via langForPath (main.c maps to cpp, not c). */
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
  ['script.lua', 'lua'],
  ['main.dart', 'dart'],
  ['analysis.r', 'r'],
  ['query.sql', 'sql'],
  ['index.html', 'html'],
  ['style.css', 'css'],
  ['styles.scss', 'scss'],
  ['styles.less', 'less'],
  ['data.json', 'json'],
  ['README.md', 'markdown'],
  ['pom.xml', 'xml'],
  ['config.yaml', 'yaml'],
  ['Cargo.toml', 'toml'],
  ['app.ini', 'ini'],
  ['Makefile', 'makefile'],
  ['CMakeLists.txt', 'cmake'],
  ['Dockerfile', 'dockerfile'],
  ['api.proto', 'protobuf'],
  ['schema.graphql', 'graphql'],
  ['elide.pkl', 'pkl'],
  ['notes.txt', 'plaintext'],
];

/** One fixture per Monaco lang id — includes c (path detection uses cpp for .c files). */
function buildProviderFixtures() {
  const pathByLang = new Map();
  for (const [filePath, langId] of LANG_PATH_FIXTURES) {
    if (!pathByLang.has(langId)) pathByLang.set(langId, filePath);
  }
  pathByLang.set('c', 'main.c');
  return ALL_EDITOR_LANGS.map((langId) => {
    const filePath = pathByLang.get(langId);
    return [filePath, langId];
  });
}

const PROVIDER_LANG_FIXTURES = buildProviderFixtures();

const SAMPLE_CONTENT = {
  java: 'public class App {\n  void run() {\n    System.out.println("hi");\n  }\n}\n',
  kotlin: 'class App {\n  fun run() { println("hi") }\n}\n',
  groovy: 'class App {\n  void run() { println "hi" }\n}\n',
  rust: 'fn main() {\n    println!("hi");\n}\n',
  javascript: 'function run() {\n  console.log("hi");\n}\n',
  typescript: 'function run(): void {\n  console.log("hi");\n}\n',
  python: 'def run():\n    print("hi")\n',
  go: 'package main\n\nfunc main() {\n\tprintln("hi")\n}\n',
  csharp: 'using System;\n\nclass Program {\n  static void Main() {\n    Console.WriteLine("hi");\n  }\n}\n',
  ruby: 'def run\n  puts "hi"\nend\n',
  php: '<?php\nfunction run() {\n  echo "hi";\n}\n',
  swift: 'func run() {\n  print("hi")\n}\n',
  c: '#include <stdio.h>\n\nint main() {\n  return 0;\n}\n',
  cpp: '#include <stdio.h>\n\nint main() {\n  printf("hi\\n");\n  return 0;\n}\n',
  shell: '#!/bin/bash\necho "hi"\n',
  lua: 'function run()\n  print("hi")\nend\n',
  dart: 'void main() {\n  print("hi");\n}\n',
  r: 'run <- function() {\n  print("hi")\n}\n',
  sql: 'SELECT id FROM users WHERE name = $1;\n',
  html: '<!DOCTYPE html><html><body>hi</body></html>\n',
  css: '.app { color: red; }\n',
  scss: '.app { color: red; }\n',
  less: '.app { color: red; }\n',
  json: '{"hello":"world"}\n',
  markdown: '# Hello\n',
  xml: '<?xml version="1.0"?><root/>\n',
  yaml: 'hello: world\n',
  toml: 'hello = "world"\n',
  ini: 'hello=world\n',
  makefile: 'all:\n\techo hi\n',
  cmake: 'cmake_minimum_required(VERSION 3.10)\n',
  dockerfile: 'FROM alpine\n',
  protobuf: 'syntax = "proto3";\nmessage Hello {}\n',
  graphql: 'type Query { hello: String }\n',
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
    globalThis: null,
    // No-op timer: provider tests must not fire timeouts synchronously (breaks raceWithTimeout).
    setTimeout: (_fn, _ms = 0) => 1,
    clearTimeout: () => {},
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
  sandbox.globalThis = sandbox;

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

  for (const [filePath, langId] of PROVIDER_LANG_FIXTURES) {
    if (!filePath) {
      ok(false, `${langId}: missing provider fixture path`);
      continue;
    }
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
  ok(/(--ij-gutter-cov-icon-size:\s*calc\(var\(--ij-ui-font-size\)[^;]+;)/.test(css), 'gutter coverage icon size scales with --ij-ui-font-size');

  ok(/\.ij-toolbar-btn\.run\s*\{[\s\S]*?color:\s*var\(--ij-run\)/.test(css), 'toolbar run button uses theme run color');
  ok(/\.ij-toolbar-btn\.run svg[\s\S]*?var\(--ij-toolbar-run-icon-size/.test(css), 'toolbar run svg uses scaled icon size');
  ok(/\.monaco-editor \.ij-gutter-run-btn[\s\S]*?color:\s*var\(--ij-run\)/.test(css), 'gutter run button uses theme run color');
  ok(/\.monaco-editor \.ij-gutter-run-btn[\s\S]*?border-radius:\s*0/.test(css), 'gutter run button is icon-only (no circular chrome)');
  ok(/\.monaco-editor \.ij-gutter-run-btn \.ij-gutter-run-icon[\s\S]*?var\(--ij-gutter-run-icon-size/.test(css), 'gutter run icon uses scaled size var');
  ok(!/\.monaco-editor \.ij-test-run-widget[\s\S]*?linear-gradient/.test(css), 'gutter run widgets avoid gradient fills');
  ok(!/\.monaco-editor \.ij-spring-run-widget[\s\S]*?linear-gradient/.test(css), 'spring app gutter run avoids gradient fills');

  ok(/id="tb-run"[\s\S]*?class="[^"]*\bij-toolbar-btn run\b[^"]*"/.test(indexHtml), 'toolbar run button uses ij-toolbar-btn run classes');
  ok(/id="tb-run"[\s\S]*?ij-toolbar-run-icon/.test(indexHtml), 'toolbar run svg uses ij-toolbar-run-icon class');

  for (const sym of ['gutterRunPlayIconHtml', 'gutterCoverageIconHtml', 'findNativeMainLine', 'createNativeRunWidget']) {
    ok(appJs.includes(`function ${sym}`), `app.js defines ${sym}`);
  }
  ok(/function gutterRunPlayIconHtml\(\)[\s\S]*?fill="currentColor"/.test(appJs), 'gutter play icon uses currentColor fill');
  ok(/function gutterCoverageIconHtml\(\)[\s\S]*?>©<\/text>/.test(appJs), 'gutter coverage icon uses copyright-style ©');
  ok(/\.monaco-editor \.ij-gutter-cov-btn[\s\S]*?var\(--ij-gutter-cov-icon-size/.test(css), 'gutter coverage button uses scaled icon size var');
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

function testTestCoverageGutterRegression(appSrc) {
  const applyBody = extractFunctionBody(appSrc, 'applyTestRunDecorations');
  ok(
    applyBody.includes('createTestCoverageWidget(method)')
      && applyBody.includes('testCovWidgets.push(covWidget)'),
    'editor: applyTestRunDecorations adds coverage © gutter widgets on Java test files',
  );
  ok(
    applyBody.includes('projectSupportsCoverage()'),
    'editor: coverage gutter widgets gated on open repo/project',
  );

  const covWidgetSrc = extractFunctionSource(appSrc, 'createTestCoverageWidget');
  ok(!!covWidgetSrc, 'extract createTestCoverageWidget from app.js');
  ok(
    covWidgetSrc.includes('GlyphMarginLane?.Left'),
    'editor: coverage © widget uses left glyph lane (run ▶ stays on right)',
  );
  ok(
    covWidgetSrc.includes('runProjectTestWithCoverage(method.filter)'),
    'editor: coverage gutter click runs tests with JaCoCo',
  );
  ok(
    covWidgetSrc.includes('ij-test-cov-widget') && covWidgetSrc.includes('gutterCoverageIconHtml()'),
    'editor: coverage gutter uses © icon button styling',
  );
}

function testInlineCompletionRegression(win) {
  const h = win.ReaperLang.inlineTestHelpers?.();
  ok(typeof h?.memberInlineSuffix === 'function', 'inlineTestHelpers exported');

  const javaPath = 'src/App.java';
  const systemOutContent = 'public class App {\n  void run() {\n    System.out.\n  }\n}\n';
  const systemOutSuffix = h.memberInlineSuffix(systemOutContent, '    System.out.', javaPath);
  ok(
    systemOutSuffix === 'println()',
    `System.out. inline ghost is println() (got ${JSON.stringify(systemOutSuffix)})`,
  );

  const systemDotContent = 'public class App {\n  void run() {\n    System.\n  }\n}\n';
  const systemDotSuffix = h.memberInlineSuffix(systemDotContent, '    System.', javaPath);
  ok(
    systemDotSuffix === 'out',
    `System. inline ghost prefers out (got ${JSON.stringify(systemDotSuffix)})`,
  );

  const classBody = 'public class App {\n    \n}\n';
  ok(
    h.shouldFetchEmptyLineInline(javaPath, '    ', classBody, 2),
    'whitespace-only class body line triggers empty-line inline fetch',
  );
  ok(
    h.shouldPreferAiStatementInline(javaPath, '    ', classBody, 2),
    'empty line prefers AI statement inline (all languages)',
  );

  const ifBlock = 'public class App {\n  void run() {\n    if (x) {\n      \n    }\n  }\n}\n';
  ok(
    h.shouldPreferAiStatementInline(javaPath, '      ', ifBlock, 4),
    'if-block empty line prefers AI statement inline',
  );

  const emptyLineGhost = h.inferEmptyLineContinuationSuffix(javaPath, classBody, 2, '    ', 17);
  ok(
    emptyLineGhost === 'System.out.println();',
    `class-body empty-line template helper is System.out.println() (got ${JSON.stringify(emptyLineGhost)})`,
  );
  ok(
    h.localInlineSuggestion(javaPath, '    ', classBody, 2) === '',
    'class-body empty line has no local inline ghost (AI handles it)',
  );

  const ghostSuffix = h.inlineGhostSuffix(javaPath, '    System.out.', systemOutContent, 3, 17, 18);
  ok(
    ghostSuffix === 'println()',
    `inlineGhostSuffix after System.out. (got ${JSON.stringify(ghostSuffix)})`,
  );
  ok(
    h.inlineFilterUsesFullPrefix('    System.out.', javaPath),
    'member access uses full line prefix for inline filterText (Monaco prefix mode)',
  );
  const memberItems = h.buildInlineItems(
    { getLineContent: () => '    System.out.' },
    { lineNumber: 3, column: 18 },
    '    System.out.',
    'println()',
    javaPath,
  );
  ok(
    memberItems.items?.[0]?.filterText === '    System.out.println()',
    `member inline filterText includes qualifier (got ${JSON.stringify(memberItems.items?.[0]?.filterText)})`,
  );

  const methodBody = 'public class App {\n  void run() {\n    S\n  }\n}\n';
  ok(
    !h.shouldPreferAiStatementInline(javaPath, '    S', methodBody, 3),
    'single-letter identifier does not prefer AI statement inline',
  );
  ok(
    h.shouldPreferAiStatementInline(javaPath, '    if', methodBody.replace('S', 'if'), 3),
    'if prefix prefers AI statement inline',
  );
  ok(
    !h.shouldFetchIndexCompletions('    S', 'S', javaPath),
    'single-letter identifier does not trigger index fetch while typing',
  );
  ok(
    h.shouldFetchIndexCompletions('    S', 'S', javaPath, { afterPause: true }),
    'single-letter identifier triggers index fetch after typing pause',
  );
  ok(
    h.extractInlinePartialToken('    Spr') === 'Spr',
    'extractInlinePartialToken reads partial identifier',
  );

  const springMain = '@SpringBootApplication\npublic class App {\n  public static void main(String[] args) {\n    Sp\n    SpringApplication.run(App.class, args);\n  }\n}\n';
  ok(
    !h.scopeIdentifierInlineSuffix(springMain, 'src/App.java', 4, 'S'),
    'scope identifier scan skipped for single-letter token',
  );
  const springSuffix = h.localInlineSuggestion('src/App.java', '    Sp', springMain, 4);
  ok(
    springSuffix === 'ringApplication',
    `Sp suggests SpringApplication from same file (got ${JSON.stringify(springSuffix)})`,
  );

  const mainEmptyLine = '@SpringBootApplication\npublic class App {\n  public static void main(String[] args) {\n    \n    SpringApplication.run(App.class, args);\n  }\n}\n';
  ok(
    h.shouldFetchEmptyLineInline('src/App.java', '    ', mainEmptyLine, 4),
    'empty line inside main() triggers empty-line inline fetch',
  );
  const emptyMainGhost = h.inferEmptyLineContinuationSuffix(
    'src/App.java', mainEmptyLine, 4, '    ', 17,
  );
  ok(
    emptyMainGhost === 'System.out.println();',
    `empty main() line suggests block starter, not duplicate of line below (got ${JSON.stringify(emptyMainGhost)})`,
  );
  ok(
    h.localInlineSuggestion('src/App.java', '    ', mainEmptyLine, 4) === '',
    'empty main() line has no local inline ghost',
  );

  const gradleDeps = 'dependencies {\n  impl\n  implementation \'com.example:lib\'\n}\n';
  ok(
    h.localInlineSuggestion('build.gradle', '  impl', gradleDeps, 2) === 'ementation',
    `build.gradle impl completes from nearby line (got ${JSON.stringify(h.localInlineSuggestion('build.gradle', '  impl', gradleDeps, 2))})`,
  );
  const gradleEmpty = 'dependencies {\n  \n  implementation \'com.example:lib\'\n}\n';
  ok(
    h.inferEmptyLineContinuationSuffix('build.gradle', gradleEmpty, 2, '  ', 17).includes('implementation '),
    'build.gradle empty line suggests dependency keyword prefix from line below',
  );
  const gradlePropsBody = 'org.gradle.jvmargs=-Xmx2g\n\norg.gradle\norg.gradle.parallel=true\n';
  ok(
    h.gradlePropertiesLocalInline('gradle.properties', 'org.gradle', gradlePropsBody, 3) === '.parallel',
    `gradle.properties completes nearby key prefix (got ${JSON.stringify(h.gradlePropertiesLocalInline('gradle.properties', 'org.gradle', gradlePropsBody, 3))})`,
  );
  ok(
    h.INLINE_NEARBY_LINES === 5,
    'INLINE_NEARBY_LINES is 5',
  );

  const argsForLoop = 'public class App {\n  public static void main(String[] args) {\n    for (int i = 0; i < args.length; i++) {\n      \n    }\n  }\n}\n';
  ok(
    h.parseJavaIndexedForLoop('int i = 0; i < args.length; i++')?.kind === 'array',
    'indexed for-loop parser detects array .length bound',
  );
  ok(
    h.parseJavaIndexedForLoop('int i = 0; i < items.size(); i++')?.kind === 'list',
    'indexed for-loop parser detects List .size() bound',
  );
  ok(
    h.javaIndexedAccessExpr(h.parseJavaIndexedForLoop('int i = 0; i < args.length; i++')) === 'args[i]',
    'array loop access uses bracket indexing',
  );
  ok(
    h.javaIndexedAccessExpr(h.parseJavaIndexedForLoop('int i = 0; i < items.size(); i++')) === 'items.get(i)',
    'list loop access uses .get(i)',
  );
  const argsLoopGhost = h.inferEmptyLineContinuationSuffix(
    'src/App.java', argsForLoop, 4, '      ', 17,
  );
  ok(
    argsLoopGhost.includes('args[i]'),
    `args.length for-loop body uses args[i] (got ${JSON.stringify(argsLoopGhost)})`,
  );
  ok(
    !argsLoopGhost.includes('.get(i)'),
    'args.length for-loop body does not use .get(i)',
  );
  const argsLoopGhostJava8 = h.inferEmptyLineContinuationSuffix(
    'src/App.java', argsForLoop, 4, '      ', 8,
  );
  ok(
    argsLoopGhostJava8 === 'args[i];',
    `Java 8 for-loop body has no var (got ${JSON.stringify(argsLoopGhostJava8)})`,
  );
  const listForLoop = 'class App {\n  void m() {\n    for (int i = 0; i < items.size(); i++) {\n      \n    }\n  }\n}\n';
  const listLoopGhost = h.inferEmptyLineContinuationSuffix(
    'src/App.java', listForLoop, 4, '      ', 17,
  );
  ok(
    listLoopGhost.includes('items.get(i)'),
    `List.size() for-loop body uses .get(i) (got ${JSON.stringify(listLoopGhost)})`,
  );

  ok(
    !h.isDeclarationTyping('src/App.java', '    S'),
    'Java single-letter S is not declaration typing',
  );
  ok(
    !h.shouldSuppressInlineGhost(
      'src/App.java', '    S', 'pringApplication', springMain, 4, 6,
    ),
    'Java S ghost is not suppressed as declaration typing',
  );
  ok(
    h.isDeclarationTyping('src/App.java', 'String greeting'),
    'Java String greeting is declaration typing',
  );
  ok(
    h.isDeclarationTyping('src/App.java', 'String greeting '),
    'Java String greeting + trailing space stays declaration typing',
  );
  ok(
    h.isDeclarationTyping('src/App.java', 'var '),
    'Java var + space is declaration typing',
  );
  ok(
    h.isDeclarationTyping('src/App.java', 'var name'),
    'Java var name is declaration typing',
  );
  ok(
    h.isDeclarationTyping('src/App.java', 'var name '),
    'Java var name + trailing space stays declaration typing (before =)',
  );
  ok(
    h.isDeclarativeLeadInFreeTyping('src/App.java', 'var'),
    'Java bare var is declarative free-typing lead-in',
  );
  ok(
    h.isDeclarativeLeadInFreeTyping('src/App.java', 'int'),
    'Java bare int is declarative free-typing lead-in',
  );
  ok(
    h.shouldFetchIndexCompletions('var name', 'name', 'src/App.java'),
    'Java var name still allows autocomplete index while typing the name',
  );
  ok(
    h.shouldSuppressInlineGhost(
      'src/App.java', 'var', '.', 'class A {\n  var\n}\n', 2, 6,
    ),
    'Java bare var suppresses punctuation ghost',
  );
  ok(
    h.shouldSuppressInlineGhost(
      'src/App.java', 'var name ', '.', 'class A {\n  var name \n}\n', 2, 12,
    ),
    'Java var name space suppresses punctuation ghost (Space must not insert .)',
  );
  ok(
    h.shouldRouteInlineToAi('src/App.java', 'var name', 'class A {\n  var name\n}\n', 2, '', true)
      || !h.shouldSuppressInlineGhost(
        'src/App.java', 'var name', 'Something', 'class A {\n  var name\n}\n', 2, 12,
      ),
    'Java declaration typing can still show AI/index ghosts (Space does not accept them)',
  );
  ok(
    !h.isDeclarationTyping('src/App.java', 'SpringApplication'),
    'Java partial type on statement line is not declaration typing',
  );
  ok(
    !h.isDeclarationTyping('src/main.kt', '    S'),
    'Kotlin single-letter S is not declaration typing',
  );
  ok(
    !h.isDeclarationTyping('src/App.groovy', '    S'),
    'Groovy single-letter S is not declaration typing',
  );
  ok(
    !h.isDeclarationTyping('src/app.py', '    s'),
    'Python partial identifier is not declaration typing',
  );
  ok(
    !h.isDeclarationTyping('src/lib.rs', '    S'),
    'Rust partial identifier is not declaration typing',
  );
  ok(
    !h.isDeclarationTyping('src/index.ts', '    S'),
    'TypeScript partial identifier is not declaration typing',
  );
  ok(
    !h.isDeclarationTyping('src/main.go', '    S'),
    'Go partial identifier is not declaration typing',
  );
  ok(
    !h.shouldFetchIndexCompletions('    S', 'S', 'src/lib.rs'),
    'Rust single-letter does not fetch index while typing',
  );
  ok(
    h.shouldFetchIndexCompletions('    S', 'S', 'src/lib.rs', { afterPause: true }),
    'Rust single-letter fetches index after pause',
  );
  ok(
    !h.shouldFetchIndexCompletions('    s', 's', 'src/app.py'),
    'Python single-letter does not fetch index while typing',
  );
  ok(
    h.shouldFetchIndexCompletions('    s', 's', 'src/app.py', { afterPause: true }),
    'Python single-letter fetches index after pause',
  );

  ok(
    !h.supportsWorkspaceIndexInline('README.md'),
    'markdown does not use workspace index inline',
  );
  ok(
    !h.shouldFetchIndexCompletions('    M', 'M', 'README.md'),
    'markdown M does not fetch Java workspace symbols',
  );
  ok(
    h.isProseLanguage('docs/guide.md'),
    'markdown detected as prose language',
  );
  ok(
    h.shouldRouteInlineToAi('README.md', '# Hello wo', '# Hello wo\n', 1),
    'markdown typing routes to AI inline',
  );
  ok(
    h.shouldRouteInlineToAi('README.md', '    ', '# Title\n\n    \n- item\n', 3),
    'markdown empty line routes to AI inline',
  );
  ok(
    !h.shouldRouteInlineToAi(
      'src/App.java',
      '    File',
      'public class App {\n  void m() {\n    File\n  }\n}\n',
      3,
    ),
    'Java File partial defers to local workspace index path',
  );
  ok(
    !h.shouldRouteInlineToAi(
      'src/App.java',
      '    System.out.',
      'public class App {\n  void m() {\n    System.out.\n  }\n}\n',
      3,
    ),
    'member context does not route to AI',
  );

  for (const [filePath, langId] of LANG_PATH_FIXTURES) {
    const emptyBody = 'above\n    \nbelow\n';
    ok(
      h.shouldRouteInlineToAi(filePath, '    ', emptyBody, 2),
      `${langId}: empty line routes inline to AI`,
    );
    if (h.supportsWorkspaceIndexInline(filePath)) {
      ok(
        !h.shouldRouteInlineToAi(
          filePath,
          '    File',
          'class App {\n  void m() {\n    File\n  }\n}\n',
          3,
        ),
        `${langId}: workspace index partial stays local`,
      );
    } else {
      ok(
        h.shouldRouteInlineToAi(filePath, '    Money', 'doc\n\n    Money', 3),
        `${langId}: non-index typing routes to AI`,
      );
    }
  }

  const fileLine = 'public class App {\n  public static void main(String[] args) {\n    File\n  }\n}\n';
  ok(
    !h.shouldPreferAiStatementInline('src/App.java', '    File', fileLine, 4),
    'File is not misdetected as finally keyword prefix',
  );
  ok(
    h.shouldFetchIndexCompletions('    File', 'File', 'src/App.java'),
    'File triggers workspace index fetch in Java',
  );
  const fileItems = [
    { label: 'File', kind: 'class' },
    { label: 'FileInputStream', kind: 'class' },
  ];
  ok(
    h.inlineSuffixFromIndexItems(fileItems, '    File', 'File', 'src/App.java') === 'InputStream',
    'exact File match falls through to next indexed type for ghost suffix',
  );
  ok(
    h.inlineSuffixFromIndexItems([{ label: 'File' }], '    File', 'File', 'src/App.java') === '',
    'exact File-only index list yields empty inline suffix',
  );
  ok(
    h.inlineSuffixFromIndexItems([{ label: 'File' }], '    Fil', 'Fil', 'src/App.java') === 'e',
    'partial Fil index ghost completes File',
  );
  ok(
    h.shouldPreferLspInlineGhost('src/App.java', 'System.'),
    'member access prefers LSP inline ghost',
  );
  ok(
    !h.shouldPreferLspInlineGhost('src/Billing.java', '    f'),
    'control keyword f does not prefer LSP inline over local for',
  );
  ok(
    h.inlineSuffixFromIndexItems(
      [{ label: 'out', kind: 'field' }, { label: 'in', kind: 'field' }],
      'System.o',
      'o',
      'src/App.java',
    ) === 'ut',
    'jdtls-order top completion drives inline ghost after dot',
  );
  ok(
    h.javaSyncTypeInlineSuffix('F') === 'ile',
    'uppercase F sync ghost completes File',
  );
  ok(
    h.javaSyncTypeInlineSuffix('Fil') === 'e',
    'partial Fil sync ghost completes File',
  );
  ok(
    h.shouldPreferControlKeywordInline('src/Billing.java', '    f', 'f'),
    'lowercase f prefers control keyword inline in Java',
  );
  ok(
    h.shouldPreferJavaTypeInline('src/Billing.java', '    F', 'F'),
    'uppercase F prefers Java type inline',
  );
  ok(
    !h.shouldPreferControlKeywordInline('src/Billing.java', '    F', 'F'),
    'uppercase F does not prefer control keyword inline',
  );
  ok(
    h.shouldPreferModifierKeywordInline('src/App.java', '    private', 'private'),
    'typing private prefers modifier keyword over types',
  );
  ok(
    h.shouldPreferModifierKeywordInline('src/App.java', '    priv', 'priv'),
    'partial priv prefers private modifier',
  );
  ok(
    !h.shouldPreferModifierKeywordInline('src/App.java', '    String privateName', 'privateName'),
    'identifier after type is not modifier typing',
  );
  ok(
    h.inlineSuffixFromIndexItems(
      [{ label: 'PrivateKeyEntry', kind: 'class' }, { label: 'private', kind: 'keyword' }],
      '    private',
      'private',
      'src/App.java',
    ) === '',
    'index ghost suppressed for private → PrivateKeyEntry',
  );
  ok(
    h.localInlineSuggestion(
      'src/App.java',
      '    private',
      'public class App {\n    private\n}\n',
      2,
      17,
      {
        helpers: {
          getRepo: () => 'r',
          getActivePath: () => 'src/App.java',
          getJavaSourceOverlays: () => [],
        },
        model: { getValue: () => 'public class App {\n    private\n}\n' },
        position: { lineNumber: 2, column: 12 },
      },
    ) === '',
    'local inline does not extend private into PrivateKeyEntry',
  );
  ok(
    h.localInlineSuggestion(
      'src/App.java',
      '    priv',
      'public class App {\n    priv\n}\n',
      2,
      17,
    ) === 'ate',
    'local inline completes priv → private',
  );
  ok(
    h.localInlineSuggestion(
      'src/App.java',
      '    private',
      'public class App {\n    private String privateField;\n    private\n}\n',
      3,
      17,
      12,
      null,
      { fast: true },
    ) === '',
    'fast scope must not ghost privateField after complete private modifier',
  );
  ok(
    h.localInlineSuggestion(
      'src/App.java',
      '    private stat',
      'public class App {\n    String status;\n    private stat\n}\n',
      3,
      17,
      16,
      null,
      { fast: true },
    ) === 'ic',
    'modifier typing prefers static over nearby status identifier',
  );
  ok(
    h.extractInlinePartialToken('    private ') === '',
    'no partial token after modifier trailing space',
  );
  ok(
    h.isCompleteModifierSequence('    private ', 'src/App.java'),
    'private + space is a complete modifier sequence',
  );
  ok(
    h.shouldPreferModifierKeywordInline('src/App.java', '    private ', ''),
    'after private space still prefers modifier lead-in',
  );
  ok(
    h.inlineSuffixFromIndexItems(
      [{ label: 'PrivateKeyEntry', kind: 'class' }, { label: 'private', kind: 'keyword' }],
      '    private ',
      '',
      'src/App.java',
    ) === '',
    'index ghost suppressed after private trailing space',
  );
  ok(
    h.localInlineSuggestion(
      'src/App.java',
      '    private ',
      'public class App {\n    private String privateField;\n    private \n}\n',
      3,
      17,
      13,
      null,
      { fast: true },
    ) === '',
    'no ghost after private space — typing next word must be free',
  );
  ok(
    !h.shouldPrefetchInlineOnPause(
      'src/App.java',
      '    private ',
      'public class App {\n    private \n}\n',
      2,
    ),
    'do not prefetch index after private space',
  );
  ok(
    h.modifierKeywordInlineSuffix('s', 'src/App.java') === 'tatic',
    'modifier suffix for s is static not switch',
  );

  // Every language with modifier keywords: free typing after complete keyword + after Space.
  const allModifierLangs = [
    {
      label: 'java',
      path: 'src/App.java',
      mod: 'private',
      partial: 'priv',
      partialExpect: 'ate',
      hijack: 'PrivateKeyEntry',
      body: (p) => `public class App {\n${p}\n}\n`,
    },
    {
      label: 'kotlin',
      path: 'src/Main.kt',
      mod: 'private',
      partial: 'intern',
      partialExpect: 'al',
      hijack: 'PrivateKeyEntry',
      body: (p) => `class App {\n${p}\n}\n`,
    },
    {
      label: 'kotlin-script',
      path: 'build.gradle.kts',
      mod: 'private',
      partial: 'priv',
      partialExpect: 'ate',
      hijack: 'PrivateType',
      body: (p) => `class App {\n${p}\n}\n`,
    },
    {
      label: 'groovy',
      path: 'src/App.groovy',
      mod: 'public',
      partial: 'pub',
      partialExpect: 'lic',
      hijack: 'PublicKey',
      body: (p) => `class App {\n${p}\n}\n`,
    },
    {
      label: 'csharp',
      path: 'Program.cs',
      mod: 'private',
      partial: 'priv',
      partialExpect: 'ate',
      hijack: 'PrivateType',
      body: (p) => `class Program {\n${p}\n}\n`,
    },
    {
      label: 'cpp',
      path: 'main.cpp',
      mod: 'private',
      partial: 'priv',
      partialExpect: 'ate',
      hijack: 'private_field',
      body: (p) => `class Foo {\n${p}\n};\n`,
    },
    {
      // .c paths map to cpp lang id — C modifiers are covered by the cpp table.
      label: 'c-as-cpp',
      path: 'main.c',
      mod: 'static',
      partial: 'stat',
      partialExpect: 'ic',
      hijack: 'status',
      body: (p) => `${p}\nint x;\n`,
      line: 1,
    },
    {
      label: 'swift',
      path: 'main.swift',
      mod: 'private',
      partial: 'filep',
      partialExpect: 'rivate',
      hijack: 'privateVar',
      body: (p) => `class Foo {\n${p}\n}\n`,
    },
    {
      label: 'php',
      path: 'src/User.php',
      mod: 'protected',
      partial: 'prot',
      partialExpect: 'ected',
      hijack: 'ProtectedType',
      body: (p) => `<?php\nclass User {\n${p}\n}\n`,
      line: 3,
    },
    {
      label: 'typescript',
      path: 'app.ts',
      mod: 'private',
      partial: 'priv',
      partialExpect: 'ate',
      hijack: 'PrivateKey',
      body: (p) => `class App {\n${p}\n}\n`,
    },
    {
      label: 'javascript',
      path: 'app.js',
      mod: 'static',
      partial: 'stat',
      partialExpect: 'ic',
      hijack: 'status',
      body: (p) => `class App {\n${p}\n}\n`,
    },
    {
      label: 'rust',
      path: 'src/lib.rs',
      mod: 'pub',
      partial: 'pu',
      partialExpect: 'b',
      hijack: 'public_fn',
      body: (p) => `mod m {\n${p}\n}\n`,
    },
    {
      label: 'dart',
      path: 'lib/main.dart',
      mod: 'static',
      partial: 'fina',
      partialExpect: 'l',
      hijack: 'finalizer',
      body: (p) => `class App {\n${p}\n}\n`,
    },
  ];
  for (const cfg of allModifierLangs) {
    const line = cfg.line || 2;
    const indent = '    ';
    const complete = `${indent}${cfg.mod}`;
    const afterSpace = `${complete} `;
    const partialLine = `${indent}${cfg.partial}`;
    const bodyComplete = cfg.body(complete);
    const bodySpace = cfg.body(afterSpace);
    const bodyPartial = cfg.body(partialLine);
    const kws = h.modifierKeywordsForPath(cfg.path);
    ok(Array.isArray(kws) && kws.includes(cfg.mod), `${cfg.label}: has modifier ${cfg.mod}`);
    ok(
      h.isModifierLeadInFreeTyping(cfg.path, complete, cfg.mod),
      `${cfg.label}: free typing at complete ${cfg.mod}`,
    );
    ok(
      h.isModifierLeadInFreeTyping(cfg.path, afterSpace, ''),
      `${cfg.label}: free typing after ${cfg.mod} + space`,
    );
    ok(
      h.isCompleteModifierSequence(afterSpace, cfg.path),
      `${cfg.label}: complete modifier sequence after space`,
    );
    ok(
      !h.shouldFetchIndexCompletions(complete, cfg.mod, cfg.path),
      `${cfg.label}: no index fetch on complete ${cfg.mod}`,
    );
    ok(
      !h.shouldFetchIndexCompletions(afterSpace, '', cfg.path),
      `${cfg.label}: no index fetch after ${cfg.mod} space`,
    );
    ok(
      h.inlineSuffixFromIndexItems(
        [{ label: cfg.hijack, kind: 'class' }, { label: cfg.mod, kind: 'keyword' }],
        complete,
        cfg.mod,
        cfg.path,
      ) === '',
      `${cfg.label}: index cannot extend ${cfg.mod} → ${cfg.hijack}`,
    );
    ok(
      h.inlineSuffixFromIndexItems(
        [{ label: cfg.hijack, kind: 'class' }],
        afterSpace,
        '',
        cfg.path,
      ) === '',
      `${cfg.label}: index ghost suppressed after ${cfg.mod} space`,
    );
    ok(
      h.localInlineSuggestion(
        cfg.path, complete, bodyComplete, line, 17, complete.length + 1, null, { fast: true },
      ) === '',
      `${cfg.label}: no ghost on complete ${cfg.mod}`,
    );
    ok(
      h.localInlineSuggestion(
        cfg.path, afterSpace, bodySpace, line, 17, afterSpace.length + 1, null, { fast: true },
      ) === '',
      `${cfg.label}: no ghost after ${cfg.mod} space`,
    );
    ok(
      !h.shouldPrefetchInlineOnPause(cfg.path, afterSpace, bodySpace, line),
      `${cfg.label}: no prefetch after ${cfg.mod} space`,
    );
    ok(
      !h.shouldRouteInlineToAi(cfg.path, afterSpace, bodySpace, line, '', true),
      `${cfg.label}: no AI route after ${cfg.mod} space`,
    );
    ok(
      h.modifierKeywordInlineSuffix(cfg.partial, cfg.path) === cfg.partialExpect,
      `${cfg.label}: ${cfg.partial} → ${cfg.partialExpect}`,
    );
    ok(
      h.localInlineSuggestion(
        cfg.path, partialLine, bodyPartial, line, 17,
      ) === cfg.partialExpect,
      `${cfg.label}: local inline ${cfg.partial} → ${cfg.partialExpect}`,
    );
    ok(
      !h.isModifierLeadInFreeTyping(cfg.path, partialLine, cfg.partial),
      `${cfg.label}: partial ${cfg.partial} still completes (not free-typing yet)`,
    );
  }
  // Languages without access-modifier lead-in must not enter the free-typing gate.
  for (const path of ['app.py', 'main.go', 'README.md', 'data.json', 'style.css']) {
    ok(
      !h.shouldPreferModifierKeywordInline(path, '    private ', ''),
      `${path}: no modifier lead-in gate`,
    );
    ok(
      !h.isModifierLeadInFreeTyping(path, '    private ', 'private'),
      `${path}: not free-typing modifier mode`,
    );
  }
  ok(
    h.localInlineSuggestion(
      'src/Billing.java',
      '    f',
      'public class Billing {\n  void m() {\n    f\n  }\n}\n',
      3,
      17,
    ) === 'or',
    'local inline on f completes for keyword',
  );
  ok(
    h.localInlineSuggestion(
      'src/Billing.java',
      '    F',
      'public class Billing {\n  void m() {\n    F\n  }\n}\n',
      3,
      17,
    ) === 'ile',
    'local inline on F completes File type',
  );
  ok(
    h.shouldPreferAiStatementInline(
      'src/Billing.java',
      '    for',
      'public class Billing {\n  void m() {\n    for\n  }\n}\n',
      3,
    ),
    'complete for keyword prefers AI inline',
  );
  ok(
    !h.localInlineSuggestion(
      'src/Billing.java',
      '    for',
      'public class Billing {\n  void m() {\n    for\n  }\n}\n',
      3,
      17,
    ).includes('('),
    'local inline on for does not expand loop header (AI handles it)',
  );
  ok(
    h.controlStructureInlineSuffix(
      'src/Billing.java',
      '    for',
      'public class Billing {\n  void m() {\n    for\n  }\n}\n',
      3,
      17,
    ).includes('('),
    'controlStructureInlineSuffix helper still expands loop header',
  );
  ok(
    h.inlineSuffixFromIndexItems(
      [{ label: 'for', kind: 'keyword' }, { label: 'File', kind: 'class' }],
      '    F',
      'F',
    ) === 'ile',
    'index inline ranks class File for uppercase F',
  );

  const codeBodies = {
    java: { path: 'src/App.java', open: 'class App {\n  void m() {\n', close: '  }\n}\n', line: 3 },
    kotlin: { path: 'src/main.kt', open: 'class App {\n  fun m() {\n', close: '  }\n}\n', line: 3 },
    groovy: { path: 'build.gradle', open: 'class App {\n  void m() {\n', close: '  }\n}\n', line: 3 },
    javascript: { path: 'app.js', open: 'function m() {\n', close: '}\n', line: 2 },
    typescript: { path: 'app.ts', open: 'function m() {\n', close: '}\n', line: 2 },
    python: { path: 'app.py', open: 'def m():\n', close: '    pass\n', line: 2 },
    go: { path: 'main.go', open: 'func m() {\n', close: '}\n', line: 2 },
    rust: { path: 'src/lib.rs', open: 'fn m() {\n', close: '}\n', line: 2 },
    csharp: { path: 'Program.cs', open: 'class App {\n  void M() {\n', close: '  }\n}\n', line: 3 },
    cpp: { path: 'main.cpp', open: 'int main() {\n', close: '}\n', line: 2 },
    ruby: { path: 'app.rb', open: 'def m\n', close: 'end\n', line: 2 },
    php: { path: 'index.php', open: 'function m() {\n', close: '}\n', line: 2 },
    swift: { path: 'main.swift', open: 'func m() {\n', close: '}\n', line: 2 },
    shell: { path: 'script.sh', open: 'm() {\n', close: '}\n', line: 2 },
    lua: { path: 'script.lua', open: 'function m()\n', close: 'end\n', line: 2 },
    dart: { path: 'main.dart', open: 'void m() {\n', close: '}\n', line: 2 },
  };

  const fInlineExpected = {
    rust: 'n',
    kotlin: 'or',
    shell: 'or',
  };

  for (const [lang, cfg] of Object.entries(codeBodies)) {
    const content = `${cfg.open}    f\n${cfg.close}`;
    ok(
      h.shouldPreferControlKeywordInline(cfg.path, '    f', 'f'),
      `${lang}: lowercase f prefers control keyword inline`,
    );
    const expectedF = fInlineExpected[lang] || 'or';
    ok(
      h.localInlineSuggestion(cfg.path, '    f', content, cfg.line, 17) === expectedF,
      `${lang}: f inline completes expected keyword suffix`,
    );
    const forContent = `${cfg.open}    for\n${cfg.close}`;
    const forGhost = h.controlStructureInlineSuffix(cfg.path, '    for', forContent, cfg.line, 17);
    ok(!!forGhost, `${lang}: for expands to control-structure inline`);
  }

  ok(
    !h.shouldPreferControlKeywordInline('README.md', '    f', 'f'),
    'markdown skips control keyword inline',
  );
  ok(
    h.localInlineSuggestion('README.md', '    f', '# Title\n\n    f\n', 3, 17) !== 'or',
    'markdown f does not ghost-complete for',
  );
  ok(
    !h.shouldPreferControlKeywordInline('data.json', '    f', 'f'),
    'json skips control keyword inline',
  );
  ok(
    !h.isCodeStatementLanguage('data.json'),
    'json is not a code-statement language for inline',
  );

  ok(typeof h.shouldPrefetchInlineOnPause === 'function', 'shouldPrefetchInlineOnPause exported');
  ok(
    h.shouldPrefetchInlineOnPause('src/Billing.java', '    File'),
    'typing pause on Java File prefetches workspace index',
  );
  ok(
    h.shouldPrefetchInlineOnPause('src/Billing.java', '    System.out.'),
    'typing pause after System.out. prefetches member index',
  );
  ok(
    h.shouldPrefetchInlineOnPause('src/Billing.java', 'String greeting'),
    'typing pause during declaration typing still prefetches autocomplete',
  );
  ok(
    !h.shouldPrefetchInlineOnPause('README.md', '    Money'),
    'typing pause in markdown skips workspace index prefetch',
  );
  ok(
    !h.shouldPrefetchInlineOnPause('README.md', '    '),
    'typing pause on markdown blank line skips workspace index prefetch',
  );
  ok(
    h.shouldPrefetchInlineOnPause(
      'README.md',
      '    ',
      '# Title\n\n    \n- item one\n',
      3,
    ),
    'typing pause on markdown blank line with line below triggers empty-line inline path',
  );
  ok(
    h.shouldPrefetchInlineOnPause('src/Billing.java', '    '),
    'typing pause on Java blank line prefetches index continuation',
  );
  ok(
    h.inlineSuffixFromIndexItems([{ label: 'Money', insert: 'Money money = null;' }], '    ', '') === 'Money money = null;',
    'empty line uses top index/LSP completion insert as inline ghost',
  );
  ok(
    h.inlineSuffixFromIndexItems([{ label: 'Money' }], '    ', '') === 'Money',
    'empty line falls back to label when insert missing',
  );
  ok(
    !h.shouldRouteInlineToAi('README.md', '    ', '# Title\n\n    \n- item\n', 3, '', false),
    'empty line does not route to AI when AI inline disabled',
  );
  ok(
    !h.shouldPrefetchInlineOnPause('src/Billing.java', '    SpringApplication.run();'),
    'typing pause on completed statement line skips index prefetch',
  );
  ok(
    !h.isWhitespaceOnlyLine('    SpringApplication.run();'),
    'isWhitespaceOnlyLine is true only for blank/indent-only lines',
  );
  ok(
    h.isWhitespaceOnlyLine('    '),
    'indent-only line is whitespace-only',
  );

  ok(typeof h.inlineIndexPrefix === 'function', 'inlineIndexPrefix exported');
  ok(
    h.inlineIndexPrefix('    System.out.') === '',
    'inlineIndexPrefix empty after trailing dot',
  );
  ok(
    h.inlineIndexPrefix('    System.out.p') === 'p',
    'inlineIndexPrefix reads member after dot',
  );
  ok(
    h.inlineIndexPrefix('    File') === 'File',
    'inlineIndexPrefix reads identifier token',
  );

  ok(typeof h.isControlKeywordPrefix === 'function', 'isControlKeywordPrefix exported');
  ok(!h.isControlKeywordPrefix('File'), 'File is not misread as finally prefix');
  ok(!h.isControlKeywordPrefix('SpringApplication'), 'SpringApplication is not a control keyword');
  ok(h.isControlKeywordPrefix('fin'), 'fin prefix matches finally keyword');
  ok(!h.isControlKeywordPrefix('Fin'), 'capitalized Fin is treated as identifier');

  const mlSrc = fs.readFileSync(path.join(STATIC, 'monaco-languages.js'), 'utf8');
  ok(mlSrc.includes('function scheduleInlineOnPause('), 'scheduleInlineOnPause defined');
  ok(mlSrc.includes('scheduleInlineOnPause(editor)'), 'editor hooks schedule inline on pause');
  ok(mlSrc.includes('runInlineOnPause(ed)'), 'pause handler runs inline refresh + trigger');
  ok(mlSrc.includes('const INLINE_PAUSE_MS = 200'), 'inline pause debounce constant present');
  ok(mlSrc.includes('function isMarkupOrConfigPath('), 'markup/config path helper defined');
  ok(mlSrc.includes('onEmptyMarkup'), 'markup/config empty lines use light pause refresh');
  ok(
    mlSrc.includes('inferEmptyLineContinuationSuffix(')
      && mlSrc.includes('refreshInlineAfterEdit(ed, { light: onEmptyMarkup })'),
    'empty-line templates wired on pause refresh',
  );
  ok(
    mlSrc.includes('routeAi && aiOn'),
    'empty-line provider schedules debounced AI retry when enabled',
  );
  const emptyLspCachedIdx = mlSrc.indexOf('const emptyLspCached = inlineSuffixFromCachedIndex');
  ok(emptyLspCachedIdx !== -1, 'empty-line provider uses cached LSP path');
  const emptyProviderSlice = mlSrc.slice(Math.max(0, emptyLspCachedIdx - 200), emptyLspCachedIdx + 2200);
  ok(
    emptyProviderSlice.includes('scheduleAiInlineFetch()')
      && !emptyProviderSlice.includes('await fetchInlineComplete(model, position, linePrefix, false)'),
    'empty-line provider shows instant ghosts and schedules AI async (no blocking fetch)',
  );
  ok(
    mlSrc.includes('getAiInlineProviderAvailable?.()'),
    'inline AI gated on configured Cursor/Gemini/Claude provider',
  );
  ok(
    !mlSrc.includes("ed.trigger('reaper', 'editor.action.inlineSuggest.hide', {});\n          requestAnimationFrame(paint)"),
    'empty-line inline repaint does not hide-before-show (avoids backspace flicker)',
  );

  const appSrc = fs.readFileSync(path.join(STATIC, 'app.js'), 'utf8');
  ok(appSrc.includes("mode: 'prefix'"), 'Monaco inlineSuggest uses prefix mode for empty lines');
  ok(mlSrc.includes('const EMPTY_LINE_PAUSE_MS = 100'), 'empty line uses faster inline pause');
  ok(mlSrc.includes('refreshInlineLocalFast(editor)'), 'keystroke path uses fast local inline refresh');
  ok(mlSrc.includes('MIN_INDEX_INLINE_CHARS'), 'index inline requires minimum token length');

  ok(typeof h.shouldPauseInlineAtCursor === 'function', 'shouldPauseInlineAtCursor exported');
  ok(
    h.shouldPauseInlineAtCursor('src/App.java', '    SpringApplication.run();'),
    'Java: semicolon at end pauses inline',
  );
  ok(
    h.shouldPauseInlineAtCursor('src/App.java', '  }'),
    'Java: closing brace at end pauses inline',
  );
  ok(
    h.shouldPauseInlineAtCursor('src/App.java', '    if (x) {'),
    'Java: opening brace at end pauses inline',
  );
  ok(
    h.shouldPauseInlineAtCursor('src/App.java', '    foo()'),
    'Java: closed call at end pauses inline',
  );
  ok(
    !h.shouldPauseInlineAtCursor('src/App.java', '    System.out.'),
    'Java: member access mid-line still allows inline',
  );
  ok(
    !h.shouldPauseInlineAtCursor('src/App.java', '    '),
    'Java: whitespace-only line does not pause inline',
  );
  ok(
    h.shouldPauseInlineAtCursor('src/main.py', '    def foo():'),
    'Python: block-colon at end pauses inline',
  );
  ok(
    !h.shouldPauseInlineAtCursor('src/main.py', '    x = foo('),
    'Python: open paren mid-call still allows inline',
  );
  ok(
    h.shouldPauseInlineAtCursor('pom.xml', '  </dependency>'),
    'XML: closed tag at end pauses inline',
  );
  ok(
    h.shouldPauseInlineAtCursor('README.md', '- item'),
    'Markdown: completed list line pauses inline',
  );
  ok(typeof h.isLineInlineComplete === 'function', 'isLineInlineComplete exported');
  ok(
    h.isLineInlineComplete('    SpringApplication.run();'),
    'line ending with semicolon is inline-complete',
  );
  ok(
    h.isLineInlineComplete('    SpringApplication.run();   '),
    'trailing spaces after semicolon still inline-complete',
  );
  ok(
    !h.isLineInlineComplete('    SpringApplication.run()'),
    'line without semicolon is not inline-complete',
  );
  ok(
    !h.isLineInlineComplete('    foo(); bar'),
    'same-line next statement after semicolon is not inline-complete',
  );
  ok(
    h.shouldSuppressInlineGhost('src/App.java', '    x();', 'NextThing', '', 1, 10),
    'inline ghost suppressed after statement semicolon',
  );
  ok(
    !h.shouldPrefetchInlineOnPause('src/Billing.java', '    SpringApplication.run();'),
    'typing pause after semicolon skips index prefetch',
  );

  const emptyLineNeighborFixtures = {
    kotlin: {
      path: 'src/main.kt',
      body: 'class App {\n  fun m() {\n    \n    println(1)\n  }\n}\n',
      line: 3,
      prefix: '    ',
      expectIncludes: 'println',
    },
    groovy: {
      path: 'src/App.groovy',
      body: 'class App {\n  void m() {\n    \n    println 1\n  }\n}\n',
      line: 3,
      prefix: '    ',
      expectIncludes: 'println',
    },
    rust: {
      path: 'src/lib.rs',
      body: 'fn m() {\n    \n    println!("x");\n}\n',
      line: 2,
      prefix: '    ',
      expectIncludes: 'TODO',
    },
    javascript: {
      path: 'app.js',
      body: 'function m() {\n  \n  console.log(1);\n}\n',
      line: 2,
      prefix: '  ',
      expectIncludes: 'TODO',
    },
    typescript: {
      path: 'app.ts',
      body: 'function m() {\n  \n  console.log(1);\n}\n',
      line: 2,
      prefix: '  ',
      expectIncludes: 'TODO',
    },
    python: {
      path: 'app.py',
      body: 'def m():\n    \n    print(1)\n',
      line: 2,
      prefix: '    ',
      expectIncludes: 'pass',
    },
    go: {
      path: 'main.go',
      body: 'func m() {\n\t\n\tfmt.Println(1)\n}\n',
      line: 2,
      prefix: '\t',
      expectIncludes: 'TODO',
    },
    csharp: {
      path: 'Program.cs',
      body: 'class App {\n  void M() {\n    \n    Console.WriteLine(1);\n  }\n}\n',
      line: 3,
      prefix: '    ',
      expectIncludes: 'TODO',
    },
    ruby: {
      path: 'app.rb',
      body: 'def m\n  \n  puts 1\nend\n',
      line: 2,
      prefix: '  ',
      expectIncludes: 'TODO',
    },
    php: {
      path: 'index.php',
      body: 'function m() {\n  \n  echo 1;\n}\n',
      line: 2,
      prefix: '  ',
      expectIncludes: 'TODO',
    },
    swift: {
      path: 'main.swift',
      body: 'func m() {\n  \n  print(1)\n}\n',
      line: 2,
      prefix: '  ',
      expectIncludes: 'TODO',
    },
    cpp: {
      path: 'main.cpp',
      body: 'int main() {\n  \n  return 0;\n}\n',
      line: 2,
      prefix: '  ',
      expectIncludes: 'TODO',
    },
    shell: {
      path: 'script.sh',
      body: 'm() {\n  \n  echo hi\n}\n',
      line: 2,
      prefix: '  ',
      expectIncludes: 'TODO',
    },
    lua: {
      path: 'script.lua',
      body: 'function m()\n  \n  print(1)\nend\n',
      line: 2,
      prefix: '  ',
      expectIncludes: 'TODO',
    },
    dart: {
      path: 'main.dart',
      body: 'void m() {\n  \n  print(1);\n}\n',
      line: 2,
      prefix: '  ',
      expectIncludes: 'TODO',
    },
    r: {
      path: 'analysis.r',
      body: 'run <- function() {\n  \n  print(1)\n}\n',
      line: 2,
      prefix: '  ',
      expectIncludes: 'TODO',
    },
    sql: {
      path: 'query.sql',
      body: 'SELECT 1;\n\nSELECT 2;\n',
      line: 2,
      prefix: '',
      expectIncludes: 'SELECT',
    },
    html: {
      path: 'index.html',
      body: '<body>\n  \n  <p>hi</p>\n</body>\n',
      line: 2,
      prefix: '  ',
      expectIncludes: '<p>',
    },
    css: {
      path: 'style.css',
      body: '.app {\n  \n  color: red;\n}\n',
      line: 2,
      prefix: '  ',
      expectIncludes: 'color:',
    },
    scss: {
      path: 'styles.scss',
      body: '.app {\n  \n  color: red;\n}\n',
      line: 2,
      prefix: '  ',
      expectIncludes: 'color:',
    },
    less: {
      path: 'styles.less',
      body: '.app {\n  \n  color: red;\n}\n',
      line: 2,
      prefix: '  ',
      expectIncludes: 'color:',
    },
    json: {
      path: 'data.json',
      body: '{\n  \n  "a": 1\n}\n',
      line: 2,
      prefix: '  ',
      expectIncludes: '"',
    },
    markdown: {
      path: 'README.md',
      body: '# Title\n\n- item one\n',
      line: 2,
      prefix: '',
      expectIncludes: '- ',
    },
    xml: {
      path: 'pom.xml',
      body: '<root>\n  \n  <child/>\n</root>\n',
      line: 2,
      prefix: '  ',
      expectIncludes: '<child>',
    },
    yaml: {
      path: 'config.yaml',
      body: 'items:\n  \n  - one\n',
      line: 2,
      prefix: '  ',
      expectIncludes: '- ',
    },
    toml: {
      path: 'Cargo.toml',
      body: '[package]\n\nname = "x"\n',
      line: 2,
      prefix: '',
      expectIncludes: 'name=',
    },
    ini: {
      path: 'app.ini',
      body: '[section]\n\nkey=value\n',
      line: 2,
      prefix: '',
      expectIncludes: 'key=',
    },
    makefile: {
      path: 'Makefile',
      body: 'all:\n\t\n\techo hi\n',
      line: 2,
      prefix: '\t',
      expectIncludes: 'echo',
    },
    cmake: {
      path: 'CMakeLists.txt',
      body: 'project(x)\n\nadd_subdirectory(src)\n',
      line: 2,
      prefix: '',
      expectIncludes: 'add_subdirectory',
    },
    dockerfile: {
      path: 'Dockerfile',
      body: 'FROM alpine\n\nRUN echo hi\n',
      line: 2,
      prefix: '',
      expectIncludes: 'RUN',
    },
    protobuf: {
      path: 'api.proto',
      body: 'message A {}\n\nmessage B {}\n',
      line: 2,
      prefix: '',
      expectIncludes: 'message',
    },
    graphql: {
      path: 'schema.graphql',
      body: 'type Query {\n  \n  hello: String\n}\n',
      line: 2,
      prefix: '  ',
      expectIncludes: 'hello:',
    },
    plaintext: {
      path: 'notes.txt',
      body: 'line1\n\nline2\n',
      line: 2,
      prefix: '',
      expectEmpty: true,
    },
  };

  for (const [langId, cfg] of Object.entries(emptyLineNeighborFixtures)) {
    ok(
      h.shouldFetchEmptyLineInline(cfg.path, cfg.prefix, cfg.body, cfg.line),
      `${langId}: empty line with line below triggers empty-line inline fetch`,
    );
    const ghost = h.inferEmptyLineContinuationSuffix(cfg.path, cfg.body, cfg.line, cfg.prefix, 17);
    if (cfg.expectEmpty) {
      ok(
        ghost === '',
        `${langId}: empty line does not duplicate line below (got ${JSON.stringify(ghost)})`,
      );
    } else {
      ok(
        typeof ghost === 'string' && ghost.includes(cfg.expectIncludes),
        `${langId}: empty line suggests structural continuation, not full duplicate (got ${JSON.stringify(ghost)})`,
      );
    }
    ok(
      h.shouldPreferAiStatementInline(cfg.path, cfg.prefix, cfg.body, cfg.line),
      `${langId}: empty line prefers AI inline in editor`,
    );
    ok(
      h.localInlineSuggestion(cfg.path, cfg.prefix, cfg.body, cfg.line) === '',
      `${langId}: empty line has no local inline ghost`,
    );
    const neighbor = h.emptyLineNeighborBelow(cfg.body, cfg.line, cfg.prefix);
    ok(!!neighbor?.text, `${langId}: emptyLineNeighborBelow finds non-blank line below`);
  }

  for (const [filePath] of LANG_PATH_FIXTURES) {
    const prefix = '    ';
    const ghostText = '- next item';
    const items = h.buildInlineItems(
      { getLineContent: () => prefix },
      { lineNumber: 2, column: prefix.length + 1 },
      prefix,
      ghostText,
    );
    ok(
      items.items.length === 1 && items.items[0].filterText === `${prefix}${ghostText}`,
      `${filePath}: empty-line filterText includes indent for ghost paint`,
    );
    ok(
      items.items[0].insertText === ghostText,
      `${filePath}: empty-line insertText is suffix only (Tab uses full cache)`,
    );
    const keyA = h.buildInlineCacheKey('repo', filePath, 2, 5, prefix);
    const keyB = h.buildInlineCacheKey('repo', filePath, 2, 1, prefix);
    ok(keyA === keyB, `${filePath}: empty-line cache key ignores column`);
  }

  ok(
    h.isMarkupOrConfigPath('pom.xml'),
    'pom.xml is markup/config for empty-line AI path',
  );
  const pomGhost = h.buildInlineItems(
    { getLineContent: () => '  ' },
    { lineNumber: 2, column: 3 },
    '  ',
    '<dependency>',
  );
  ok(
    pomGhost.items[0]?.filterText === '  <dependency>',
    'pom.xml empty-line filterText includes indent for ghost paint',
  );

  ok(
    mlSrc.includes("const REAPER_COMPLETION_REV = '354'"),
    'completion revision bumped to c354',
  );
  ok(
    mlSrc.includes('ctx?.java_level != null'),
    'inlineJavaLevel prefers language-context java_level',
  );
  ok(
    !mlSrc.includes('ctx?.project_java_level != null'),
    'inlineJavaLevel does not read project_java_level directly',
  );
  ok(
    h.inlineJavaLevel({
      getJavaLanguageLevel: () => 17,
      getLanguageContext: () => ({ java_level: 21, project_java_level: 11, jdk_level: 21 }),
    }) === 21,
    'inlineJavaLevel uses completion java_level (max JDK and project)',
  );
  ok(
    h.inlineJavaLevel({ getJavaLanguageLevel: () => 21, getLanguageContext: () => ({ jdk_level: 21 }) }) === 21,
    'inlineJavaLevel falls back to configured JDK when context has no java_level',
  );
  ok(
    h.suggestionMatchesEditorLanguage('src/App.java', 'SELECT') === false,
    'SQL keywords filtered from Java completions',
  );
  ok(
    h.suggestionMatchesEditorLanguage('query.sql', 'SELECT') === true,
    'SQL keywords allowed in .sql files',
  );
  ok(
    h.suggestionMatchesEditorLanguage('src/App.java', 'String') === true,
    'Java type names allowed in Java completions',
  );
  ok(
    h.javaIndexedLoopBodyLine(
      h.parseJavaIndexedForLoop('i < items.size()', 'i'),
      '    ',
      21,
    ).includes('var item'),
    'Java 21 configured level enables var in indexed loop body',
  );
  ok(
    mlSrc.includes('function isImportTypingLine('),
    'isImportTypingLine defined for import-only index ghost',
  );
  ok(
    appSrc.includes('function completionLevelForPath'),
    'footer completion level helper defined',
  );
  ok(
    appSrc.includes('updateStatusLanguage(state.activeTab)'),
    'setLanguageStatus refreshes footer with compiler version',
  );
  ok(
    appSrc.includes('${lang} · ${level} · ${tool}'),
    'Java footer shows language level and compiler version',
  );
  const apiRs = fs.readFileSync(path.join(ROOT, 'src/web/api.rs'), 'utf8');
  ok(
    !apiRs.includes('workspace/java-level'),
    'java-level API removed — completions use configured JDK via language-context only',
  );
  ok(
    !apiRs.includes('java_language_level'),
    'java_language_level min(JDK, project) not exposed from API',
  );
  ok(
    !h.shouldRouteInlineToAi(
      'src/App.java',
      'import org.',
      'package com.example;\nimport org.\n',
      2,
    ),
    'import lines route to index ghost, not AI',
  );
  ok(
    h.shouldFetchIndexCompletions('import org.', 'org', 'src/App.java'),
    'import lines fetch workspace index completions',
  );
  ok(
    mlSrc.includes('NEVER preventDefault')
      && mlSrc.includes("e.key === ' '")
      && mlSrc.includes('dismissSuggestUi(ed)')
      && !mlSrc.includes("typeThroughCompletion(ed, ' ')"),
    'Space dismisses suggest but never preventDefault/typeThrough (typing stays free)',
  );
  ok(
    mlSrc.includes('function editorAcceptsInlineAi(')
      && mlSrc.includes('function cancelAiInlineFetch('),
    'AI inline gated on editor focus + cancel on blur',
  );
  ok(
    mlSrc.includes('if (!editorAcceptsInlineAi(editor)) return')
      && mlSrc.includes('cancelAiInlineFetch()'),
    'AI fetch skipped without focus; pending fetch cancelled on blur',
  );
  ok(
    mlSrc.includes('function pauseInlineRulesForPath('),
    'pauseInlineRulesForPath per-language boundary rules',
  );
  ok(
    mlSrc.includes('function shouldPauseInlineAtCursor('),
    'shouldPauseInlineAtCursor guards line boundaries',
  );
  ok(
    mlSrc.includes('function shouldRouteInlineToAi('),
    'shouldRouteInlineToAi defined for all-language AI routing',
  );
  ok(
    mlSrc.includes('function skipLocalStatementTemplates('),
    'skipLocalStatementTemplates defined',
  );
  ok(
    h.skipLocalStatementTemplates(),
    'local statement templates always skipped in editor inline path',
  );
  ok(
    h.localInlineSuggestion(
      'src/Billing.java',
      '    for',
      'public class Billing {\n  void m() {\n    for\n  }\n}\n',
      3,
      17,
    ) === '',
    'no local for-loop template ghost in editor path',
  );
  ok(
    !!h.controlStructureInlineSuffix(
      'src/Billing.java',
      '    for',
      'public class Billing {\n  void m() {\n    for\n  }\n}\n',
      3,
      17,
    ),
    'controlStructureInlineSuffix still available when templates disabled in editor',
  );
  ok(
    mlSrc.includes('if (options.repaint)')
      && mlSrc.includes('queueInlineSuggestion(ed, { emptyLine: isWhitespaceOnlyLine(linePrefix) })'),
    'setInlineCache re-queues ghost only when repaint requested',
  );
  ok(
    mlSrc.includes('scheduleInlineOnPause(editor)'),
    'cursor move schedules inline refresh on pause',
  );
}

/** Synthetic Java buffer for perf micro-benchmarks. */
function makeLargeJavaBody(methodCount = 600) {
  let body = 'public class Big {\n';
  for (let i = 0; i < methodCount; i += 1) {
    body += `  void m${i}() { int v${i} = ${i}; String t${i} = "n${i}"; }\n`;
  }
  body += '  void run() {\n    Sy\n  }\n}\n';
  return body;
}

function benchMs(fn, iterations = 1) {
  const t0 = performance.now();
  for (let i = 0; i < iterations; i += 1) fn();
  return performance.now() - t0;
}

function okPerfUnder(ms, budgetMs, msg) {
  ok(ms <= budgetMs, `${msg} (${ms.toFixed(2)}ms ≤ ${budgetMs}ms)`);
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

function extractRustFnBody(src, name) {
  const re = new RegExp(`\\bfn ${name}\\s*\\(`);
  const m = src.match(re);
  if (!m) return '';
  let i = m.index + m[0].length;
  while (i < src.length && src[i] !== '{') i += 1;
  if (i >= src.length) return '';
  const start = i + 1;
  let depth = 1;
  for (i = start; i < src.length; i += 1) {
    const ch = src[i];
    if (ch === '{') depth += 1;
    else if (ch === '}') {
      depth -= 1;
      if (depth === 0) return src.slice(start, i);
    }
  }
  return '';
}

/** Mirror static/app.js shouldDeferAutoSave — keep in sync when editing defer rules. */
function lineLooksIncompleteForAutoSave(line) {
  const trimmed = line.trimEnd();
  if (!trimmed || trimmed.startsWith('//') || trimmed.startsWith('*')) return false;
  if (/=\s*$/.test(trimmed)) return true;
  if (/\.\s*$/.test(trimmed)) return true;
  if (/,\s*$/.test(trimmed)) return true;
  if (/(\|\||&&|\?|:|\+\+|--)\s*$/.test(trimmed)) return true;
  if (/\(\s*$/.test(trimmed) && !/\)/.test(trimmed)) return true;
  return false;
}

function shouldDeferAutoSave(content, path) {
  const ext = path.split('.').pop()?.toLowerCase() ?? '';
  const codeExts = new Set([
    'java', 'kt', 'kts', 'scala', 'js', 'jsx', 'ts', 'tsx', 'mjs', 'cjs',
  ]);
  if (!content || !codeExts.has(ext)) return false;
  return content.split('\n').some((line) => lineLooksIncompleteForAutoSave(line));
}

function extractMonacoContentHandler(src, { contains } = {}) {
  const anchor = 'editor.onDidChangeModelContent(() => {';
  let searchFrom = 0;
  while (searchFrom < src.length) {
    const start = src.indexOf(anchor, searchFrom);
    if (start < 0) return '';
    let depth = 0;
    let end = -1;
    for (let i = start + anchor.indexOf('{'); i < src.length; i += 1) {
      const ch = src[i];
      if (ch === '{') depth += 1;
      else if (ch === '}') {
        depth -= 1;
        if (depth === 0) {
          end = i + 1;
          break;
        }
      }
    }
    if (end < 0) return '';
    const body = src.slice(start, end);
    if (!contains || body.includes(contains)) return body;
    searchFrom = end;
  }
  return '';
}

function testInlinePerformanceGuards() {
  const mlSrc = fs.readFileSync(path.join(STATIC, 'monaco-languages.js'), 'utf8');
  const keystrokeHandler = extractMonacoContentHandler(mlSrc, {
    contains: 'refreshInlineLocalFast(editor)',
  });

  ok(
    keystrokeHandler.includes('refreshInlineLocalFast(editor)')
      && keystrokeHandler.includes('scheduleInlineOnPause(editor)')
      && !keystrokeHandler.includes('refreshInlineAfterEdit'),
    'keystroke path: fast inline + pause only (no full refresh on every key)',
  );

  const refreshBody = extractFunctionBody(mlSrc, 'refreshInlineAfterEdit');
  ok(
    refreshBody && !refreshBody.includes('scheduleAiInlineFetch'),
    'refreshInlineAfterEdit does not schedule AI fetch (pause path only)',
  );

  ok(
    mlSrc.includes('const MIN_INDEX_INLINE_CHARS = 2'),
    'index inline gated to 2+ chars while typing',
  );
  ok(
    mlSrc.includes('const INLINE_PAUSE_MS = 200')
      && mlSrc.includes('const EMPTY_LINE_PAUSE_MS = 100'),
    'typing pause debounce longer than empty-line pause',
  );
  ok(
    mlSrc.includes('onEmptyMarkup'),
    'markup/config empty lines use light pause refresh',
  );
  ok(
    mlSrc.includes('isWhitespaceOnlyLine(linePrefix)')
      && mlSrc.includes('routeAi && aiOn'),
    'inline provider bails early on empty-line AI when AI enabled',
  );

  const queueBody = extractFunctionBody(mlSrc, 'queueInlineSuggestion');
  ok(
    queueBody.includes('emptyLine')
      && (queueBody.match(/editor\.action\.inlineSuggest\.trigger/g) || []).length <= 2,
    'inline suggest trigger capped (no per-key double fire on normal typing)',
  );
}

function extractEditorContentHandler(src) {
  const anchor = 'state.editor.onDidChangeModelContent(() => {';
  const start = src.indexOf(anchor);
  if (start < 0) return '';
  let depth = 0;
  for (let i = start + anchor.indexOf('{'); i < src.length; i += 1) {
    const ch = src[i];
    if (ch === '{') depth += 1;
    else if (ch === '}') {
      depth -= 1;
      if (depth === 0) return src.slice(start, i + 1);
    }
  }
  return '';
}

function parseAppConstMs(src, name) {
  const m = src.match(new RegExp(`const ${name} = (\\d+)`));
  return m ? Number(m[1]) : null;
}

function isProjectSourceFileForTest(path) {
  if (!path || path.startsWith('.reaper/')) return false;
  return path.replace(/\\/g, '/').endsWith('.java');
}

function isProjectBuildFileForTest(path) {
  if (!path) return false;
  const base = path.replace(/\\/g, '/').toLowerCase().split('/').pop() || '';
  return base === 'pom.xml'
    || base === 'build.gradle'
    || base === 'build.gradle.kts'
    || base === 'settings.gradle'
    || base === 'settings.gradle.kts'
    || base === 'gradle.properties';
}

/** Mirror of scheduleDiagnostics delay selection in app.js. */
function diagnosticDelayForPath(activeTab, constants, { isMarkupOrConfigPath = () => false } = {}) {
  let delay = constants.DIAG_DELAY_MS;
  if (activeTab?.endsWith('.java')) delay = constants.JAVA_DIAG_DELAY_MS;
  else if (isProjectBuildFileForTest(activeTab)) delay = constants.BUILD_DIAG_DELAY_MS;
  else if (isMarkupOrConfigPath(activeTab)) delay = constants.CONFIG_DIAG_DELAY_MS;
  return delay;
}

function withFakeTimers(run) {
  const timers = new Map();
  let now = 0;
  let nextId = 1;
  const origSetTimeout = globalThis.setTimeout;
  const origClearTimeout = globalThis.clearTimeout;
  globalThis.setTimeout = (fn, ms = 0) => {
    const id = nextId++;
    timers.set(id, { fn, fireAt: now + ms });
    return id;
  };
  globalThis.clearTimeout = (id) => {
    timers.delete(id);
  };
  const tick = (ms) => {
    now += ms;
    for (const [id, timer] of [...timers.entries()]) {
      if (timer.fireAt <= now) {
        timers.delete(id);
        timer.fn();
      }
    }
  };
  try {
    return run({ tick, now: () => now });
  } finally {
    globalThis.setTimeout = origSetTimeout;
    globalThis.clearTimeout = origClearTimeout;
  }
}

function createActiveDiagHarness(pickDelay, runDiag) {
  let timer = null;
  let runs = 0;
  return {
    schedule(activeTab) {
      clearTimeout(timer);
      const delay = pickDelay(activeTab);
      if (delay == null) return;
      timer = setTimeout(() => {
        timer = null;
        runs += 1;
        runDiag?.(activeTab);
      }, delay);
    },
    get runs() {
      return runs;
    },
  };
}

function testJavaDiagnosticsThrottling(appSrc, win) {
  const constants = {
    DIAG_DELAY_MS: parseAppConstMs(appSrc, 'DIAG_DELAY_MS'),
    JAVA_DIAG_DELAY_MS: parseAppConstMs(appSrc, 'JAVA_DIAG_DELAY_MS'),
    ALL_JAVA_DIAG_DELAY_MS: parseAppConstMs(appSrc, 'ALL_JAVA_DIAG_DELAY_MS'),
    BUILD_DIAG_DELAY_MS: parseAppConstMs(appSrc, 'BUILD_DIAG_DELAY_MS'),
    CONFIG_DIAG_DELAY_MS: parseAppConstMs(appSrc, 'CONFIG_DIAG_DELAY_MS'),
  };

  ok(
    appSrc.includes('const ALL_JAVA_DIAG_DELAY_MS'),
    'all-tab Java diagnostics debounced after classpath/batch edits',
  );
  ok(
    appSrc.includes('const JAVA_DIAG_DELAY_MS'),
    'JAVA_DIAG_DELAY_MS constant retained for tuning',
  );

  const scheduleDiagBody = extractFunctionBody(appSrc, 'scheduleDiagnostics');
  ok(
    scheduleDiagBody.includes("endsWith('.java')")
      && scheduleDiagBody.includes("scope: 'typing'"),
    'scheduleDiagnostics queues lightweight typing javac for Java',
  );

  ok(
    !appSrc.includes('function scheduleAllJavaDiagnosticsOnTypingPause'),
    'typing pause no longer schedules all-tab Java diagnostics',
  );

  const contentHandler = extractMonacoContentHandler(appSrc, {
    fallbackMarker: 'scheduleDiagnostics();',
  });
  ok(
    contentHandler.includes('scheduleDiagnostics()')
      && !contentHandler.includes('scheduleAllJavaDiagnosticsOnTypingPause()'),
    'keystroke path still calls scheduleDiagnostics (Java uses typing javac inside)',
  );

  const activateBody = extractFunctionBody(appSrc, 'activateTabShell');
  ok(
    activateBody.includes('scheduleJavaFullDiagnostics()'),
    'tab switch to Java schedules full javac diagnostics',
  );
  ok(
    !activateBody.includes("void runDiagnostics()"),
    'tab switch does not fire immediate duplicate Java diagnostics',
  );

  const refreshAllBody = extractFunctionBody(appSrc, 'refreshAllJavaTabDiagnostics');
  ok(
    refreshAllBody.includes('if (path === activePath) continue'),
    'all-tab refresh leaves active editor tab to runDiagnostics',
  );

  const classpathBody = extractFunctionBody(appSrc, 'refreshProjectClasspathUi');
  ok(
    classpathBody.includes('refreshAllJavaTabDiagnostics()'),
    'classpath change refreshes all open Java tabs',
  );

  const saveBody = extractFunctionBody(appSrc, 'saveFile');
  ok(
    saveBody.includes('scheduleJavaFullDiagnostics()'),
    'save on Java file schedules debounced full javac',
  );
  ok(
    saveBody.includes('queueJavaFullDiagnostics'),
    'manual save queues immediate full javac after persist',
  );
  ok(
    !saveBody.includes('scheduleAllJavaDiagnostics()'),
    'ordinary Java save does not batch-refresh all open tabs',
  );

  const isMarkup = win?.ReaperLang?.isMarkupOrConfigPath?.bind(win.ReaperLang) ?? (() => false);
  ok(
    diagnosticDelayForPath('src/App.java', constants, { isMarkupOrConfigPath: isMarkup })
      === constants.JAVA_DIAG_DELAY_MS,
    'delay selection: Java files use JAVA_DIAG_DELAY_MS for typing javac',
  );
  ok(
    diagnosticDelayForPath('pom.xml', constants, { isMarkupOrConfigPath: isMarkup })
      === constants.BUILD_DIAG_DELAY_MS,
    'delay selection: pom.xml uses BUILD_DIAG_DELAY_MS',
  );
  ok(
    diagnosticDelayForPath('src/lib.rs', constants, { isMarkupOrConfigPath: isMarkup })
      === constants.DIAG_DELAY_MS,
    'delay selection: Rust source keeps generic DIAG_DELAY_MS',
  );
  if (isMarkup('README.md')) {
    ok(
      diagnosticDelayForPath('README.md', constants, { isMarkupOrConfigPath: isMarkup })
        === constants.CONFIG_DIAG_DELAY_MS,
      'delay selection: markdown uses CONFIG_DIAG_DELAY_MS',
    );
  }

  withFakeTimers(({ tick }) => {
    const harness = createActiveDiagHarness(
      (path) => diagnosticDelayForPath(path, constants, { isMarkupOrConfigPath: isMarkup }),
    );
    harness.schedule('src/App.java');
    tick(constants.JAVA_DIAG_DELAY_MS + 5000);
    ok(harness.runs === 1, 'active Java diagnostics run after typing debounce');
  });
}

function normalizeDiagnosticsResponseForTest(body) {
  if (Array.isArray(body)) {
    return { diagnostics: body, cancelled: false };
  }
  return {
    diagnostics: Array.isArray(body?.diagnostics) ? body.diagnostics : [],
    cancelled: body?.cancelled === true,
  };
}

/** Mirror of removeJavaStringLiterals + local_member_without_parens_diags in java_diagnostics.rs */
function removeJavaStringLiteralsForTest(s) {
  let out = '';
  let inString = false;
  for (const ch of s) {
    if (ch === '"') {
      inString = !inString;
      out += ' ';
    } else if (inString) {
      out += ' ';
    } else {
      out += ch;
    }
  }
  return out;
}

function localMemberWithoutParensForTest(content) {
  const skipMembers = new Set(['length', 'out', 'err', 'in', 'class', 'this', 'super']);
  const diags = [];
  const seen = new Set();
  const lines = content.split('\n');
  for (let lineIdx = 0; lineIdx < lines.length; lineIdx++) {
    const line = lines[lineIdx];
    const trimmed = line.trimStart();
    if (trimmed.startsWith('//') || trimmed.startsWith('*') || trimmed.startsWith('import ')
      || trimmed.startsWith('package ')) {
      continue;
    }
    const code = removeJavaStringLiteralsForTest(line.split('//')[0]);
    let i = 0;
    while (i < code.length) {
      if (code[i] !== '.') {
        i += 1;
        continue;
      }
      const start = i + 1;
      if (start >= code.length || !/[a-z]/.test(code[start])) {
        i += 1;
        continue;
      }
      let j = start + 1;
      while (j < code.length && /[\w]/.test(code[j])) j += 1;
      const ident = line.slice(start, j);
      const after = line.slice(j).trimStart();
      if (after.startsWith('(') || skipMembers.has(ident)) {
        i = j;
        continue;
      }
      const key = `${lineIdx + 1}:${ident}`;
      if (seen.has(key)) {
        i = j;
        continue;
      }
      seen.add(key);
      diags.push({
        line: lineIdx + 1,
        column: start + 1,
        ident,
        message: `cannot find symbol\n  symbol:   variable ${ident}\n  location: `,
      });
      i = j;
    }
  }
  return diags;
}

/** Mirror of diagnosticMarkerSpan member-ref branch in app.js */
function diagnosticMemberSpanForTest(lineText, message) {
  const msgLower = String(message || '').toLowerCase();
  if (!msgLower.includes('cannot find symbol') && !msgLower.includes('does not exist')) {
    return null;
  }
  const sym = message.match(/symbol:\s*(?:class|interface|variable|method|package)?\s*([A-Za-z_][\w.]*)/i)
    || message.match(/package\s+([A-Za-z_][\w.]*)/i);
  if (!sym?.[1]) return null;
  const name = sym[1].split('.').pop();
  const scanLine = removeJavaStringLiteralsForTest(lineText);
  const memberRe = new RegExp(`([A-Za-z_][\\w]*)\\.${name}(?!\\s*\\()`, 'g');
  let memberMatch = null;
  let m;
  while ((m = memberRe.exec(scanLine)) !== null) {
    memberMatch = m;
  }
  if (!memberMatch) return null;
  const idx = memberMatch.index;
  return {
    startColumn: idx + 1,
    endColumn: idx + 1 + memberMatch[0].length,
    text: memberMatch[0],
  };
}

function shouldApplyDiagnosticsForTest(result) {
  if (result.cancelled && !result.diagnostics.length) return 'retry';
  return 'apply';
}

/** Mirror plain-symbol + member branches used by diagnosticMarkerSpan in app.js */
function diagnosticSymbolSpanForTest(lineText, message) {
  const member = diagnosticMemberSpanForTest(lineText, message);
  if (member) return member;
  const msgLower = String(message || '').toLowerCase();
  if (!msgLower.includes('cannot find symbol') && !msgLower.includes('does not exist')) {
    return null;
  }
  const sym = message.match(/symbol:\s*(?:class|interface|variable|method|package)?\s*([A-Za-z_][\w.]*)/i)
    || message.match(/package\s+([A-Za-z_][\w.]*)/i);
  if (!sym?.[1]) return null;
  const name = sym[1].split('.').pop().replace(/\(\).*$/, '').replace(/\(\s*$/, '');
  const scanLine = removeJavaStringLiteralsForTest(lineText);
  const dotIdx = scanLine.indexOf(`${name}.`);
  const plainIdx = scanLine.indexOf(name);
  const idx = dotIdx >= 0 ? dotIdx : plainIdx;
  if (idx < 0) return null;
  const endCol = dotIdx >= 0 ? idx + 1 + name.length + 1 : idx + name.length;
  return {
    startColumn: idx + 1,
    endColumn: endCol + 1,
    text: dotIdx >= 0 ? `${name}.` : name,
  };
}

function buildSquiggleMarkersForTest(model, diags) {
  const MarkerSeverity = { Error: 8, Warning: 4 };
  return diags.map((d) => {
    const line = Math.max(1, d.line || 1);
    const lineText = model.getLineContent(line);
    const span = diagnosticSymbolSpanForTest(lineText, d.message);
    const startCol = span?.startColumn ?? Math.max(1, d.column || 1);
    const endCol = span?.endColumn ?? startCol + 1;
    return {
      startLineNumber: line,
      startColumn: startCol,
      endLineNumber: line,
      endColumn: endCol,
      severity: d.severity === 'warning' ? MarkerSeverity.Warning : MarkerSeverity.Error,
      message: d.message,
    };
  });
}

const TERM_SOURCE_EXT_FOR_TEST = 'java|kt|kts|scala|groovy|gradle|xml|properties|json|yaml|yml|rs|py|js|ts|tsx|jsx|go|rb|cs|cpp|c|h|hpp|md|sql|html|css|vue|swift|php|sh|toml|proto';
const TERM_PATH_SEGMENT_FOR_TEST = `[A-Za-z0-9_.@\\[\\]-]+`;
const TERM_FILE_PATH_FOR_TEST = `(?:(?:[A-Za-z]:)?\\/)?${TERM_PATH_SEGMENT_FOR_TEST}(?:[\\/]${TERM_PATH_SEGMENT_FOR_TEST})*\\.(?:${TERM_SOURCE_EXT_FOR_TEST})`;

function terminalLinkRangeForTest(match, lineText = '') {
  const path = match[1];
  let start = match.index + match[0].indexOf(path);
  if (match[0].includes('[')) {
    let slice = match[0].slice(match[0].indexOf(path));
    if (lineText && start > 0 && lineText[start - 1] === '/' && path[0] !== '/') {
      start -= 1;
      slice = `/${slice}`;
    }
    return { start, end: start + slice.length };
  }
  let end = start + path.length + 1 + String(match[2]).length;
  if (match[3] && /:\d+(?::|\]|$)/.test(match[0])) {
    end += 1 + String(match[3]).length;
  }
  return { start, end };
}

function parseTerminalFileLocationsForTest(lineText) {
  const out = [];
  const seen = new Set();
  const patterns = [
    {
      re: new RegExp(`(${TERM_FILE_PATH_FOR_TEST}):\\[\\s*(\\d+)\\s*,\\s*(\\d+)\\s*\\]`, 'gi'),
      pick: (m) => ({ path: m[1], line: +m[2], column: +m[3] }),
    },
  ];
  for (const { re, pick } of patterns) {
    re.lastIndex = 0;
    let match;
    while ((match = re.exec(lineText)) !== null) {
      const hit = pick(match);
      if (match.index > 0 && lineText[match.index - 1] === '/' && !hit.path.startsWith('/')) {
        hit.path = `/${hit.path}`;
      }
      const { start, end } = terminalLinkRangeForTest(match, lineText);
      const key = `${hit.path}:${hit.line}:${hit.column}:${start}`;
      if (seen.has(key)) continue;
      seen.add(key);
      out.push({ ...hit, start, end });
    }
  }
  out.sort((a, b) => a.start - b.start);
  return out;
}

function resolveTerminalFilePathForTest(rawPath, { projectFolder = '', tabs = [] } = {}) {
  let p = String(rawPath || '').trim().replace(/\\/g, '/');
  if (!p) return null;

  const root = projectFolder.replace(/\\/g, '/').replace(/\/$/, '');
  if (root && (p === root || p.startsWith(`${root}/`))) {
    p = p === root ? '' : p.slice(root.length + 1);
  } else if (p.startsWith('/') || /^[A-Za-z]:\//.test(p)) {
    for (const tab of tabs) {
      const rel = String(tab || '').replace(/\\/g, '/').replace(/^\.\//, '');
      if (!rel) continue;
      if (p.endsWith(`/${rel}`) || p.endsWith(rel)) return rel;
    }
    const markers = ['/src/', '/test/', '/main/', '/java/', '/kotlin/', '/resources/'];
    for (const mark of markers) {
      const idx = p.lastIndexOf(mark);
      if (idx >= 0) {
        p = p.slice(idx + 1);
        break;
      }
    }
    if (p.startsWith('/') || /^[A-Za-z]:\//.test(p)) {
      const tail = p.match(/\/((?:src|test|services)\/.+)$/);
      if (!tail) return null;
      p = tail[1];
    }
  }

  p = p.replace(/^\.\//, '');
  if (!p || !/\.\w+$/.test(p)) return null;
  return p;
}

function testTerminalCompileLinkRegression(appSrc) {
  ok(appSrc.includes('TERM_PATH_SEGMENT'), 'terminal links: path regex supports absolute paths');
  ok(
    appSrc.includes("lineText[match.index - 1] === '/'") && appSrc.includes('hit.path = `/${hit.path}`'),
    'terminal links: restore leading slash on absolute javac paths',
  );
  ok(
    extractFunctionBody(appSrc, 'resolveTerminalFilePath').includes('p.endsWith(rel)'),
    'terminal links: resolve absolute path via open tab suffix',
  );
  ok(
    extractFunctionBody(appSrc, 'resolveTerminalFilePath').includes('lastIndexOf(mark)'),
    'terminal links: prefer last /src/ segment for multi-module paths',
  );

  const mavenLine = '[ERROR] /Users/sunny/reaper/workspaces/Spring-maven-complicated/services/analytics-service/src/main/java/com/enterprise/analytics/AnalyticsServiceApplication.java:[28,34] cannot find symbol';
  const hits = parseTerminalFileLocationsForTest(mavenLine);
  ok(hits.length === 1, 'terminal links: maven [ERROR] absolute path produces one link');
  ok(hits[0].line === 28 && hits[0].column === 34, 'terminal links: maven line:column parsed');
  ok(
    hits[0].path.startsWith('/Users/') && hits[0].path.endsWith('AnalyticsServiceApplication.java'),
    'terminal links: absolute path includes leading slash',
  );
  ok(hits[0].start === 8, 'terminal links: link range includes leading slash');

  const rel = resolveTerminalFilePathForTest(hits[0].path, {
    projectFolder: '/Users/sunny/reaper/workspaces/Spring-maven-complicated',
    tabs: ['services/analytics-service/src/main/java/com/enterprise/analytics/AnalyticsServiceApplication.java'],
  });
  ok(
    rel === 'services/analytics-service/src/main/java/com/enterprise/analytics/AnalyticsServiceApplication.java',
    'terminal links: absolute javac path resolves to repo-relative file',
  );
}

function testJavaSquiggleRegression(appSrc) {
  const applyBody = extractFunctionBody(appSrc, 'applyDiagnostics');
  const fullBody = extractFunctionBody(appSrc, 'runJavaDiagnosticsForPath');
  const fetchBody = extractFunctionBody(appSrc, 'fetchDiagnosticsForPath');
  const tabShellBody = extractFunctionBody(appSrc, 'activateTabShell');
  const javaDiagSrc = fs.readFileSync(
    path.join(ROOT, 'src', 'workspace', 'java_diagnostics.rs'),
    'utf8',
  );
  const checkJavaChunk = javaDiagSrc.slice(
    javaDiagSrc.indexOf('pub fn check_java'),
    javaDiagSrc.indexOf('fn check_project_java'),
  );

  ok(
    applyBody.includes("monaco.editor.setModelMarkers(model, 'reaper-diagnostics', markers)"),
    'squiggles: applyDiagnostics publishes Monaco error markers',
  );
  ok(
    applyBody.includes('diagnosticMarkerSpan(model, d)'),
    'squiggles: marker ranges come from diagnosticMarkerSpan',
  );
  ok(
    applyBody.includes('monaco.MarkerSeverity.Error'),
    'squiggles: javac errors use Error severity',
  );
  ok(
    applyBody.includes('refreshQuickFixBulbs'),
    'squiggles: applying diagnostics refreshes quick-fix bulbs',
  );

  ok(
    fullBody.includes('showCompileFooter'),
    'squiggles: full javac shows Compiling status while compile runs',
  );
  ok(
    fullBody.includes('JAVA_FULL_DIAG_MAX_RETRIES'),
    'squiggles: full javac retries capped (no long cancel loops)',
  );
  ok(
    appSrc.includes('Analyzing…'),
    'squiggles: full javac shows Analyzing status while compile runs',
  );
  ok(
    appSrc.includes('javaCompileFooterGen'),
    'squiggles: compile footer uses generation guard',
  );
  ok(
    appSrc.includes('COMPILE_FOOTER_SAFETY_MS'),
    'squiggles: compile footer auto-releases if javac stalls',
  );
  ok(
    appSrc.includes('queueJavaDiagnostics'),
    'squiggles: save/auto-save coalesce javac through single compile queue',
  );
  ok(
    appSrc.includes('flushJavaDiagnosticQueue'),
    'squiggles: one javac compile at a time (integration parity)',
  );
  ok(
    fetchBody.includes('chain after in-flight compile')
      || fetchBody.includes('never abort'),
    'squiggles: full javac chains after in-flight compile instead of abort storm',
  );
  ok(
    !checkJavaChunk.includes('with_workspace_java_lock'),
    'backend: workspace javac lock is not held for entire check_java (classpath prep stays unlocked)',
  );
  ok(
    javaDiagSrc.includes('Compile only the active overlay file')
      && javaDiagSrc.includes('with_workspace_java_lock(ws, || {\n        run_cancellable_javac'),
    'backend: project javac compiles active file only (sourcepath resolves cross-file types)',
  );
  ok(
    fullBody.includes('applyDiagnostics(path, result.diagnostics)'),
    'squiggles: save/full javac path calls applyDiagnostics with results',
  );
  ok(
    fullBody.includes('result.cancelled && !result.diagnostics.length'),
    'squiggles: cancelled empty javac retries instead of clearing markers',
  );
  ok(
    !appSrc.includes('javaFullDiagPending'),
    'squiggles: no pending-queue discard of javac results',
  );

  ok(
    tabShellBody.includes('scheduleJavaFullDiagnostics()'),
    'squiggles: switching to Java tab schedules full javac refresh',
  );
  ok(
    !tabShellBody.includes('clearDiagnostics()'),
    'squiggles: tab switch does not wipe Monaco markers before javac finishes',
  );

  const springLine = '    SpringAppliatin.run(AuthServiceApplication.class, args);';
  const springMsg = 'cannot find symbol\n  symbol:   class SpringAppliatin\n  location: class App';
  const springSpan = diagnosticSymbolSpanForTest(springLine, springMsg);
  ok(
    springSpan?.text?.startsWith('SpringAppliatin'),
    'squiggle span: typo class SpringAppliatin underlined',
  );
  ok(
    springSpan.endColumn > springSpan.startColumn,
    'squiggle span: SpringAppliatin has non-zero underline width',
  );

  const mockModel = { getLineContent: (ln) => (ln === 5 ? springLine : '') };
  const markers = buildSquiggleMarkersForTest(mockModel, [{ line: 5, message: springMsg }]);
  ok(markers.length === 1, 'squiggle markers: one javac error produces one marker');
  ok(markers[0].severity === 8, 'squiggle markers: javac error marker severity is Error');
  ok(
    markers[0].startColumn === springSpan.startColumn
      && markers[0].endColumn === springSpan.endColumn,
    'squiggle markers: SpringAppliatin marker matches diagnostic span',
  );

  const cancelledWithErrors = {
    diagnostics: [{ line: 5, message: springMsg }],
    cancelled: true,
  };
  ok(
    shouldApplyDiagnosticsForTest(cancelledWithErrors) === 'apply',
    'squiggles: cancelled javac with errors still paints markers',
  );
}

async function testJavaEditorUiBackendIntegration(appSrc) {
  const saveBody = extractFunctionBody(appSrc, 'saveFile');
  const scheduleAutoSaveBody = extractFunctionBody(appSrc, 'scheduleAutoSave');
  const fetchBody = extractFunctionBody(appSrc, 'fetchDiagnosticsForPath');
  const fullBody = extractFunctionBody(appSrc, 'runJavaDiagnosticsForPath');

  ok(
    saveBody.includes('writeTabToDisk') && saveBody.includes('queueJavaFullDiagnostics'),
    'integration: editor save persists then queues full javac',
  );
  const writeBody = extractFunctionBody(appSrc, 'writeTabToDiskOnce');
  const prepareSaveBody = extractFunctionBody(appSrc, 'prepareForSave');
  ok(
    writeBody.includes('allowDuringSave: true'),
    'save: PUT/read-back bypass save gate',
  );
  ok(
    saveBody.includes('prepareForSave') && prepareSaveBody.includes('usesInProcessApi'),
    'save: lightweight path on reaper://; HTTP fallback still drains connection pool',
  );
  ok(
    appSrc.includes('SAVE_GATE_DRAIN_MS'),
    'save: waits after aborting javac before PUT',
  );
  ok(
    !writeBody.includes('abortAllDiagnosticFetches'),
    'save: abort coalesced in prepareForSave (not per retry)',
  );
  ok(
    writeBody.includes('SAVE_MAX_RETRIES'),
    'save: retries PUT after aborting busy compile connections',
  );
  ok(
    prepareSaveBody.includes('usesInProcessApi'),
    'save: lightweight prepare when WebView uses reaper:// custom protocol',
  );
  ok(
    writeBody.includes('usesInProcessApi') && writeBody.includes('readTabFromDisk'),
    'save: read-back verify skipped on in-process protocol, kept for HTTP fallback',
  );
  ok(
    appSrc.includes('saveWriteCoalesceByPath') && appSrc.includes('flushSaveWriteCoalesce'),
    'save: coalesces concurrent PUTs per path (latest buffer wins)',
  );
  ok(
    !appSrc.includes('quit and reopen the app'),
    'save: timeout message does not tell user to restart app',
  );
  ok(
    saveBody.includes('showSavingFooterStatus'),
    'save: footer shows Saving… while persist runs',
  );
  ok(
    !saveBody.includes("showSaveFooterStatus('Saved', { auto: true })"),
    'save: auto-save does not flash Saved footer (misleading after rapid edits)',
  );
  const autoSaveBody = extractFunctionBody(appSrc, 'autoSaveToDisk');
  ok(
    autoSaveBody.includes('showSavingFooterStatus') && autoSaveBody.includes('writeTabToDisk'),
    'auto-save: footer shows Saving… then persists to disk first',
  );
  ok(
    autoSaveBody.includes('queueJavaDiagnostics') && autoSaveBody.includes("scope: 'typing'"),
    'auto-save: lightweight typing javac after disk write',
  );
  ok(
    !scheduleAutoSaveBody.includes('saveFile({ silent: true })'),
    'auto-save: scheduleAutoSave uses autoSaveToDisk not silent saveFile',
  );
  ok(
    appSrc.includes('shouldDeferAutoSave') && appSrc.includes('JAVA_AUTO_SAVE_DELAY_MS'),
    'auto-save: defers mid-edit syntax and uses longer Java idle delay',
  );
  ok(
    shouldDeferAutoSave('class A {\n  String s = \n}', 'App.java'),
    'auto-save defer: trailing = on Java line',
  );
  ok(
    !shouldDeferAutoSave('class A {\n  String s = file.getAbsolutePath();\n}', 'App.java'),
    'auto-save defer: complete Java statement may persist',
  );
  ok(
    shouldDeferAutoSave('x = foo.', 'App.js'),
    'auto-save defer: trailing . on JS line',
  );
  ok(
    !shouldDeferAutoSave('# comment only\n', 'notes.txt'),
    'auto-save defer: non-code files are not deferred',
  );
  ok(
    saveBody.includes('clearDiagnostics()') && saveBody.includes("showSaveFooterStatus('Save failed'"),
    'save: failed save clears stale squiggles and shows Save failed footer',
  );
  ok(
    saveBody.includes('queueJavaFullDiagnostics') && !saveBody.includes('fromSave'),
    'manual save queues immediate full javac after persist',
  );
  ok(
    fetchBody.includes("scope === 'full'") && fetchBody.includes('collectJavaDiagnosticOverlays'),
    'integration: editor diagnostics POST uses full scope + tab overlays',
  );
  ok(
    fullBody.includes('applyDiagnostics(path, result.diagnostics)'),
    'integration: javac results applied to Monaco markers',
  );
  ok(
    fs.existsSync(path.join(ROOT, 'scripts/lib/javac-edit-loop.mjs')),
    'integration: shared javac edit-loop harness present',
  );
  const loopSrc = fs.readFileSync(path.join(ROOT, 'scripts/lib/javac-edit-loop.mjs'), 'utf8');
  ok(
    loopSrc.includes('runConcurrentDiagnosticsBurst')
      && loopSrc.includes('runConcurrentSaveBurst')
      && loopSrc.includes('runConcurrentSaveDiagnosticBurst'),
    'integration: client-parallel burst harness present',
  );
  ok(
    loopSrc.includes('assertLatestHasNoStaleMarkers')
      && loopSrc.includes('reportDiagnosticBurstStats')
      && loopSrc.includes('messageContainsSymbol'),
    'integration: burst reports cancelled counts and word-boundary stale checks',
  );
  const coalesceSrc = fs.readFileSync(
    path.join(ROOT, 'scripts/lib/java-coalesce-harness.mjs'),
    'utf8',
  );
  ok(
    coalesceSrc.includes('createJavaFullDiagCoalescer')
      && coalesceSrc.includes('runCoalescedClientBurst')
      && coalesceSrc.includes('testCoalescerUnit'),
    'integration: editor coalesce harness mirrors queueJavaFullDiagnostics',
  );

  const apiRs = fs.readFileSync(path.join(ROOT, 'src/web/api.rs'), 'utf8');
  ok(
    apiRs.includes('async fn save_workspace_file')
      && apiRs.includes('tokio::fs::write')
      && apiRs.includes('patch_java_index_after_save'),
    'backend: save uses async fs write; java index patch stays off hot path',
  );
  ok(
    apiRs.includes('queue_java_index_patch_after_save')
      || fs.existsSync(path.join(ROOT, 'src/workspace/java_index_patch.rs')),
    'backend: java index patch coalesced (one worker per workspace)',
  );
  ok(
    fs.readFileSync(path.join(ROOT, 'src/web/api.rs'), 'utf8').includes('spawn_blocking(move || workspace::read_file'),
    'backend: file GET uses spawn_blocking so save read-back does not starve workers',
  );
  ok(
    apiRs.includes('DiagRequestGuard') && apiRs.includes('cancel_inflight_diagnostics'),
    'backend: diagnostics cancels javac when HTTP client disconnects',
  );
  ok(
    apiRs.includes('tokio::spawn(async move {')
      && apiRs.includes('oneshot::channel')
      && apiRs.includes('header::CONNECTION'),
    'backend: diagnostics runs in detached task; Connection: close frees WebKit slot',
  );
  ok(
    apiRs.includes('JAVA_FULL_DIAG_SEM') && apiRs.includes('Semaphore::new(1)'),
    'backend: one full javac HTTP request at a time (global semaphore)',
  );
  const mainRs = fs.readFileSync(path.join(ROOT, 'src/main.rs'), 'utf8');
  const guiRs = fs.readFileSync(path.join(ROOT, 'src/gui.rs'), 'utf8');
  const customProtocolRs = fs.readFileSync(path.join(ROOT, 'src/web/custom_protocol.rs'), 'utf8');
  const webModRs = fs.readFileSync(path.join(ROOT, 'src/web/mod.rs'), 'utf8');
  const configRs = fs.readFileSync(path.join(ROOT, 'src/config.rs'), 'utf8');
  ok(
    mainRs.includes('GuiProtocolBridge')
      && mainRs.includes('WEBVIEW_ENTRY')
      && guiRs.includes('with_asynchronous_custom_protocol'),
    'gui: WebView uses reaper:// custom protocol with loopback for WS/git',
  );
  ok(
    customProtocolRs.includes('GuiProtocolBridge') && customProtocolRs.includes('oneshot'),
    'gui: custom protocol dispatches in-process to axum router',
  );
  ok(
    appSrc.includes('ensureLoopbackWsBase()') || appSrc.includes('__REAPER_LOOPBACK_WS__'),
    'terminal: uses loopback WebSocket base when WebView is on reaper://',
  );
  ok(
    webModRs.includes('inject_loopback_ws_script') || customProtocolRs.includes('__REAPER_LOOPBACK_WS__'),
    'backend: injects loopback WebSocket URL into served index.html',
  );
  ok(
    appSrc.includes('loopback_ws'),
    'terminal: caches loopback WebSocket URL from /api/version',
  );
  ok(
    mainRs.includes('local_https::ensure_local_tls')
      && mainRs.includes('web::serve::serve_tls')
      && mainRs.includes('persist_server_url'),
    'backend: --server uses HTTPS/HTTP2 (serve_tls) and writes reaper.url',
  );
  ok(
    configRs.includes('uses_tls: !running_in_app_bundle()')
      && configRs.includes('if self.uses_tls { "https" } else { "http" }'),
    'backend: app bundle loopback uses plain HTTP; --server uses HTTPS',
  );
  const localHttpsRs = fs.readFileSync(path.join(ROOT, 'src/local_https.rs'), 'utf8');
  ok(
    localHttpsRs.includes('b"h2"') && localHttpsRs.includes('alpn_protocols'),
    'backend: rustls ALPN advertises HTTP/2',
  );

  {
    const { testCoalescerUnit } = await import(path.join(ROOT, 'scripts/lib/java-coalesce-harness.mjs'));
    let unitFailed = false;
    const unitOk = (cond, msg) => {
      if (!cond) unitFailed = true;
      return ok(cond, msg);
    };
    const unitFail = (msg) => {
      unitFailed = true;
      fail(msg);
    };
    const unitBurst = Number.parseInt(process.env.REAPER_EDITS || '10', 10);
    await testCoalescerUnit({ burstCount: unitBurst, ok: unitOk, fail: unitFail });
    ok(!unitFailed, `coalescer unit: ${unitBurst} queue calls coalesced (no server)`);
  }

  const binaryCandidates = [
    path.join(ROOT, 'target/release/reaper'),
    path.join(ROOT, 'target/debug/reaper'),
    path.join(ROOT, 'dist/Reaper.app/Contents/MacOS/reaper'),
  ];
  const hasBinary = binaryCandidates.some((p) => {
    try {
      fs.accessSync(p, fs.constants.X_OK);
      return true;
    } catch {
      return false;
    }
  });

  if (process.env.REAPER_SKIP_INTEGRATION === '1') {
    ok(true, 'integration spawn: skipped (REAPER_SKIP_INTEGRATION=1)');
    return;
  }
  if (!hasBinary) {
    ok(true, 'integration spawn: skipped (reaper binary not built yet)');
    return;
  }

  const mod = await import(path.join(ROOT, 'scripts/test-java-editor-integration.mjs'));
  const result = await mod.testJavaEditorIntegration({ quiet: true });
  if (result.skipped) {
    ok(true, 'integration spawn: skipped by harness');
    return;
  }
  ok(result.passed, `integration spawn: ${process.env.REAPER_EDITS || '10'}× sequential + client-parallel + coalesced bursts`);
}

function testJavaCompilerErrorRegression(appSrc) {
  const javaDiagSrc = fs.readFileSync(
    path.join(ROOT, 'src', 'workspace', 'java_diagnostics.rs'),
    'utf8',
  );
  const javaInflightSrc = fs.readFileSync(
    path.join(ROOT, 'src', 'workspace', 'java_javac_inflight.rs'),
    'utf8',
  );
  const diagnosticsSrc = fs.readFileSync(
    path.join(ROOT, 'src', 'workspace', 'diagnostics.rs'),
    'utf8',
  );

  ok(
    javaDiagSrc.includes('token.as_bytes()[0]')
      && javaDiagSrc.includes('local_missing_import_does_not_treat_create_temp_file_as_temp_file_class'),
    'backend: missing-import scan uses whole identifiers (not createTempFile→TempFile)',
  );
  ok(
    javaDiagSrc.includes('parse_class_symbol_accepts_pascal_case_variable'),
    'backend: filters javac variable Foo when Foo is a project type (ApiResponse.ok)',
  );
  ok(
    javaDiagSrc.includes('classpath::is_java_lang_public_type')
      && javaDiagSrc.includes('local_missing_import_skips_java_lang_runtime_exception'),
    'backend: missing-import scan skips all java.lang public types (RuntimeException, etc.)',
  );
  ok(
    !javaDiagSrc.includes('fn local_instant_diags'),
    'backend: diagnostics are javac-only (no instant local substitutes)',
  );
  ok(
    diagnosticsSrc.includes('struct FileDiagnosticsResult')
      && diagnosticsSrc.includes('fn diagnose_file'),
    'backend: diagnostics API exposes cancelled javac separately from results',
  );
  ok(
    diagnosticsSrc.includes('Do not fall back to jdtls publishDiagnostics')
      && /if cancelled \{[\s\S]*?return Ok\(FileDiagnosticsResult::cancelled\(\)\)/.test(diagnosticsSrc),
    'backend: cancelled full javac does not fall back to stale jdtls diagnostics',
  );
  ok(
    diagnosticsSrc.includes('fn merge_jdtls_diagnostics')
      && diagnosticsSrc.includes('typing_diagnostics'),
    'backend: merges jdtls publishDiagnostics alongside javac when not cancelled',
  );
  ok(
    javaInflightSrc.includes('content_fingerprint')
      && javaInflightSrc.includes('last_output'),
    'backend: javac inflight caches output per content fingerprint',
  );
  ok(
    extractFunctionBody(appSrc, 'diagnosticMarkerSpan').includes('lineWithoutJavaStringLiterals'),
    'frontend: diagnosticMarkerSpan ignores string literals when placing member squiggles',
  );
  ok(
    appSrc.includes('diagRetryTimer'),
    'frontend: retries diagnostics when javac compile was cancelled with no local errors',
  );

  ok(
    fs.readFileSync(path.join(ROOT, 'src', 'workspace', 'quick_fix.rs'), 'utf8')
      .includes('file_exists_receiver_fix_for_diagnostic'),
    'backend: local quick fix suggests file.exists() instead of bare exists()',
  );

  const existLine = '        System.out.println("file.exists" + file.exists);';
  const spanExist = diagnosticMemberSpanForTest(
    existLine,
    'cannot find symbol\n  symbol:   variable exists\n  location: variable file of type File',
  );
  ok(spanExist?.text === 'file.exists', 'diagnostic span: file.exists typo not string literal');

  const existTypoLine = '        System.out.println("file.exists" + file.exist);';
  const spanTypo = diagnosticMemberSpanForTest(
    existTypoLine,
    'cannot find symbol\n  symbol:   variable exist\n  location: variable file of type File',
  );
  ok(spanTypo?.text === 'file.exist', 'diagnostic span: underlines file.exist not string literal');
  ok(
    spanTypo?.startColumn === removeJavaStringLiteralsForTest(existTypoLine).indexOf('file.exist') + 1,
    'diagnostic span: column targets code-side file.exist',
  );

  const legacy = normalizeDiagnosticsResponseForTest([{ line: 1, message: 'err' }]);
  ok(legacy.cancelled === false && legacy.diagnostics.length === 1, 'normalizeDiagnosticsResponse: legacy array API');

  const wrapped = normalizeDiagnosticsResponseForTest({
    diagnostics: [{ line: 21, message: 'cannot find symbol' }],
    cancelled: false,
  });
  ok(wrapped.diagnostics.length === 1 && wrapped.cancelled === false, 'normalizeDiagnosticsResponse: wrapped success');

  const cancelledEmpty = normalizeDiagnosticsResponseForTest({ diagnostics: [], cancelled: true });
  ok(
    shouldApplyDiagnosticsForTest(cancelledEmpty) === 'retry',
    'cancelled empty diagnostics schedule retry instead of clearing squiggles',
  );

  const cancelledWithLocal = normalizeDiagnosticsResponseForTest({
    diagnostics: [{ line: 5, message: 'cannot find symbol variable exist' }],
    cancelled: true,
  });
  ok(
    shouldApplyDiagnosticsForTest(cancelledWithLocal) === 'apply',
    'cancelled javac with cached diagnostics still applies until retry succeeds',
  );

  // Do not spawn `cargo test` here — build-macos-app.sh and build.rs already run this
  // suite before `cargo build`; nested cargo deadlocks on the build directory lock.
  ok(
    javaDiagSrc.includes('check_project_java'),
    'backend: project Java diagnostics compile via javac',
  );
}

function testJavaJavacInflightGuards(appSrc) {
  const refreshBody = extractFunctionBody(appSrc, 'refreshAllJavaTabDiagnostics');
  const fetchBody = extractFunctionBody(appSrc, 'fetchDiagnosticsForPath');

  ok(
    appSrc.includes('const diagFetchByPath = new Map()'),
    'single-flight map tracks in-flight diagnostic fetches per file',
  );
  ok(
    fetchBody.includes('prev.content === content'),
    'in-flight diagnostic fetch for same path+content is shared (javac can take 30s+)',
  );
  ok(
    fetchBody.includes('prev.controller.abort()'),
    'new diagnostic fetch aborts stale request when buffer content changed',
  );
  ok(
    fetchBody.includes("prev.scope === 'full' && scope === 'full'")
      && fetchBody.includes('prev.promise.then'),
    'full javac chains follow-up fetch after in-flight compile finishes',
  );
  ok(
    appSrc.includes('diagRetryDelayMs'),
    'cancelled javac retries with backoff instead of aborting long compiles',
  );
  ok(
    appSrc.includes('function normalizeDiagnosticsResponse'),
    'diagnostic fetch normalizes cancelled javac responses',
  );
  ok(
    extractFunctionBody(appSrc, 'runDiagnostics').includes('result.cancelled && !result.diagnostics.length'),
    'cancelled javac with no local errors retries; non-empty results still apply',
  );
  ok(
    !refreshBody.includes('Promise.all'),
    'all-tab Java diagnostics do not compile every tab in parallel',
  );
  ok(
    refreshBody.includes('for (const path of javaTabs)'),
    'all-tab Java diagnostics iterate tabs sequentially',
  );
  ok(
    appSrc.includes("scope = 'typing'"),
    'typing diagnostics use lightweight scope default',
  );
  ok(
    extractFunctionBody(appSrc, 'runDiagnostics').includes("endsWith('.java')"),
    'runDiagnostics skips Java (dedicated typing javac queue)',
  );
  ok(
    extractFunctionBody(appSrc, 'runDiagnostics').includes("scope: 'typing'"),
    'non-Java active-file diagnostic fetch uses typing scope',
  );
  ok(
    fetchBody.includes("scope === 'full'")
      && fetchBody.includes('collectJavaDiagnosticOverlays'),
    'full diagnostics include open-tab overlays',
  );
  ok(
    refreshBody.includes("scope: 'full'")
      && refreshBody.includes('JAVA_DIAG_FULL_STAGGER_MS'),
    'all-tab refresh uses full scope with stagger between compiles',
  );
  ok(
    fs.readFileSync(path.join(ROOT, 'src', 'workspace', 'java_diagnostics.rs'), 'utf8')
      .includes('enum JavaDiagScope'),
    'backend: JavaDiagScope separates typing vs full javac',
  );
  ok(
    fs.readFileSync(path.join(ROOT, 'src', 'workspace', 'diagnostics.rs'), 'utf8')
      .includes('JavaDiagScope::Typing')
      && !fs.readFileSync(path.join(ROOT, 'src', 'workspace', 'diagnostics.rs'), 'utf8')
        .includes('typing scope should not run javac'),
    'backend: typing scope runs lightweight single-file javac for Java',
  );
  ok(
    (() => {
      const javaDiagSrc = fs.readFileSync(path.join(ROOT, 'src', 'workspace', 'java_diagnostics.rs'), 'utf8');
      return javaDiagSrc.includes('local_missing_import_flags_slf4j_annotation_without_import')
        && extractRustFnBody(javaDiagSrc, 'filter_project_javac_diags').includes('local_missing_import_type_diags');
    })(),
    'backend: missing-import pass wired for Lombok @Slf4j and other unresolved types',
  );
  ok(
    refreshBody.includes('allJavaDiagRefreshGen'),
    'all-tab refresh generation skips stale batch results',
  );
  ok(
    appSrc.includes('function isDiagFetchAbort'),
    'AbortError from cancelled diagnostic fetch is ignored',
  );
  ok(
    fs.readFileSync(path.join(ROOT, 'src', 'workspace', 'java_javac_inflight.rs'), 'utf8')
      .includes('fn peek_cached'),
    'backend: javac inflight serves cached compile output after superseded run',
  );
  ok(
    fs.readFileSync(path.join(ROOT, 'src', 'workspace', 'java_javac_inflight.rs'), 'utf8')
      .includes('run_cancellable_java_command')
      && fs.readFileSync(path.join(ROOT, 'src', 'workspace', 'java_javac_inflight.rs'), 'utf8')
        .includes('workspace_lock_for'),
    'backend serializes javac per workspace and cancels superseded per-file compiles',
  );
}

function testJavaMemberCompletionPipelineRegression(appSrc) {
  const mlSrc = fs.readFileSync(path.join(STATIC, 'monaco-languages.js'), 'utf8');
  const modRs = fs.readFileSync(path.join(ROOT, 'src', 'workspace', 'mod.rs'), 'utf8');
  const classpathRs = fs.readFileSync(path.join(ROOT, 'src', 'workspace', 'classpath.rs'), 'utf8');
  const symbolsRs = fs.readFileSync(path.join(ROOT, 'src', 'workspace', 'symbols.rs'), 'utf8');
  const dotBody = extractFunctionBody(mlSrc, 'handleDotCompletion');
  const closeBody = extractFunctionBody(appSrc, 'closeTab');
  const bindTabsBody = extractFunctionBody(appSrc, 'bindEditorTabs');

  ok(
    modRs.includes('java_completions_for_jdtls_gap_fill')
      && modRs.includes('merge_completion_items(jdtls_items, index_items, 80)'),
    'backend: java_completions uses jdtls primary with lightweight index gap-fill when ready',
  );
  ok(
    modRs.includes('find_local_java_definition')
      && modRs.includes('find_external_definition_with_well_known')
      && modRs.includes('Some(true)')
      && modRs.includes('jdtls::find_definition'),
    'backend: definition tries same-file then classpath/index then jdtls then symbol scan',
  );
  ok(
    classpathRs.includes('super::gradle::find_gradle_repo_root')
      && classpathRs.includes('find_gradle_settings_repo_root')
      && classpathRs.includes('java_navigation_scan_root')
      && classpathRs.includes('accept_java_definition')
      && classpathRs.includes('get_navigation_lookup'),
    'backend: Gradle navigation uses wrapper repo root for multi-module index + source scan',
  );
  const jdtlsRs = fs.readFileSync(path.join(ROOT, 'src/workspace/jdtls.rs'), 'utf8');
  ok(
    jdtlsRs.includes('if result.is_err()') && jdtlsRs.includes('session.child.kill()'),
    'backend: jdtls drops poisoned session after request timeout/error',
  );
  ok(
    classpathRs.includes('is_preferred_java_definition_path')
      && classpathRs.includes('guard.remove(key)'),
    'backend: definition cache evicts jar/build-output hits instead of caching permanent misses',
  );
  ok(
    classpathRs.includes('annotation_indices')
      && classpathRs.includes('annotations_matching_name_prefix'),
    'backend: @ annotation lookup uses precomputed annotation index (not full symbol scan)',
  );
  ok(
    modRs.includes('if items.is_empty() && classpath::is_java_like(from_path)')
      && classpathRs.includes('use_java_navigation_fallback'),
    'backend: full index + hardcoded FQCN tables only when jdtls offline or jdtls returns empty',
  );
  ok(
    classpathRs.includes('collect_annotation_completions')
      && classpathRs.includes('is_annotation_fqcn')
      && classpathRs.includes('if at_annotation'),
    'backend: @ annotation context uses dedicated annotation completion path',
  );
  ok(
    symbolsRs.includes('import_path_hint')
      || (symbolsRs.includes('parse_imports(content)') && symbolsRs.includes('best_definition')),
    'backend: symbol scan prefers explicit import path for Java definitions',
  );
  ok(
    classpathRs.includes('symbol_known_on_classpath'),
    'backend: classpath index resolves annotation types for missing-import diagnostics',
  );
  ok(
    classpathRs.includes('infer_java_receiver_type_from_expr')
      && classpathRs.includes('member_completions_for_qualifier')
      && classpathRs.includes('lookup.members_for_type'),
    'backend: member completion resolves receiver type then queries index members',
  );
  ok(
    classpathRs.includes('plain_java_instance_variable_dot_member_completions'),
    'backend: regression test for any-instance variable dot completions (File file.)',
  );
  ok(
    symbolsRs.includes('pub(crate) fn infer_java_receiver_type')
      && symbolsRs.includes('pub(crate) fn infer_java_receiver_type_from_expr'),
    'backend: type inference exported for member completion on any object',
  );
  ok(
    dotBody.includes('fetchCompletionsWithTimeout')
      && dotBody.includes('presentCompletionSuggestions')
      && dotBody.includes('local.suggestions.length > 0'),
    'frontend: handleDotCompletion fetches index members when local hints empty',
  );
  ok(
    mlSrc.includes('presentCompletionSuggestions(ed, items, { content, path })')
      && extractFunctionBody(mlSrc, 'fetchCompletions').includes('/workspace/completions'),
    'frontend: member completion shows index API results in fallback popup',
  );
  ok(
    appSrc.includes('function tabPathFromEl')
      && appSrc.includes('data-tab-idx')
      && bindTabsBody.includes('tabsBound'),
    'editor tabs: stable path resolution via tab index (close/switch)',
  );
  ok(
    !closeBody.includes('confirm(')
      && closeBody.includes('skipFlush: true'),
    'editor tabs: close without WKWebView confirm; active tab switch skips stale flush',
  );
}

function testRepoPickerUnregisterFlow() {
  const appSrc = fs.readFileSync(path.join(STATIC, 'app.js'), 'utf8');
  ok(typeof appSrc === 'string' && appSrc.includes('function bindRepoPicker'), 'bindRepoPicker defined');
  ok(appSrc.includes('repoPickerUnregisterBusy'), 'repo picker tracks unregister busy state');
  ok(
    appSrc.includes('data-repo-unregister-confirm') && appSrc.includes('data-repo-remove'),
    'repo picker remove + confirm controls',
  );
  ok(
    appSrc.includes('ij-clone-spinner') && appSrc.includes('Removing <strong>'),
    'repo unregister shows inline spinner while busy',
  );
  ok(appSrc.includes("repoApi(repoName, '/unregister')"), 'unregisterRepo POSTs /unregister');
  ok(
    appSrc.includes('if (state.repoPickerUnregisterBusy) return'),
    'repo picker ignores input while unregister is in flight',
  );
}

function testLocalJavaImportQuickFix(win) {
  const h = win.ReaperLang?.inlineTestHelpers?.();
  ok(typeof h?.localJavaImportFqcn === 'function', 'localJavaImportFqcn exported');
  ok(typeof h?.extractJavacClassSymbol === 'function', 'extractJavacClassSymbol exported');
  ok(h.localJavaImportFqcn('List') === 'java.util.List', 'local import resolves List');
  ok(h.localJavaImportFqcn('ArrayList') === 'java.util.ArrayList', 'local import resolves ArrayList');
  ok(h.localJavaImportFqcn('String') === null, 'java.lang String needs no import');
  ok(
    h.extractJavacClassSymbol('cannot find symbol symbol: class ArrayList location: class App') === 'ArrayList',
    'extractJavacClassSymbol parses class symbol',
  );
  ok(
    h.extractJavacClassSymbol('cannot find symbol symbol: variable foo location: class App') === 'foo',
    'extractJavacClassSymbol parses variable symbol',
  );
}

function testHiddenRepoRestoreFlow() {
  const appSrc = fs.readFileSync(path.join(STATIC, 'app.js'), 'utf8');
  ok(appSrc.includes('hiddenRepos'), 'app tracks hidden repos list');
  ok(appSrc.includes("api('/api/repos/hidden')"), 'loadRepos fetches hidden repos');
  ok(appSrc.includes('function restoreRepo') && appSrc.includes("repoApi(name, '/restore')"), 'restoreRepo POSTs /restore');
  ok(appSrc.includes('data-repo-restore') && appSrc.includes('Add back'), 'repo picker shows restore control');
  ok(appSrc.includes('Removed from Reaper'), 'repo picker labels hidden section');
}

function testLongRunningTaskBusyUi() {
  const appSrc = fs.readFileSync(path.join(STATIC, 'app.js'), 'utf8');
  const langSrc = fs.readFileSync(path.join(STATIC, 'monaco-languages.js'), 'utf8');
  const indexHtml = fs.readFileSync(path.join(STATIC, 'index.html'), 'utf8');
  ok(appSrc.includes('function setGlobalLoading') && indexHtml.includes('id="loading-spinner"'), 'global loading overlay has spinner');
  ok(appSrc.includes('function setPublishModalState') && appSrc.includes('publishBusy'), 'publish modal busy state');
  ok(appSrc.includes('function setPushModalBusy') && appSrc.includes('pushBusy'), 'push modal busy state');
  ok(appSrc.includes('function busyStatusHtml'), 'shared busy status markup helper');
  ok(appSrc.includes("setGlobalLoading(true, `Opening ${name}…`)"), 'opening repo shows global spinner');
  ok(appSrc.includes('function hydrateRepoWorkspace') && appSrc.includes('repoSelectToken'), 'repo switch serialized and hydrates in background');
  ok(appSrc.includes('function dismissLaunchSplashNow') && appSrc.includes('dismissLaunchSplashNow();'), 'repo open dismisses launch splash');
  ok(appSrc.includes('const showLoader = switching || !previousRepo'), 'first repo open in window shows loader');
  ok(appSrc.includes('leaveSaveGate();') && appSrc.includes('async function selectRepoOnce'), 'repo switch clears save gate before open');
  ok(appSrc.includes('window-title-project') && appSrc.includes('function updateWindowTitle'), 'window title bar project label wired');
  ok(indexHtml.includes('ij-window-titlebar') && indexHtml.includes('window-title-project'), 'window title bar markup');
  ok(appSrc.includes('header-search-input') && indexHtml.includes('ij-header-search-icon'), 'header search on right with icon');
  ok(!indexHtml.includes('file-breadcrumb'), 'menubar file breadcrumb removed (editor breadcrumbs remain)');
  ok(!indexHtml.includes('header-context-chip') && !indexHtml.includes('header-git-chip'), 'crowded header chips removed');
  ok(appSrc.includes("setGlobalLoading(true, `Deleting ${repoName}…`)"), 'delete repo shows global spinner');
  ok(appSrc.includes("setGlobalLoading(true, 'Committing & pushing…')"), 'commit & push shows global spinner');
  ok(appSrc.includes('busyStatusHtml(\'Loading push preview…\')'), 'push preview loads with spinner');
  ok(indexHtml.includes('id="publish-status"') && indexHtml.includes('id="push-modal-busy"'), 'publish/push modals have busy UI slots');
  ok(appSrc.includes("api('/api/ui-preferences'") && appSrc.includes('coverage_inline_enabled'), 'coverage inline pref saved to ui-preferences.json');
  const themesSrc = fs.readFileSync(path.join(ROOT, 'static/themes.js'), 'utf8');
  ok(themesSrc.includes("'/api/ui-preferences'") && themesSrc.includes('theme_id'), 'color theme saved to ui-preferences.json');
  ok(!themesSrc.includes('localStorage.setItem(THEME_KEY'), 'theme no longer stored in localStorage');
  ok(themesSrc.includes("solarized: 'offwhite'"), 'removed light themes migrate to offwhite');
  ok(appSrc.includes('function navigationBusyHtml') && appSrc.includes('ij-nav-busy'), 'navigation busy uses styled pill indicator');
  ok(appSrc.includes('function stopNavigationBusyIndicator') && appSrc.includes('showNavigationResult'), 'navigation result clears spinner');
  ok(langSrc.includes('definitionInflight') && langSrc.includes('reportNoDefinition'), 'definition lookup deduped and guards false no-def');
  ok(langSrc.includes('definitionRequestSeq') && langSrc.includes('resetDefinitionInflight'), 'definition lookup clears poisoned inflight after failure');
  ok(appSrc.includes('runWithNavigationBusy,'), 'navigation busy helper wired to editor features');
  ok(langSrc.includes('withNavigationBusy') && langSrc.includes('Go to definition…'), 'definition lookup shows nav spinner');
  ok(langSrc.includes('Finding usages…'), 'find usages shows nav spinner');
  ok(langSrc.includes('Renaming…'), 'rename shows nav spinner');
  ok(appSrc.includes("treeContextMenuItem('rename', 'Rename…')"), 'file tree context menu has rename');
  ok(appSrc.includes('/workspace/rename-path') && appSrc.includes('renameTreePath'), 'file tree rename wired to API');
  ok(appSrc.includes('plan_only: true'), 'file tree rename plans symbol edits before move');
  ok(appSrc.includes('skipSymbolEdits'), 'symbol rename skips duplicate file-tree symbol pass');
  ok(langSrc.includes('REFERENCES_FETCH_TIMEOUT_MS'), 'find usages has fetch timeout');
  ok(langSrc.includes('referencesCache'), 'find usages caches recent lookups');
  const runFindUsagesBody = extractFunctionBody(langSrc, 'runFindUsages');
  ok(runFindUsagesBody.includes('loading: true'), 'find usages opens panel immediately while searching');
  ok(appSrc.includes('javaReferencesLoading'), 'find usages panel has loading state');
  ok(appSrc.includes('ij-references-file'), 'find usages shows file and line in structured rows');
  ok(appSrc.includes('scrollTop = 0'), 'find usages resets scroll position on each search');
  ok(appSrc.includes('showRenamePrompt') && appSrc.includes('rename-prompt-submit'), 'rename uses styled modal dialog');
  ok(appSrc.includes('resolveWorkspaceEditPath'), 'rename edits resolve tab paths consistently');
}

function testAppPerformanceGuards() {
  const appSrc = fs.readFileSync(path.join(STATIC, 'app.js'), 'utf8');
  const contentHandler = extractEditorContentHandler(appSrc);

  ok(
    appSrc.includes('function scheduleRenderTabs')
      && appSrc.includes('function scheduleEditorContentSync')
      && appSrc.includes('function markActiveTabDirty'),
    'editor content sync + tab render are debounced helpers',
  );
  ok(
    contentHandler.includes('scheduleEditorContentSync()')
      && contentHandler.includes('scheduleRenderTabs()')
      && !contentHandler.includes('state.tabContents.set(state.activeTab, state.editor.getValue())'),
    'keystroke path debounces getValue tab sync',
  );
  ok(
    contentHandler.includes('if (state.buildTasksPanelOpen) scheduleBuildTasksRefresh()'),
    'pom/build file edits skip build-tasks refresh when panel closed',
  );
  ok(
    contentHandler.includes('scheduleTestRunDecorations()')
      && !contentHandler.includes('applyTestRunDecorations()'),
    'Java test gutter decorations debounced off keystroke path',
  );
  ok(
    appSrc.includes('function scheduleCoverageClear')
      && contentHandler.includes('scheduleCoverageClear()'),
    'coverage gutter clear debounced off keystroke path',
  );
  ok(
    !appSrc.includes('scheduleAllJavaDiagnosticsOnTypingPause')
      && !contentHandler.includes('scheduleAllJavaDiagnostics()'),
    'keystroke path does not schedule all-tab Java diagnostics',
  );
  ok(
    contentHandler.includes('scheduleDiagnostics()'),
    'keystroke path schedules active-tab diagnostics only',
  );
}

function testInlinePerformanceBenchmarks(win) {
  const h = win.ReaperLang.inlineTestHelpers?.();
  ok(typeof h?.localInlineSuggestion === 'function', 'perf bench: inlineTestHelpers available');

  const bigJava = makeLargeJavaBody(700);
  const javaPath = 'src/App.java';
  const javaPrefix = '    Sy';
  const javaLine = 702;

  const fastMs = benchMs(() => {
    h.localInlineSuggestion(javaPath, javaPrefix, bigJava, javaLine, 17, 5, null, { fast: true });
  }, 800);
  const fullMs = benchMs(() => {
    h.localInlineSuggestion(javaPath, javaPrefix, bigJava, javaLine, 17, 5, null, { fast: false });
  }, 800);

  okPerfUnder(fastMs, 120, 'fast local inline on 700-method Java file (800 iter)');
  ok(fastMs < fullMs * 1.35, `fast local inline faster than full scan (fast=${fastMs.toFixed(1)}ms full=${fullMs.toFixed(1)}ms)`);

  const scopeSkipMs = benchMs(() => {
    h.scopeIdentifierInlineSuffix(bigJava, javaPath, javaLine, 'S');
  }, 400);
  okPerfUnder(scopeSkipMs, 25, '1-char scope scan skipped on large file (400 iter)');

  const scopeFullMs = benchMs(() => {
    h.scopeIdentifierInlineSuffix(bigJava, javaPath, javaLine, 'Sy');
  }, 120);
  ok(scopeFullMs > scopeSkipMs, `2-char scope scan costs more than 1-char skip (1c=${scopeSkipMs.toFixed(1)}ms 2c=${scopeFullMs.toFixed(1)}ms)`);

  const routeMs = benchMs(() => {
    for (const [filePath] of LANG_PATH_FIXTURES) {
      h.shouldRouteInlineToAi(filePath, '    ', 'line\n    \nnext\n', 2);
    }
  }, 400);
  okPerfUnder(routeMs, 160, 'shouldRouteInlineToAi all languages empty line (400 iter)');

  const markupPaths = LANG_PATH_FIXTURES.filter(([p]) => h.isMarkupOrConfigPath(p));
  ok(markupPaths.length >= 10, 'markup/config paths covered for perf bench');

  const markupStormMs = benchMs(() => {
    for (const [filePath] of markupPaths) {
      h.shouldRouteInlineToAi(filePath, '    ', '# Title\n\n    \n- item\n', 3);
      h.buildInlineItems(
        { getLineContent: () => '    ' },
        { lineNumber: 3, column: 5 },
        '    ',
        '- next',
      );
    }
  }, 600);
  okPerfUnder(markupStormMs, 140, 'markup empty-line route + inline items (600 iter)');

  const pomItems = h.buildInlineItems(
    { getLineContent: () => '  ' },
    { lineNumber: 2, column: 3 },
    '  ',
    '<dependency>',
  );
  ok(
    pomItems.items[0]?.filterText === '  <dependency>',
    'pom.xml perf fixture: filterText includes indent',
  );

  const indexGateMs = benchMs(() => {
    h.shouldFetchIndexCompletions('    S', 'S', javaPath);
  }, 2000);
  okPerfUnder(indexGateMs, 30, 'single-char index gate check is cheap (2000 iter)');
}

function testPerformanceRegression(win) {
  testInlinePerformanceGuards();
  testAppPerformanceGuards();
  const appSrc = fs.readFileSync(path.join(STATIC, 'app.js'), 'utf8');
  testJavaDiagnosticsThrottling(appSrc, win);
  testJavaJavacInflightGuards(appSrc);
  testInlinePerformanceBenchmarks(win);
}

async function main() {
  console.log('Reaper editor regression suite');
  console.log(`Root: ${ROOT}`);

  section('JavaScript syntax');
  for (const file of JS_FILES) syntaxCheck(file);

  section('Static scans');
  scanForMalformedIf('monaco-languages.js');
  scanForMalformedIf('app.js');
  testRepoPickerUnregisterFlow();
  testHiddenRepoRestoreFlow();
  testLongRunningTaskBusyUi();

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

  section('Language coverage');
  ok(PROVIDER_LANG_FIXTURES.length === ALL_EDITOR_LANGS.length, 'provider fixtures match ALL_EDITOR_LANGS count');
  for (const langId of ALL_EDITOR_LANGS) {
    const fixture = PROVIDER_LANG_FIXTURES.find(([, id]) => id === langId);
    ok(!!fixture?.[0], `${langId}: covered by provider fixture`);
  }
  const pathDetectable = new Set(LANG_PATH_FIXTURES.map(([, langId]) => langId));
  for (const langId of ALL_EDITOR_LANGS) {
    if (langId === 'c') continue;
    ok(pathDetectable.has(langId), `${langId}: covered by langForPath fixture`);
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
    getAiInlineProviderAvailable: () => false,
    getGeminiConfigured: () => false,
    getCursorInlineAvailable: () => false,
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

  section('Test coverage gutter regression');
  testTestCoverageGutterRegression(fs.readFileSync(path.join(STATIC, 'app.js'), 'utf8'));

  section('Inline completion regression');
  testInlineCompletionRegression(win);
  testLocalJavaImportQuickFix(win);

  section('Java compiler error regression');
  testJavaCompilerErrorRegression(fs.readFileSync(path.join(STATIC, 'app.js'), 'utf8'));

  section('Terminal compile link regression');
  testTerminalCompileLinkRegression(fs.readFileSync(path.join(STATIC, 'app.js'), 'utf8'));

  section('Terminal xterm load + input regression');
  {
    const appSrc = fs.readFileSync(path.join(STATIC, 'app.js'), 'utf8');
    const indexHtml = fs.readFileSync(path.join(STATIC, 'index.html'), 'utf8');
    const terminalHarness = await import(path.join(ROOT, 'scripts/lib/terminal-harness.mjs'));
    terminalHarness.testTerminalStaticRegression(appSrc, indexHtml, ok);
    terminalHarness.testTerminalVendorLoadRegression(ok);
    terminalHarness.testTerminalLifecycleRegression(appSrc, ok);
    terminalHarness.testTerminalBuildTaskStreamRegression(appSrc, ok);
  }

  section('Debugger regression');
  {
    const appSrc = fs.readFileSync(path.join(STATIC, 'app.js'), 'utf8');
    const indexHtml = fs.readFileSync(path.join(STATIC, 'index.html'), 'utf8');
    const sessionRs = fs.readFileSync(path.join(ROOT, 'src/workspace/debug/session.rs'), 'utf8');
    const dapRs = fs.readFileSync(path.join(ROOT, 'src/workspace/debug/dap.rs'), 'utf8');
    const adaptersRs = fs.readFileSync(path.join(ROOT, 'src/workspace/debug/adapters.rs'), 'utf8');
    const debugHarness = await import(path.join(ROOT, 'scripts/lib/debug-harness.mjs'));
    debugHarness.testDebugRegression(appSrc, indexHtml, sessionRs, dapRs, adaptersRs, ok);
  }

  section('Structure / AST regression');
  {
    const appSrc = fs.readFileSync(path.join(STATIC, 'app.js'), 'utf8');
    const indexHtml = fs.readFileSync(path.join(STATIC, 'index.html'), 'utf8');
    const apiSrc = fs.readFileSync(path.join(ROOT, 'src/web/api.rs'), 'utf8');
    const astRs = fs.readFileSync(path.join(ROOT, 'src/workspace/ast.rs'), 'utf8');
    const languagesRs = fs.readFileSync(path.join(ROOT, 'src/workspace/languages.rs'), 'utf8');
    const cargoToml = fs.readFileSync(path.join(ROOT, 'Cargo.toml'), 'utf8');
    const astHarness = await import(path.join(ROOT, 'scripts/lib/ast-structure-harness.mjs'));
    astHarness.testAstStructureRegression(
      appSrc,
      indexHtml,
      apiSrc,
      astRs,
      languagesRs,
      cargoToml,
      ok,
    );
  }

  section('Elide package manifest / build tasks regression');
  {
    const appSrc = fs.readFileSync(path.join(STATIC, 'app.js'), 'utf8');
    const monacoSrc = fs.readFileSync(path.join(STATIC, 'monaco-languages.js'), 'utf8');
    const elideHarness = await import(path.join(ROOT, 'scripts/lib/elide-manifest-harness.mjs'));
    elideHarness.testElideManifestRegression(appSrc, monacoSrc, ok);
  }

  section('Compiler settings regression');
  {
    const appSrc = fs.readFileSync(path.join(STATIC, 'app.js'), 'utf8');
    const toolchainSrc = fs.readFileSync(path.join(ROOT, 'src/toolchain.rs'), 'utf8');
    const mavenRsSrc = fs.readFileSync(path.join(ROOT, 'src/maven.rs'), 'utf8');
    const compilerHarness = await import(path.join(ROOT, 'scripts/lib/compiler-settings-harness.mjs'));
    compilerHarness.testCompilerSettingsRegression(appSrc, toolchainSrc, mavenRsSrc, ok);
    compilerHarness.testCompilerStatusSimulation(ok);
  }

  section('Inline AI provider chain regression');
  {
    const appSrc = fs.readFileSync(path.join(STATIC, 'app.js'), 'utf8');
    const langSrc = fs.readFileSync(path.join(STATIC, 'monaco-languages.js'), 'utf8');
    const agentModSrc = fs.readFileSync(path.join(ROOT, 'src/agent/mod.rs'), 'utf8');
    const apiSrc = fs.readFileSync(path.join(ROOT, 'src/web/api.rs'), 'utf8');
    const cursorRsSrc = fs.readFileSync(path.join(ROOT, 'src/web/cursor.rs'), 'utf8');
    const inlineHarness = await import(path.join(ROOT, 'scripts/lib/inline-provider-harness.mjs'));
    inlineHarness.testInlineProviderRegression(appSrc, langSrc, agentModSrc, apiSrc, cursorRsSrc, ok);
    inlineHarness.testCursorSessionRegression(appSrc, cursorRsSrc, ok);
  }

  section('Refactoring regression');
  {
    const appSrc = fs.readFileSync(path.join(STATIC, 'app.js'), 'utf8');
    const langSrc = fs.readFileSync(path.join(STATIC, 'monaco-languages.js'), 'utf8');
    const modRsSrc = fs.readFileSync(path.join(ROOT, 'src/workspace/mod.rs'), 'utf8');
    const symbolsRsSrc = fs.readFileSync(path.join(ROOT, 'src/workspace/symbols.rs'), 'utf8');
    const jdtlsRsSrc = fs.readFileSync(path.join(ROOT, 'src/workspace/jdtls.rs'), 'utf8');
    const refactorHarness = await import(path.join(ROOT, 'scripts/lib/refactor-harness.mjs'));
    refactorHarness.testRefactorRegression(appSrc, langSrc, modRsSrc, symbolsRsSrc, ok, jdtlsRsSrc);
  }

  section('Java navigation discovery regression');
  {
    const classpathRs = fs.readFileSync(path.join(ROOT, 'src/workspace/classpath.rs'), 'utf8');
    const modRs = fs.readFileSync(path.join(ROOT, 'src/workspace/mod.rs'), 'utf8');
    const projectJobsRs = fs.readFileSync(path.join(ROOT, 'src/workspace/project_jobs.rs'), 'utf8');
    const navHarness = await import(path.join(ROOT, 'scripts/lib/navigation-discovery-harness.mjs'));
    navHarness.testNavigationDiscoveryRegression(classpathRs, modRs, projectJobsRs, ok);
  }

  section('Java member completion + tab close regression');
  testJavaMemberCompletionPipelineRegression(fs.readFileSync(path.join(STATIC, 'app.js'), 'utf8'));

  section('Java squiggle regression');
  testJavaSquiggleRegression(fs.readFileSync(path.join(STATIC, 'app.js'), 'utf8'));

  section('Java editor integration (UI → backend)');
  await testJavaEditorUiBackendIntegration(fs.readFileSync(path.join(STATIC, 'app.js'), 'utf8'));

  section('Performance regression (CPU spike guards)');
  testPerformanceRegression(win);

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
