/* global monaco */

(function () {
  const GROOVY_KEYWORDS = [
    'as', 'assert', 'break', 'case', 'catch', 'class', 'const', 'continue', 'def', 'default',
    'do', 'else', 'enum', 'extends', 'false', 'final', 'finally', 'for', 'goto', 'if',
    'implements', 'import', 'in', 'instanceof', 'interface', 'new', 'null', 'package', 'return',
    'static', 'super', 'switch', 'this', 'throw', 'throws', 'trait', 'true', 'try', 'while',
    'var', 'void', 'private', 'protected', 'public', 'abstract', 'synchronized', 'native',
    'strictfp', 'transient', 'volatile', 'with',
  ];

  function langForPath(path) {
    const base = (path.split('/').pop() || '').toLowerCase();
    if (!base) return 'plaintext';

    if (base === 'dockerfile' || base.startsWith('dockerfile.')) return 'dockerfile';
    if (base === 'makefile' || base === 'gnumakefile') return 'makefile';
    if (base === 'cmakelists.txt') return 'cmake';
    if (base.endsWith('.gradle.kts')) return 'kotlin';
    if (base.endsWith('.gradle')) return 'groovy';
    if (base.endsWith('.properties') || base.endsWith('.gradle.properties')) return 'ini';

    const ext = base.includes('.') ? base.slice(base.lastIndexOf('.') + 1) : '';
    const map = {
      rs: 'rust',
      js: 'javascript', mjs: 'javascript', cjs: 'javascript',
      ts: 'typescript', jsx: 'javascript', tsx: 'typescript',
      py: 'python', pyw: 'python',
      go: 'go',
      json: 'json', jsonc: 'json',
      md: 'markdown', mdx: 'markdown',
      html: 'html', htm: 'html',
      css: 'css', scss: 'scss', less: 'less',
      yml: 'yaml', yaml: 'yaml',
      toml: 'toml',
      sh: 'shell', bash: 'shell', zsh: 'shell',
      sql: 'sql',
      xml: 'xml',
      java: 'java',
      groovy: 'groovy', gvy: 'groovy', gy: 'groovy', gsh: 'groovy',
      kt: 'kotlin', kts: 'kotlin',
      gradle: 'groovy',
      c: 'c', h: 'c',
      cpp: 'cpp', cc: 'cpp', cxx: 'cpp', hpp: 'cpp', hh: 'cpp',
      cs: 'csharp',
      rb: 'ruby',
      php: 'php',
      swift: 'swift',
      lua: 'lua',
      r: 'r',
      dart: 'dart',
      vue: 'html',
      svelte: 'html',
      ini: 'ini',
      properties: 'ini',
      dockerfile: 'dockerfile',
      proto: 'protobuf',
      graphql: 'graphql',
      gql: 'graphql',
    };
    return map[ext] || 'plaintext';
  }

  function langLabel(lang) {
    const labels = {
      groovy: 'Groovy', kotlin: 'Kotlin', javascript: 'JavaScript', typescript: 'TypeScript',
      plaintext: 'Plain Text', ini: 'Properties',
    };
    return labels[lang] || (lang.charAt(0).toUpperCase() + lang.slice(1));
  }

  function registerGroovy() {
    if (window.__reaperGroovyRegistered) return;
    window.__reaperGroovyRegistered = true;

    monaco.languages.register({ id: 'groovy', aliases: ['Groovy', 'Gradle'] });

    monaco.languages.setMonarchTokensProvider('groovy', {
      defaultToken: '',
      tokenPostfix: '.groovy',
      keywords: GROOVY_KEYWORDS,
      operators: ['=', '>', '<', '!', '~', '?', ':', '==', '!=', '&&', '||', '++', '--', '+', '-', '*', '/'],
      symbols: /[=><!~?:&|+\-*\/\^%]+/,
      escapes: /\\(?:[abfnrtv\\"']|x[0-9A-Fa-f]{1,4}|u[0-9A-Fa-f]{4}|U[0-9A-Fa-f]{8})/,
      tokenizer: {
        root: [
          [/\/\/.*$/, 'comment'],
          [/\/\*/, 'comment', '@comment'],
          [/"([^"\\]|\\.)*$/, 'string.invalid'],
          [/'([^'\\]|\\.)*$/, 'string.invalid'],
          [/"/, 'string', '@string_dquote'],
          [/'/, 'string', '@string_squote'],
          [/\b\d+\.\d+([eE][\-+]?\d+)?[fFdD]?/, 'number.float'],
          [/\b0[xX][0-9a-fA-F]+/, 'number.hex'],
          [/\b\d+[lL]?/, 'number'],
          [/[{}()\[\]]/, '@brackets'],
          [/@symbols/, { cases: { '@operators': 'operator', '@default': '' } }],
          [/@\s*[a-zA-Z_]\w*/, {
            cases: {
              '@keywords': 'keyword',
              '@default': 'identifier',
            },
          }],
        ],
        comment: [
          [/[^\/*]+/, 'comment'],
          [/\*\//, 'comment', '@pop'],
          [/[\/*]/, 'comment'],
        ],
        string_dquote: [
          [/[^\\"]+/, 'string'],
          [/@escapes/, 'string.escape'],
          [/\\./, 'string.escape.invalid'],
          [/"/, 'string', '@pop'],
        ],
        string_squote: [
          [/[^\\']+/, 'string'],
          [/@escapes/, 'string.escape'],
          [/\\./, 'string.escape.invalid'],
          [/'/, 'string', '@pop'],
        ],
      },
    });

    monaco.languages.setLanguageConfiguration('groovy', {
      comments: { lineComment: '//', blockComment: ['/*', '*/'] },
      brackets: [['{', '}'], ['[', ']'], ['(', ')']],
      autoClosingPairs: [
        { open: '{', close: '}' },
        { open: '[', close: ']' },
        { open: '(', close: ')' },
        { open: '"', close: '"' },
        { open: "'", close: "'" },
      ],
      surroundingPairs: [
        { open: '{', close: '}' },
        { open: '[', close: ']' },
        { open: '(', close: ')' },
        { open: '"', close: '"' },
        { open: "'", close: "'" },
      ],
    });
  }

  const DEF_PREFIXES = [
    'class', 'module', 'interface', 'enum', 'record', 'struct', 'trait', 'mod', 'type', 'object',
    'def', 'fun', 'function', 'fn', 'func', 'const', 'let', 'var',
  ];

  function extractSymbols(content, path) {
    const symbols = [];
    const lines = content.split('\n');
    lines.forEach((line, idx) => {
      const trimmed = line.trim();
      if (!trimmed || trimmed.startsWith('//') || trimmed.startsWith('#') || trimmed.startsWith('*')) return;
      for (const kw of DEF_PREFIXES) {
        const re = new RegExp(`\\b${kw}\\s+([A-Za-z_][\\w]*)`);
        const m = line.match(re);
        if (m) {
          const col = line.indexOf(m[1]) + 1;
          symbols.push({
            name: m[1],
            kind: kw,
            path,
            line: idx + 1,
            column: col,
          });
          break;
        }
      }
      const py = line.match(/^\s*def\s+(?:self\.)?([A-Za-z_]\w*)/);
      if (py) {
        symbols.push({
          name: py[1],
          kind: 'def',
          path,
          line: idx + 1,
          column: line.indexOf(py[1]) + 1,
        });
      }
    });
    return symbols;
  }

  function completionKind(kind) {
    const map = {
      class: monaco.languages.CompletionItemKind.Class,
      interface: monaco.languages.CompletionItemKind.Interface,
      enum: monaco.languages.CompletionItemKind.Enum,
      annotation: monaco.languages.CompletionItemKind.Interface,
      property: monaco.languages.CompletionItemKind.Property,
      value: monaco.languages.CompletionItemKind.Value,
    };
    return map[kind] || monaco.languages.CompletionItemKind.Text;
  }

  async function fetchCompletions(helpers, model, position, prefix) {
    const repo = helpers.getRepo();
    const path = helpers.getActivePath?.() || '';
    if (!repo || !path) return [];

    const q = new URLSearchParams({
      path,
      line: String(position.lineNumber),
      column: String(position.column),
      prefix: prefix || '',
    });
    const items = await helpers.api(`${helpers.repoApi(repo, '/workspace/completions')}?${q}`);
    return Array.isArray(items) ? items : [];
  }

  const definitionCache = new Map();
  const DEF_CACHE_MAX = 512;

  function definitionCacheKey(repo, path, line, column, content) {
    let hash = 2166136261;
    const text = content || '';
    const sample = text.length > 8192 ? text.slice(0, 4096) + text.slice(-4096) : text;
    for (let i = 0; i < sample.length; i += 1) {
      hash ^= sample.charCodeAt(i);
      hash = Math.imul(hash, 16777619);
    }
    return `${repo}:${path}:${line}:${column}:${text.length}:${hash >>> 0}`;
  }

  async function lookupDefinition(helpers, model, position) {
    if (!helpers.repoApi || !helpers.getRepo || !helpers.openFileAt) return null;
    const repo = helpers.getRepo();
    if (!repo) return null;
    const path = helpers.getActivePath?.() || '';
    if (!path) return null;

    const line = position.lineNumber;
    const column = position.column;
    const dirty = helpers.isFileDirty?.(path);
    const content = dirty ? model.getValue() : undefined;
    const cacheKey = definitionCacheKey(repo, path, line, column, content ?? model.getValue());
    const cached = definitionCache.get(cacheKey);
    if (cached) {
      return navigateToDefinition(helpers, cached);
    }

    try {
      const url = helpers.repoApi(repo, '/workspace/definition');
      const hit = dirty
        ? await helpers.api(url, {
            method: 'POST',
            body: JSON.stringify({ path, line, column, content }),
          })
        : await helpers.api(`${url}?${new URLSearchParams({
            path,
            line: String(line),
            column: String(column),
          })}`);
      if (!hit?.path) return null;
      if (definitionCache.size >= DEF_CACHE_MAX) definitionCache.clear();
      definitionCache.set(cacheKey, hit);
      return navigateToDefinition(helpers, hit);
    } catch {
      return null;
    }
  }

  async function navigateToDefinition(helpers, hit) {
    await helpers.openFileAt(hit.path, hit.line || 1, hit.column || 1);
    const editor = helpers.getEditor?.();
    const activeModel = editor?.getModel();
    if (!activeModel) return null;
    const nameLen = (hit.name || 'symbol').length;
    const line = Math.max(1, hit.line || 1);
    const col = Math.max(1, hit.column || 1);
    return {
      uri: activeModel.uri,
      range: new monaco.Range(line, col, line, col + nameLen),
    };
  }

  function setupEditorFeatures(editor, helpers) {
    registerGroovy();

    const langs = new Set([
      'java', 'groovy', 'kotlin', 'rust', 'javascript', 'typescript', 'python', 'go',
      'csharp', 'ruby', 'php', 'swift', 'c', 'cpp', 'shell',
    ]);

    monaco.languages.registerDocumentSymbolProvider(Array.from(langs), {
      provideDocumentSymbols(model) {
        const path = helpers.getActivePath?.() || model.uri.path.replace(/^\//, '');
        return extractSymbols(model.getValue(), path).map((s) => ({
          name: s.name,
          detail: s.kind,
          kind: monaco.languages.SymbolKind.Class,
          range: new monaco.Range(s.line, s.column, s.line, s.column + s.name.length),
          selectionRange: new monaco.Range(s.line, s.column, s.line, s.column + s.name.length),
        }));
      },
    });

    monaco.languages.registerDefinitionProvider(Array.from(langs), {
      provideDefinition(model, position) {
        return lookupDefinition(helpers, model, position);
      },
    });

    monaco.languages.registerCompletionItemProvider(['java', 'kotlin', 'groovy'], {
      triggerCharacters: ['.', '@'],
      async provideCompletionItems(model, position) {
        if (!helpers.repoApi || !helpers.getRepo) return { suggestions: [] };
        const repo = helpers.getRepo();
        const path = helpers.getActivePath?.() || '';
        if (!repo || !path) return { suggestions: [] };

        const word = model.getWordUntilPosition(position);
        const prefix = word.word || '';
        if (!prefix && model.getValueInRange(new monaco.Range(position.lineNumber, 1, position.lineNumber, position.column)).slice(-1) !== '@') {
          return { suggestions: [] };
        }

        try {
          const items = await fetchCompletions(helpers, model, position, prefix);
          if (!items.length) return { suggestions: [] };

          const range = new monaco.Range(
            position.lineNumber,
            word.startColumn,
            position.lineNumber,
            word.endColumn,
          );

          return {
            suggestions: items.map((item) => ({
              label: item.label,
              kind: completionKind(item.kind),
              detail: item.detail || undefined,
              insertText: item.label,
              range,
            })),
          };
        } catch {
          return { suggestions: [] };
        }
      },
    });

    monaco.languages.registerCompletionItemProvider(['ini', 'yaml'], {
      triggerCharacters: ['.', '=', ':'],
      async provideCompletionItems(model, position) {
        if (!helpers.repoApi || !helpers.getRepo) return { suggestions: [] };
        const path = helpers.getActivePath?.() || '';
        if (!path) return { suggestions: [] };
        const lower = path.toLowerCase();
        if (!lower.endsWith('.properties') && !lower.endsWith('.yml') && !lower.endsWith('.yaml')) {
          return { suggestions: [] };
        }

        const word = model.getWordUntilPosition(position);
        const linePrefix = model.getValueInRange(
          new monaco.Range(position.lineNumber, 1, position.lineNumber, position.column),
        );
        const prefix = word.word || linePrefix.split(/[=:#]/).pop()?.trim() || '';

        try {
          const items = await fetchCompletions(helpers, model, position, prefix);
          if (!items.length) return { suggestions: [] };

          const startCol = Math.max(1, position.column - prefix.length);
          const range = new monaco.Range(
            position.lineNumber,
            startCol,
            position.lineNumber,
            position.column,
          );

          return {
            suggestions: items.map((item) => ({
              label: item.label,
              kind: completionKind(item.kind),
              detail: item.detail || undefined,
              documentation: item.detail ? { value: item.detail } : undefined,
              insertText: item.label,
              range,
            })),
          };
        } catch {
          return { suggestions: [] };
        }
      },
    });

    monaco.languages.registerDocumentFormattingEditProvider(Array.from(langs).concat(['json', 'html', 'css', 'scss', 'less', 'yaml', 'markdown', 'xml', 'ini']), {
      async provideDocumentFormattingEdits(model) {
        if (!helpers.repoApi || !helpers.getRepo) return [];
        const repo = helpers.getRepo();
        const path = helpers.getActivePath?.() || '';
        if (!repo || !path) return [];
        try {
          const res = await helpers.api(helpers.repoApi(repo, '/workspace/format'), {
            method: 'POST',
            body: JSON.stringify({ path, content: model.getValue() }),
          });
          if (!res?.content || res.content === model.getValue()) return [];
          const fullRange = model.getFullModelRange();
          return [{ range: fullRange, text: res.content }];
        } catch {
          return [];
        }
      },
    });

    monaco.editor.registerEditorOpener({
      openCodeEditor(_source, resource, selection) {
        if (!helpers.openFileAt) return false;
        const path = decodeURIComponent(resource.path.replace(/^\//, ''));
        helpers.openFileAt(path, selection?.startLineNumber || 1, selection?.startColumn || 1);
        return true;
      },
    });

    editor.addAction({
      id: 'reaper.formatDocument',
      label: 'Format Document',
      keybindings: [monaco.KeyMod.Shift | monaco.KeyMod.Alt | monaco.KeyCode.KeyF],
      run: async () => {
        await editor.getAction('editor.action.formatDocument')?.run();
      },
    });

    editor.addAction({
      id: 'reaper.goToDefinition',
      label: 'Go to Definition',
      keybindings: [monaco.KeyCode.F12],
      run: async () => {
        const pos = editor.getPosition();
        if (!pos || !helpers.repoApi || !helpers.getRepo) return;
        const repo = helpers.getRepo();
        const path = helpers.getActivePath?.() || '';
        if (!repo || !path) return;
        try {
          const q = new URLSearchParams({
            path,
            line: String(pos.lineNumber),
            column: String(pos.column),
          });
          const hit = await helpers.api(`${helpers.repoApi(repo, '/workspace/definition')}?${q}`);
          if (!hit?.path) {
            helpers.toast?.('No definition found', 'info');
            return;
          }
          await helpers.openFileAt(hit.path, hit.line, hit.column);
        } catch {
          helpers.toast?.('No definition found', 'info');
        }
      },
    });

    editor.addAction({
      id: 'reaper.goToSymbol',
      label: 'Go to Symbol in File',
      keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyMod.Shift | monaco.KeyCode.KeyO],
      run: () => editor.getAction('editor.action.quickOutline')?.run(),
    });

    editor.onDidChangeModelContent(() => {
      const lang = editor.getModel()?.getLanguageId() || 'plaintext';
      helpers.setLanguageStatus?.(langLabel(lang));
    });
  }

  function isDiagnosablePath(path) {
    if (!path || path.startsWith('.reaper/')) return false;
    const base = (path.split('/').pop() || '').toLowerCase();
    if (!base || base.startsWith('.')) return false;
    if (base === 'dockerfile' || base.startsWith('dockerfile.')) return true;
    if (base === 'makefile' || base === 'gnumakefile' || base === 'cmakelists.txt') return true;
    return langForPath(path) !== 'plaintext';
  }

  window.ReaperLang = {
    langForPath,
    langLabel,
    isDiagnosablePath,
    registerGroovy,
    setupEditorFeatures,
    extractSymbols,
  };
})();
