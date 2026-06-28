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

  function compilerToolIdsForPath(path) {
    const lower = (path || '').replace(/\\/g, '/').toLowerCase();
    const base = lower.split('/').pop() || '';
    if (base === 'dockerfile' || base.startsWith('dockerfile.')) return ['bash'];
    if (base === 'makefile' || base === 'gnumakefile') return ['bash'];
    if (base === 'cmakelists.txt') return ['bash'];
    if (base === 'gemfile' || base.endsWith('.rb')) return ['ruby', 'bundle'];
    const lang = langForPath(path);
    const map = {
      java: ['java'],
      kotlin: ['kotlin'],
      groovy: ['groovy'],
      python: ['python'],
      ruby: ['ruby', 'bundle'],
      rust: ['rustc', 'cargo'],
      go: ['go'],
      javascript: ['node'],
      typescript: ['tsc', 'node'],
      php: ['php'],
      csharp: ['csc'],
      swift: ['swiftc'],
      c: ['clang', 'gcc'],
      cpp: ['clang', 'gcc'],
      shell: ['bash'],
      lua: ['luac'],
      dart: ['dart'],
      json: ['jsonlint'],
      jsonc: ['jsonlint'],
      yaml: ['yamllint'],
    };
    return map[lang] || [];
  }

  function compilerLabelsForPath(path) {
    const ids = compilerToolIdsForPath(path);
    const labels = {
      java: 'Java', kotlin: 'Kotlin', groovy: 'Groovy', python: 'Python',
      ruby: 'Ruby', bundle: 'Bundler', rustc: 'rustc', cargo: 'cargo', go: 'Go',
      node: 'Node', tsc: 'tsc', php: 'PHP', clang: 'clang', gcc: 'gcc',
      swiftc: 'swiftc', luac: 'luac', csc: 'csc', dart: 'dart', bash: 'bash',
      yamllint: 'yamllint', jsonlint: 'jsonlint', ajv: 'ajv',
    };
    return ids.map((id) => labels[id] || id).join(', ');
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
      method: monaco.languages.CompletionItemKind.Method,
      module: monaco.languages.CompletionItemKind.Module,
      struct: monaco.languages.CompletionItemKind.Struct,
      keyword: monaco.languages.CompletionItemKind.Keyword,
    };
    return map[kind] || monaco.languages.CompletionItemKind.Text;
  }

  const ALL_EDITOR_LANGS = [
    'java', 'kotlin', 'groovy', 'rust', 'javascript', 'typescript', 'python', 'go',
    'csharp', 'ruby', 'php', 'swift', 'c', 'cpp', 'shell', 'lua', 'dart', 'r', 'sql',
    'html', 'css', 'scss', 'less', 'json', 'markdown', 'xml', 'yaml', 'toml', 'ini',
    'dockerfile', 'makefile', 'cmake', 'protobuf', 'graphql', 'plaintext',
  ];

  const CLIENT_KEYWORDS = {
    rust: ['fn', 'let', 'mut', 'pub', 'struct', 'enum', 'impl', 'trait', 'match', 'use', 'async', 'await'],
    python: ['def', 'class', 'import', 'from', 'if', 'elif', 'else', 'for', 'while', 'with', 'return', 'async', 'await'],
    go: ['func', 'package', 'import', 'type', 'struct', 'interface', 'if', 'for', 'return', 'go', 'chan'],
    javascript: ['function', 'const', 'let', 'var', 'class', 'import', 'export', 'async', 'await', 'return', 'if', 'for'],
    typescript: ['interface', 'type', 'enum', 'namespace', 'readonly', 'declare', 'function', 'const', 'async', 'await'],
    java: ['class', 'interface', 'enum', 'public', 'private', 'protected', 'static', 'final', 'void', 'return', 'import'],
    kotlin: ['fun', 'val', 'var', 'class', 'object', 'interface', 'when', 'suspend', 'data', 'sealed'],
    groovy: ['def', 'class', 'import', 'package', 'return', 'if', 'for', 'while'],
    ruby: ['def', 'class', 'module', 'end', 'require', 'include', 'attr_reader', 'attr_writer'],
    php: ['function', 'class', 'namespace', 'use', 'public', 'private', 'protected', 'return', 'if', 'foreach'],
    csharp: ['class', 'namespace', 'using', 'public', 'private', 'async', 'await', 'var', 'record', 'interface'],
    swift: ['func', 'var', 'let', 'class', 'struct', 'enum', 'protocol', 'extension', 'import', 'guard'],
    c: ['int', 'char', 'void', 'struct', 'enum', 'typedef', 'static', 'const', 'return', 'if', 'for'],
    cpp: ['class', 'namespace', 'template', 'typename', 'constexpr', 'virtual', 'public', 'private', 'override'],
    shell: ['if', 'then', 'else', 'fi', 'for', 'do', 'done', 'function', 'export', 'local', 'echo'],
    lua: ['function', 'local', 'if', 'then', 'else', 'end', 'for', 'while', 'return'],
    dart: ['class', 'void', 'Future', 'async', 'await', 'import', 'extends', 'implements', 'factory'],
    sql: ['SELECT', 'FROM', 'WHERE', 'INSERT', 'UPDATE', 'DELETE', 'CREATE', 'JOIN', 'ORDER', 'GROUP'],
    yaml: ['apiVersion', 'kind', 'metadata', 'spec', 'name', 'labels', 'jobs', 'steps', 'uses', 'run', 'on'],
    toml: ['true', 'false'],
    dockerfile: ['FROM', 'RUN', 'CMD', 'COPY', 'WORKDIR', 'ENV', 'EXPOSE', 'ENTRYPOINT'],
    makefile: ['ifeq', 'endif', 'include', 'export', '.PHONY'],
    cmake: ['cmake_minimum_required', 'project', 'add_executable', 'find_package', 'target_link_libraries'],
    html: ['div', 'span', 'script', 'style', 'head', 'body', 'meta', 'link', 'button', 'input', 'form'],
    css: ['display', 'margin', 'padding', 'color', 'background', 'flex', 'grid', 'position'],
    scss: ['@import', '@mixin', '@include', '@media'],
    protobuf: ['message', 'enum', 'service', 'rpc', 'package', 'import'],
    graphql: ['query', 'mutation', 'type', 'interface', 'input', 'schema'],
    ini: ['spring.application.name', 'server.port', 'logging.level'],
  };

  function clientKeywordsForPath(path) {
    const lang = langForPath(path);
    return CLIENT_KEYWORDS[lang] || [];
  }

  function buildCompletionRange(model, position, prefix) {
    const word = model.getWordUntilPosition(position);
    const p = prefix || word.word || '';
    const startCol = p.length
      ? Math.max(1, position.column - p.length)
      : word.startColumn;
    return new monaco.Range(position.lineNumber, startCol, position.lineNumber, position.column);
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

    const langs = new Set(ALL_EDITOR_LANGS);

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

    monaco.languages.registerCompletionItemProvider(ALL_EDITOR_LANGS, {
      triggerCharacters: ['.', '@', ':', '-', '<', '"', '/', '#', '*', '='],
      async provideCompletionItems(model, position) {
        const path = helpers.getActivePath?.() || '';
        if (!path) return { suggestions: [] };

        const word = model.getWordUntilPosition(position);
        const linePrefix = model.getValueInRange(
          new monaco.Range(position.lineNumber, 1, position.lineNumber, position.column),
        );
        const prefix = word.word || linePrefix.split(/[=:#.\s]+/).pop() || '';
        const range = buildCompletionRange(model, position, prefix);

        const seen = new Set();
        const suggestions = [];

        function add(label, kind, detail) {
          if (!label || seen.has(label)) return;
          seen.add(label);
          suggestions.push({
            label,
            kind: completionKind(kind),
            detail: detail || undefined,
            insertText: label,
            range,
          });
        }

        for (const kw of clientKeywordsForPath(path)) {
          if (!prefix || kw.toLowerCase().startsWith(prefix.toLowerCase())) {
            add(kw, 'keyword', 'keyword');
          }
        }

        if (helpers.repoApi && helpers.getRepo) {
          try {
            const items = await fetchCompletions(helpers, model, position, prefix);
            for (const item of items) {
              add(item.label, item.kind, item.detail);
              if (suggestions.length >= 80) break;
            }
          } catch {
            /* API unavailable — local keywords still shown */
          }
        }

        return { suggestions };
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
          if (typeof res?.content !== 'string' || res.content === model.getValue()) return [];
          const fullRange = model.getFullModelRange();
          return [{ range: fullRange, text: res.content }];
        } catch (e) {
          helpers.toast?.(e?.message || 'Format failed', 'error');
          throw e;
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
    compilerToolIdsForPath,
    compilerLabelsForPath,
    isDiagnosablePath,
    registerGroovy,
    setupEditorFeatures,
    extractSymbols,
  };
})();
