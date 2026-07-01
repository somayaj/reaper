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

  function inlineJavaLevel(helpers) {
    const ctx = helpers.getLanguageContext?.();
    if (ctx?.java_level) return ctx.java_level;
    return helpers.getJavaLanguageLevel?.() ?? 17;
  }

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

  function isSpringConfigFile(path) {
    const base = (path.split('/').pop() || '').toLowerCase();
    if (base.endsWith('.properties')) {
      return !base.endsWith('gradle.properties') && !base.endsWith('.gradle.properties');
    }
    return base.endsWith('.yml') || base.endsWith('.yaml');
  }

  function springConfigKeyPrefix(path, linePrefix, content, lineNumber, column) {
    const line = String(linePrefix || '');
    const trimmed = line.split('#')[0].trimEnd();
    if (isSpringConfigFile(path) && path.toLowerCase().endsWith('.properties')) {
      if (trimmed.includes('=')) {
        return '';
      }
      return trimmed.trim();
    }
    if (!isSpringConfigFile(path)) return '';
    const col = Math.max(0, (column || 1) - 1);
    const upto = line.slice(0, Math.min(col, line.length));
    const beforeColon = upto.split(':')[0].trim();
    if (!beforeColon || beforeColon.startsWith('#')) return '';
    const lines = String(content || '').split(/\r?\n/);
    const idx = Math.max(0, (lineNumber || 1) - 1);
    const current = lines[idx] || '';
    const currentIndent = current.match(/^[\t ]*/)?.[0]?.length || 0;
    const segments = [];
    if (beforeColon && !beforeColon.includes(':')) {
      segments.push(beforeColon);
    }
    let needIndent = currentIndent;
    for (let i = idx - 1; i >= 0; i -= 1) {
      const row = lines[i];
      if (!row.trim() || row.trim().startsWith('#')) continue;
      const indent = row.match(/^[\t ]*/)?.[0]?.length || 0;
      if (indent < needIndent) {
        const key = row.split('#')[0].split(':')[0].trim();
        if (key) segments.unshift(key);
        needIndent = indent;
      }
      if (needIndent === 0) break;
    }
    return segments.join('.');
  }

  function completionSuffixFromLabel(label, prefix) {
    const p = prefix || '';
    if (!label || !p) return '';
    if (label.startsWith(p)) return label.slice(p.length);
    if (label.toLowerCase().startsWith(p.toLowerCase())) {
      return label.slice(p.length);
    }
    return '';
  }

  function springConfigLocalInline(path, linePrefix, content, lineNumber, column) {
    if (!isSpringConfigFile(path)) return '';
    const keyPrefix = springConfigKeyPrefix(path, linePrefix, content, lineNumber, column);
    if (!keyPrefix || keyPrefix.length < 1) return '';
    const lower = keyPrefix.toLowerCase();
    for (const kw of clientKeywordsForPath(path)) {
      if (kw.toLowerCase().startsWith(lower) && kw.length > keyPrefix.length) {
        return kw.slice(keyPrefix.length);
      }
    }
    return '';
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
      const rb = line.match(/^\s*def\s+([A-Za-z_]\w*)/);
      if (rb) {
        symbols.push({ name: rb[1], kind: 'method', path, line: idx + 1, column: line.indexOf(rb[1]) + 1 });
      }
      const jsFn = line.match(/\bfunction\s+([A-Za-z_$]\w*)/);
      if (jsFn) {
        symbols.push({ name: jsFn[1], kind: 'function', path, line: idx + 1, column: line.indexOf(jsFn[1]) + 1 });
      }
      const goFn = line.match(/^\s*func\s+(?:\([^)]*\)\s+)?([A-Za-z_]\w*)/);
      if (goFn) {
        symbols.push({ name: goFn[1], kind: 'func', path, line: idx + 1, column: line.indexOf(goFn[1]) + 1 });
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
      field: monaco.languages.CompletionItemKind.Field,
      property: monaco.languages.CompletionItemKind.Property,
      value: monaco.languages.CompletionItemKind.Value,
      method: monaco.languages.CompletionItemKind.Method,
      module: monaco.languages.CompletionItemKind.Module,
      struct: monaco.languages.CompletionItemKind.Struct,
      keyword: monaco.languages.CompletionItemKind.Keyword,
      snippet: monaco.languages.CompletionItemKind.Snippet,
      variable: monaco.languages.CompletionItemKind.Variable,
    };
    return map[kind] || monaco.languages.CompletionItemKind.Text;
  }

  const ALL_EDITOR_LANGS = [
    'java', 'kotlin', 'groovy', 'rust', 'javascript', 'typescript', 'python', 'go',
    'csharp', 'ruby', 'php', 'swift', 'c', 'cpp', 'shell', 'lua', 'dart', 'r', 'sql',
    'html', 'css', 'scss', 'less', 'json', 'markdown', 'xml', 'yaml', 'toml', 'ini',
    'dockerfile', 'makefile', 'cmake', 'protobuf', 'graphql', 'plaintext',
  ];

  /** Reaper uses one Monaco model + setModelLanguage per tab (inmemory:// URIs). */
  const REAPER_DOC_SELECTOR = ALL_EDITOR_LANGS;
  const REAPER_COMPLETION_REV = '253';
  let reaperDotCompletionHandler = null;

  /** WKWebView Monaco builds may omit CodeActionKind — use string fallbacks. */
  function reaperCodeActionKind(name) {
    const kind = monaco.languages.CodeActionKind?.[name];
    if (kind != null) return kind;
    const fallbacks = { QuickFix: 'quickfix', Empty: '' };
    return fallbacks[name] ?? String(name).toLowerCase();
  }

  function codeActionOnlyWantsQuickFix(only) {
    if (!only) return true;
    const quickFix = reaperCodeActionKind('QuickFix');
    const empty = reaperCodeActionKind('Empty');
    if (typeof only.contains === 'function') {
      return only.contains(quickFix) || only.contains(empty);
    }
    return only === quickFix || only === empty;
  }

  function editorContent(ed, model) {
    return ed?._reaperContent ?? model.getValue();
  }

  function completionDebugEnabled() {
    try {
      return localStorage.getItem('reaper-complete-debug') === '1';
    } catch {
      return false;
    }
  }

  function completionTriggerLabel(context) {
    const k = context?.triggerKind;
    const T = monaco.languages.CompletionTriggerKind;
    if (k === T.Invoke) return 'Invoke';
    if (k === T.TriggerCharacter) return 'TriggerChar';
    if (k === T.TriggerForIncompleteCompletions) return 'Incomplete';
    return `kind:${k ?? '?'}`;
  }

  function completionDebug(helpers, parts, { warn = false } = {}) {
    if (!completionDebugEnabled()) return;
    const msg = parts.filter(Boolean).join(' · ');
    console.log(`[Reaper complete] ${msg}`);
    helpers.setCompleteDebugStatus?.(`[complete] ${msg}`);
  }

  const CLIENT_KEYWORDS = {
    rust: ['fn', 'let', 'mut', 'pub', 'struct', 'enum', 'impl', 'trait', 'match', 'use', 'async', 'await'],
    python: ['def', 'class', 'import', 'from', 'if', 'elif', 'else', 'for', 'while', 'with', 'return', 'async', 'await'],
    go: ['func', 'package', 'import', 'type', 'struct', 'interface', 'if', 'for', 'return', 'go', 'chan'],
    javascript: ['function', 'const', 'let', 'var', 'class', 'import', 'export', 'async', 'await', 'return', 'if', 'else', 'for', 'while', 'do', 'switch', 'case', 'try', 'catch', 'finally', 'throw', 'break', 'continue', 'new', 'this'],
    typescript: ['interface', 'type', 'enum', 'namespace', 'readonly', 'declare', 'function', 'const', 'async', 'await'],
    java: ['if', 'else', 'for', 'while', 'do', 'switch', 'case', 'try', 'catch', 'finally', 'throw', 'return', 'new', 'this', 'class', 'interface', 'enum', 'public', 'private', 'protected', 'static', 'final', 'void', 'import', 'package', 'extends', 'implements'],
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
    yaml: [
      'apiVersion', 'kind', 'metadata', 'spec', 'name', 'labels', 'jobs', 'steps', 'uses', 'run', 'on',
      'spring', 'server', 'logging', 'management', 'datasource',
    ],
    toml: ['true', 'false'],
    dockerfile: ['FROM', 'RUN', 'CMD', 'COPY', 'WORKDIR', 'ENV', 'EXPOSE', 'ENTRYPOINT'],
    makefile: ['ifeq', 'endif', 'include', 'export', '.PHONY'],
    cmake: ['cmake_minimum_required', 'project', 'add_executable', 'find_package', 'target_link_libraries'],
    html: ['div', 'span', 'script', 'style', 'head', 'body', 'meta', 'link', 'button', 'input', 'form'],
    css: ['display', 'margin', 'padding', 'color', 'background', 'flex', 'grid', 'position'],
    scss: ['@import', '@mixin', '@include', '@media'],
    protobuf: ['message', 'enum', 'service', 'rpc', 'package', 'import'],
    graphql: ['query', 'mutation', 'type', 'interface', 'input', 'schema'],
    ini: [
      'spring.application.name', 'spring.profiles.active', 'server.port',
      'server.servlet.context-path', 'logging.level.root', 'logging.level',
      'logging.level.org.springframework', 'management.endpoints.web.exposure.include',
      'spring.datasource.url', 'spring.datasource.username', 'spring.datasource.password',
      'spring.datasource.driver-class-name', 'spring.jpa.hibernate.ddl-auto',
      'spring.jpa.show-sql', 'spring.main.banner-mode',
    ],
  };

  function clientKeywordsForPath(path) {
    const lang = langForPath(path);
    return CLIENT_KEYWORDS[lang] || [];
  }

  function buildCompletionRange(model, position, prefix, linePrefix) {
    const memberCtx = linePrefix ? dotQualifierFromLinePrefix(linePrefix) : null;
    if (memberCtx) {
      const dotIdx = linePrefix.lastIndexOf('.');
      const startCol = dotIdx >= 0
        ? dotIdx + 2
        : Math.max(1, position.column - memberCtx.memberPrefix.length);
      return new monaco.Range(
        position.lineNumber,
        startCol,
        position.lineNumber,
        position.column,
      );
    }
    const word = model.getWordUntilPosition(position);
    const p = prefix || word.word || '';
    const startCol = p.length
      ? Math.max(1, position.column - p.length)
      : word.startColumn;
    return new monaco.Range(position.lineNumber, startCol, position.lineNumber, position.column);
  }

  const MIN_AUTOCOMPLETE_CHARS = 1;
  const MAX_INLINE_LINES = 15;
  const AUTOCOMPLETE_TRIGGER_RE = /[.@:<#"=/\\\-]$/;

  function capInlineText(text) {
    const lines = String(text ?? '').split(/\r?\n/);
    if (lines.length <= MAX_INLINE_LINES) return lines.join('\n');
    return lines.slice(0, MAX_INLINE_LINES).join('\n');
  }

  /** Inline ghost text: always try while editing (server may return empty). */
  function inlineTypingReady() {
    return true;
  }

  function stripSnippetMarkers(text) {
    return String(text || '')
      .replace(/\$\{\d+:(.*?)}/g, '$1')
      .replace(/\$\d+/g, '');
  }

  function hasCompleteControlKeyword(trimmed) {
    if (CONTROL_KEYWORD_PREFIXES.some((kw) => lineEndsWithKeyword(trimmed, kw))) return true;
    return trimmed.endsWith('for (') || trimmed.endsWith('for(')
      || trimmed.endsWith('if (') || trimmed.endsWith('if(')
      || trimmed.endsWith('while (') || trimmed.endsWith('while(')
      || trimmed.endsWith('switch (') || trimmed.endsWith('switch(');
  }

  /** Statement/construct templates disabled — AI inline handles if/for/while/etc. */
  function controlStructureInlineSuffix(_path, _linePrefix, _content, _lineNumber, _javaLevel) {
    return '';
  }

  function controlStructureInlineGhost(_path, _linePrefix, _content, _lineNumber, _javaLevel) {
    return '';
  }

  function inlineGhostSuffix(path, linePrefix, content, lineNumber, javaLevel, column = 0) {
    const inline = localInlineSuggestion(path, linePrefix, content, lineNumber, javaLevel, column);
    if (!inline) return '';
    if (inline.includes('\n')) return inline.split('\n')[0];
    return inline;
  }

  function lineIndent(linePrefix) {
    const m = linePrefix.match(/^[\t ]*/);
    return m ? m[0] : '';
  }

  function extractInlinePartialToken(linePrefix) {
    const trimmed = linePrefix.trimEnd();
    if (!trimmed) return '';
    const m = trimmed.match(/([A-Za-z_$][\w$]*)$/);
    return m ? m[1] : '';
  }

  function lineEndsWithKeyword(trimmed, kw) {
    const re = new RegExp(`(?:^|[\\s({])${kw}$`);
    return re.test(trimmed);
  }

  const CONTROL_KEYWORD_PREFIXES = [
    'if', 'else', 'for', 'while', 'do', 'switch', 'case', 'try', 'catch', 'finally',
    'elif', 'def', 'class', 'function', 'fun', 'when', 'break', 'continue', 'return',
  ];

  function isControlKeywordPrefix(token) {
    if (!token) return false;
    const lower = token.toLowerCase();
    return CONTROL_KEYWORD_PREFIXES.some((kw) => kw.startsWith(lower));
  }

  function controlStructureMenuLabel(trimmed, path) {
    if (lineEndsWithKeyword(trimmed, 'while')) return 'while (…) { }';
    if (lineEndsWithKeyword(trimmed, 'for')) return 'for (…) { }';
    if (lineEndsWithKeyword(trimmed, 'do')) return 'do { } while (…)';
    if (lineEndsWithKeyword(trimmed, 'if')) return 'if (…) { }';
    if (lineEndsWithKeyword(trimmed, 'else')) return 'else { }';
    if (lineEndsWithKeyword(trimmed, 'switch')) return 'switch (…) { }';
    if (lineEndsWithKeyword(trimmed, 'try')) return 'try { } catch { }';
    if (trimmed.endsWith('while (') || trimmed.endsWith('while(')) return 'while (…) { }';
    if (trimmed.endsWith('for (') || trimmed.endsWith('for(')) return 'for (…) { }';
    if (trimmed.endsWith('if (') || trimmed.endsWith('if(')) return 'if (…) { }';
    if (trimmed.endsWith('switch (') || trimmed.endsWith('switch(')) return 'switch (…) { }';
    const lang = langForPath(path);
    if (lang === 'python') {
      if (lineEndsWithKeyword(trimmed, 'while')) return 'while …:';
      if (lineEndsWithKeyword(trimmed, 'for')) return 'for … in …:';
      if (lineEndsWithKeyword(trimmed, 'if')) return 'if …:';
      if (lineEndsWithKeyword(trimmed, 'elif')) return 'elif …:';
      if (lineEndsWithKeyword(trimmed, 'else')) return 'else:';
      if (lineEndsWithKeyword(trimmed, 'def')) return 'def …():';
      if (lineEndsWithKeyword(trimmed, 'class')) return 'class …:';
    }
    return 'block';
  }

  function editorContextAroundLine(content, lineNumber, maxAbove = 25, maxBelow = 15) {
    const lines = String(content || '').split(/\r?\n/);
    const cur = Math.min(Math.max(0, lineNumber - 1), lines.length);
    const start = Math.max(0, cur - maxAbove);
    const end = Math.min(lines.length, cur + 1 + maxBelow);
    return lines.slice(start, end).join('\n');
  }

  /** Trim huge buffers for API payloads; returns adjusted 1-based line in the slice. */
  function contentForApiPayload(content, lineNumber) {
    const text = String(content || '');
    if (text.length <= 65536) {
      return { content: text, line: lineNumber };
    }
    const lines = text.split(/\r?\n/);
    const cur = Math.min(Math.max(0, lineNumber - 1), lines.length);
    const start = Math.max(0, cur - 120);
    const end = Math.min(lines.length, cur + 1 + 40);
    return { content: lines.slice(start, end).join('\n'), line: cur - start + 1 };
  }

  function shouldFetchIndexCompletions(linePrefix, prefix, path) {
    if (path && isSpringConfigFile(path)) {
      const p = prefix || extractInlinePartialToken(linePrefix) || '';
      return p.length >= 1 || AUTOCOMPLETE_TRIGGER_RE.test(linePrefix || '');
    }
    const p = prefix || '';
    if (p.length >= MIN_AUTOCOMPLETE_CHARS) return true;
    if (AUTOCOMPLETE_TRIGGER_RE.test(linePrefix || '')) return true;
    if (dotQualifierFromLinePrefix(linePrefix)) return true;
    if (isInsideControlParen(linePrefix)) return true;
    const trimmed = (linePrefix || '').trimStart();
    if (/^import\s/.test(trimmed)) return true;
    return false;
  }

  function editorLinePrefix(model, position) {
    return model.getValueInRange(
      new monaco.Range(position.lineNumber, 1, position.lineNumber, position.column),
    );
  }

  function memberDotContext(model, position) {
    const linePrefix = editorLinePrefix(model, position);
    return { linePrefix, member: dotQualifierFromLinePrefix(linePrefix) };
  }

  function completionContext(model, position) {
    const linePrefix = editorLinePrefix(model, position);
    const memberCtx = dotQualifierFromLinePrefix(linePrefix);
    const word = model.getWordUntilPosition(position);
    const prefix = memberCtx
      ? (memberCtx.memberPrefix || '')
      : (word.word || extractInlinePartialToken(linePrefix) || '');
    const range = buildCompletionRange(model, position, prefix, linePrefix);
    return { word, linePrefix, prefix, range, memberCtx };
  }

  function mapIndexItemToSuggestion(item, range, seen, memberContext) {
    const label = item.label;
    if (!label || seen.has(label)) return null;
    if (!isCodeLikeCompletion(label, item.kind)) return null;
    seen.add(label);
    const kind = String(item.kind || '').toLowerCase();
    let insertText = item.insert;
    if (!insertText) {
      insertText = kind === 'method' ? `${label}()` : label;
    }
    const sortRank = memberContext ? '0' : '1';
    const detail = item.detail
      ? (memberContext && kind === 'method' && !item.detail.includes('(')
        ? `${item.detail} · method`
        : item.detail)
      : undefined;
    const suggestion = {
      label,
      kind: completionKind(item.kind),
      detail,
      insertText,
      range,
      sortText: `${sortRank}_${label}`,
      filterText: memberContext
        ? `${memberContext.memberPrefix || ''}${label}`
        : undefined,
    };
    if (item.documentation) {
      const doc = String(item.documentation);
      suggestion.documentation = doc;
      suggestion.documentationHtml = `<div class="reaper-java-hover-doc">${javadocHtmlFromText(doc)}</div>`;
    }
    return suggestion;
  }

  function extractJavaJavadocBeforeLine(lines, lineIndex) {
    let end = lineIndex - 1;
    while (end >= 0 && !lines[end].trim()) end -= 1;
    if (end < 0 || !lines[end].trim().endsWith('*/')) return '';
    let start = end;
    while (start >= 0 && !lines[start].trim().startsWith('/**')) start -= 1;
    if (start < 0) return '';
    return lines.slice(start, end + 1)
      .map((ln) => ln.trim()
        .replace(/^\/\*\*?/, '')
        .replace(/\*\/$/, '')
        .replace(/^\*\s?/, '')
        .trim())
      .filter(Boolean)
      .join('\n');
  }

  function javaMethodMetaFromContent(content, methodName) {
    const lines = String(content || '').split(/\r?\n/);
    const re = new RegExp(`\\b${methodName}\\s*\\(`);
    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];
      if (!re.test(line)) continue;
      const trimmed = line.split('//')[0].trim();
      if (!trimmed.includes(`${methodName}(`)) continue;
      if (/^\s*(if|while|for|catch|switch|return|throw|new)\b/.test(trimmed)) continue;
      const sig = trimmed.split('{')[0].split(';')[0].trim();
      const doc = extractJavaJavadocBeforeLine(lines, i);
      return { signature: sig, documentation: doc };
    }
    return null;
  }

  function enrichJavaSuggestion(item, content, path) {
    if (!content || langForPath(path || '') !== 'java') return item;
    const label = typeof item.label === 'string' ? item.label : (item.label?.label || '');
    const kindStr = String(item.kind || '').toLowerCase();
    const isMethod = kindStr.includes('method')
      || (typeof item.kind === 'number'
        && typeof monaco !== 'undefined'
        && item.kind === monaco.languages.CompletionItemKind.Method);
    const isField = kindStr.includes('field')
      || kindStr.includes('property')
      || (typeof item.kind === 'number'
        && typeof monaco !== 'undefined'
        && (item.kind === monaco.languages.CompletionItemKind.Field
          || item.kind === monaco.languages.CompletionItemKind.Property));
    if (!isMethod && !isField) return item;
    const bare = label.replace(/\(\)$/, '');
    const meta = javaMethodMetaFromContent(content, bare);
    if (!meta) return item;
    if (meta.signature && (!item.detail || !item.detail.includes('('))) {
      item.detail = meta.signature;
    }
    if (meta.documentation && !item.documentation) {
      item.documentation = meta.documentation;
    }
    return item;
  }

  function escapeHtml(text) {
    return String(text)
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;');
  }

  function javadocHtmlFromText(text) {
    const doc = String(text || '').trim();
    if (!doc) return '';
    return doc.split('\n').map((line) => {
      const trimmed = line.trim();
      if (!trimmed) return '<div class="reaper-javadoc-spacer"></div>';
      if (trimmed.startsWith('@')) {
        const m = trimmed.match(/^@(\w+)\s*(.*)/);
        if (m) {
          return `<div class="reaper-javadoc-tag"><span class="reaper-javadoc-tag-name">@${escapeHtml(m[1])}</span>${m[2] ? ` ${escapeHtml(m[2])}` : ''}</div>`;
        }
      }
      return `<div class="reaper-javadoc-line">${escapeHtml(trimmed)}</div>`;
    }).join('');
  }

  function hoverHtmlFromInfo(info) {
    if (!info?.name) return '';
    const kind = escapeHtml(info.kind || 'symbol');
    let html = '<div class="reaper-java-hover">';
    if (info.signature) {
      html += '<div class="reaper-java-hover-head">';
      html += `<span class="reaper-java-hover-kind-pill">${kind}</span>`;
      html += `<div class="reaper-java-hover-sig">${escapeHtml(info.signature)}</div>`;
      html += '</div>';
    } else {
      html += '<div class="reaper-java-hover-title">';
      html += `<span class="reaper-java-hover-name">${escapeHtml(info.name)}</span>`;
      html += `<span class="reaper-java-hover-kind-pill">${kind}</span>`;
      html += '</div>';
    }
    if (info.documentation) {
      html += `<div class="reaper-java-hover-doc panel-scroll">${javadocHtmlFromText(info.documentation)}</div>`;
    } else if (info.kind === 'method' || info.kind === 'field') {
      html += '<div class="reaper-java-hover-empty">No documentation available.</div>';
    }
    html += '</div>';
    return html;
  }

  function buildEditorHoverHtml(symInfo, markers) {
    const parts = [];
    if (symInfo?.name) {
      parts.push(hoverHtmlFromInfo(symInfo));
    }
    if (markers?.length) {
      const errors = markers.filter(
        (m) => m.severity === monaco.MarkerSeverity.Error,
      );
      const warnings = markers.filter(
        (m) => m.severity === monaco.MarkerSeverity.Warning,
      );
      const tone = errors.length ? 'error' : 'warning';
      const count = markers.length;
      const line = markers[0].startLineNumber;
      const title = count === 1 ? '1 problem' : `${count} problems`;

      let diagHtml = `<div class="reaper-editor-hover-problems reaper-editor-hover-problems--${tone}">`;
      diagHtml += '<div class="reaper-editor-hover-problems-head">';
      diagHtml += `<span class="reaper-editor-hover-problems-badge">${tone === 'error' ? 'Error' : 'Warning'}</span>`;
      diagHtml += `<span class="reaper-editor-hover-problems-title">${escapeHtml(title)}</span>`;
      diagHtml += `<span class="reaper-editor-hover-problems-loc">Ln ${line}</span>`;
      diagHtml += '</div>';
      diagHtml += '<ul class="reaper-editor-hover-problems-list">';
      for (const m of markers) {
        const hint = typeof helpers?.diagnosticFriendlyHint === 'function'
          ? helpers.diagnosticFriendlyHint(m.message)
          : '';
        const msg = String(m.message || '').trim() + hint;
        if (msg) {
          diagHtml += `<li>${escapeHtml(msg)}</li>`;
        }
      }
      diagHtml += '</ul>';
      if (errors.length) {
        diagHtml += '<div class="reaper-editor-hover-problems-foot">⌘. quick fix · click flag in status bar</div>';
      }
      diagHtml += '</div>';
      parts.push(diagHtml);
    }
    if (!parts.length) return '';
    const rootCls = markers?.length && !symInfo?.name
      ? 'reaper-editor-hover reaper-editor-hover--problems-only panel-scroll'
      : 'reaper-editor-hover panel-scroll';
    return `<div class="${rootCls}">${parts.join('')}</div>`;
  }

  function itemDocumentationParts(item) {
    if (item?.documentationHtml) {
      return { text: '', html: String(item.documentationHtml).trim() };
    }
    if (!item?.documentation) return { text: '', html: '' };
    const doc = item.documentation;
    if (typeof doc === 'string') {
      return { text: doc.trim(), html: '' };
    }
    if (typeof doc === 'object') {
      const value = typeof doc.value === 'string' ? doc.value.trim() : '';
      if (!value) return { text: '', html: '' };
      if (doc.supportHtml || value.includes('<')) {
        return { text: '', html: value };
      }
      return { text: value, html: '' };
    }
    return { text: '', html: '' };
  }

  function itemDocumentationText(item) {
    const parts = itemDocumentationParts(item);
    if (parts.text) return parts.text;
    if (!parts.html) return '';
    const tmp = document.createElement('div');
    tmp.innerHTML = parts.html;
    return (tmp.textContent || '').trim();
  }

  function enrichItemDocumentationFromSource(item, content, path) {
    if (!item || !content) return item;
    enrichJavaSuggestion(item, content, path);
    return item;
  }

  function hoverInfoFromItem(item, content, path) {
    enrichItemDocumentationFromSource(item, content, path);
    const kindKey = typeof item.kind === 'number' ? '' : String(item.kind || '');
    const docParts = itemDocumentationParts(item);
    return {
      name: typeof item.label === 'string' ? item.label : (item.label?.label || ''),
      kind: kindKey || 'member',
      signature: item.detail ? String(item.detail) : '',
      documentation: docParts.text || itemDocumentationText(item),
      documentationHtml: docParts.html,
    };
  }

  function applyHoverInfoToDocPanel(sigEl, bodyEl, info, kindKey) {
    const sig = info?.signature ? String(info.signature) : '';
    if (sigEl) {
      sigEl.textContent = sig;
      sigEl.hidden = !sig;
    }
    if (!bodyEl) return;
    const docHtml = info?.documentationHtml ? String(info.documentationHtml).trim() : '';
    const doc = info?.documentation ? String(info.documentation).trim() : '';
    if (docHtml) {
      bodyEl.innerHTML = docHtml;
      bodyEl.classList.remove('reaper-member-suggest-doc-empty');
      return;
    }
    if (doc) {
      renderJavadocBody(bodyEl, doc);
      bodyEl.classList.remove('reaper-member-suggest-doc-empty');
      return;
    }
    bodyEl.innerHTML = '';
    const empty = document.createElement('div');
    empty.className = 'reaper-java-hover-empty';
    empty.textContent = (kindKey === 'method' || kindKey === 'field' || kindKey === 'member')
      ? 'No documentation available.'
      : '';
    if (empty.textContent) bodyEl.appendChild(empty);
  }

  function formatJavadocMarkdown(doc) {
    return String(doc || '').split('\n').map((line) => {
      const trimmed = line.trim();
      if (!trimmed) return '';
      if (trimmed.startsWith('@')) {
        const m = trimmed.match(/^@(\w+)\s*(.*)/);
        if (m) return `**@${m[1]}** ${m[2]}`.trim();
      }
      return trimmed;
    }).filter((line, i, arr) => !(line === '' && arr[i + 1] === '')).join('\n\n');
  }

  function hoverMarkdownFromInfo(info) {
    if (!info?.name) return '';
    const parts = [];
    if (info.signature) {
      parts.push(`\`\`\`java\n${info.signature}\n\`\`\``);
    } else {
      parts.push(`**${info.name}** · ${info.kind || 'symbol'}`);
    }
    if (info.documentation) {
      parts.push(formatJavadocMarkdown(info.documentation));
    } else if (info.kind === 'method' || info.kind === 'field') {
      parts.push('*No documentation.*');
    }
    return parts.join('\n\n');
  }

  function renderJavadocBody(el, text) {
    el.innerHTML = '';
    const doc = String(text || '').trim();
    if (!doc) return;
    for (const line of doc.split('\n')) {
      const row = document.createElement('div');
      const trimmed = line.trim();
      if (!trimmed) {
        row.className = 'reaper-javadoc-spacer';
        el.appendChild(row);
        continue;
      }
      if (trimmed.startsWith('@')) {
        row.className = 'reaper-javadoc-tag';
        const m = trimmed.match(/^@(\w+)\s*(.*)/);
        if (m) {
          const tag = document.createElement('span');
          tag.className = 'reaper-javadoc-tag-name';
          tag.textContent = `@${m[1]}`;
          row.appendChild(tag);
          if (m[2]) {
            row.appendChild(document.createTextNode(` ${m[2]}`));
          }
        } else {
          row.textContent = trimmed;
        }
      } else {
        row.className = 'reaper-javadoc-line';
        row.textContent = trimmed;
      }
      el.appendChild(row);
    }
  }

  function wordRangeAt(model, position) {
    const word = model.getWordAtPosition(position);
    if (!word) {
      return new monaco.Range(
        position.lineNumber,
        position.column,
        position.lineNumber,
        position.column,
      );
    }
    return new monaco.Range(
      position.lineNumber,
      word.startColumn,
      position.lineNumber,
      word.endColumn,
    );
  }

  function sanitizeInlineGhostText(text) {
    let t = String(text ?? '');
    const openThink = '<' + 'think' + '>';
    const closeThink = '<' + '/' + 'think' + '>';
    const openRr = '<' + 'redacted_reasoning' + '>';
    const closeRr = '<' + '/' + 'redacted_reasoning' + '>';
    const stripTag = (s, open, close) => {
      let out = s;
      for (;;) {
        const start = out.toLowerCase().indexOf(open);
        if (start < 0) break;
        const after = start + open.length;
        const endRel = out.toLowerCase().indexOf(close, after);
        if (endRel < 0) {
          out = out.slice(0, start);
          break;
        }
        out = out.slice(0, start) + out.slice(endRel + close.length);
      }
      return out;
    };
    t = stripTag(t, openThink, closeThink);
    t = stripTag(t, openRr, closeRr);
    t = t.replace(/^\s*(thinking|thought)\s*:\s*/im, '');
    const trimmed = t.trim();
    if (!trimmed) return '';
    if (/^(here is|the user|this (code|suggestion)|i (think|would)|let me)/i.test(trimmed)) {
      return '';
    }
    return t;
  }

  function isValidJsReceiverExpr(expr) {
    const e = String(expr || '').trim();
    if (!e) return false;
    if (e === 'this' || e === 'super' || e === 'self') return true;
    if (/^@[A-Za-z_]\w*$/.test(e)) return true;
    if (/^(?:[A-Za-z_$][\w$]*)(?:\.[A-Za-z_$][\w$]*)*$/.test(e)) return true;
    if (/^(?:this|super|self|[A-Za-z_$][\w$]*)\[(?:\d+|[A-Za-z_$][\w$]*)\]$/.test(e)) return true;
    return false;
  }

  function dotQualifierFromLinePrefix(linePrefix) {
    const trimmed = String(linePrefix || '').trimEnd();
    if (!trimmed.includes('.')) return null;
    const dotPos = trimmed.lastIndexOf('.');
    const qualPart = trimmed.slice(0, dotPos).trim();
    const memberPart = trimmed.slice(dotPos + 1).replace(/[^\w$].*$/, '');
    if (!qualPart) return null;
    const paren = qualPart.lastIndexOf('(');
    const receiverExpr = paren >= 0 ? qualPart.slice(paren + 1).trim() : qualPart;
    if (!receiverExpr || !isValidJsReceiverExpr(receiverExpr)) return null;
    const baseIdent = receiverExpr.split(/[.\[]/)[0].trim();
    const blockKw = ['if', 'for', 'while', 'return', 'import', 'package', 'new', 'else', 'catch'];
    if (blockKw.includes(baseIdent) && baseIdent !== 'this' && baseIdent !== 'super' && baseIdent !== 'self') {
      return null;
    }
    return { qualifier: receiverExpr.replace(/^@/, ''), memberPrefix: memberPart };
  }

  function readIdentAtStart(s) {
    const m = String(s || '').trimStart().match(/^([A-Za-z_$][\w$]*)/);
    return m ? m[1] : '';
  }

  function simplifyJavaTypeName(typeName) {
    let s = String(typeName || '').trim();
    if (!s) return '';
    s = s.replace(/\[\]/g, '');
    const lt = s.indexOf('<');
    if (lt > 0) s = s.slice(0, lt);
    return s.split('.').pop() || s;
  }

  function inferJavaDeclaredMemberType(content, memberName) {
    const name = String(memberName || '').replace(/^@/, '').trim();
    if (!name || name === 'this' || name === 'super') return '';
    const src = String(content || '');
    const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    const typePat = '([A-Za-z_][\\w.<>,]*(?:\\[\\])*?)';
    const fieldRe = new RegExp(
      `(?:^|[\\n;{}])\\s*(?:@[\\w.]+\\s*)*(?:public|private|protected)?\\s*(?:static\\s+)?(?:final\\s+)?${typePat}\\s+${escaped}\\s*[=;]`,
      'gm',
    );
    let m = fieldRe.exec(src);
    if (m) return simplifyJavaTypeName(m[1]);
    const localRe = new RegExp(
      `(?:^|[;{}])\\s*(?:final\\s+)?${typePat}\\s+${escaped}\\s*[=;]`,
      'gm',
    );
    m = localRe.exec(src);
    if (m) return simplifyJavaTypeName(m[1]);
    return inferJavaMethodParameterType(content, name);
  }

  function inferJavaMethodParameterType(content, paramName) {
    const name = String(paramName || '').trim();
    if (!name) return '';
    const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    const typePat = '([A-Za-z_][\\w.<>,]*(?:\\[\\])*?)';
    const re = new RegExp(`\\([^)]*?${typePat}\\s+${escaped}\\b`, 'g');
    const m = re.exec(String(content || ''));
    return m ? simplifyJavaTypeName(m[1]) : '';
  }

  const JDK_STATIC_FIELD_TYPES = {
    System: { out: 'PrintStream', in: 'InputStream', err: 'PrintStream' },
  };

  const JDK_MEMBER_SUMMARIES = {
    'System.out': 'Standard output. A PrintStream used for console output.',
    'System.in': 'Standard input. An InputStream for reading from the console.',
    'System.err': 'Standard error. A PrintStream for error messages.',
    'Math.PI': 'Ratio of the circumference of a circle to its diameter (π).',
    'Math.E': "Euler's number, the base of natural logarithms.",
  };

  const COMMON_JAVA_IMPORTS = {
    RestController: 'org.springframework.web.bind.annotation.RestController',
    Controller: 'org.springframework.stereotype.Controller',
    Service: 'org.springframework.stereotype.Service',
    Component: 'org.springframework.stereotype.Component',
    Repository: 'org.springframework.stereotype.Repository',
    Autowired: 'org.springframework.beans.factory.annotation.Autowired',
    Value: 'org.springframework.beans.factory.annotation.Value',
    RequestMapping: 'org.springframework.web.bind.annotation.RequestMapping',
    GetMapping: 'org.springframework.web.bind.annotation.GetMapping',
    PostMapping: 'org.springframework.web.bind.annotation.PostMapping',
    PutMapping: 'org.springframework.web.bind.annotation.PutMapping',
    DeleteMapping: 'org.springframework.web.bind.annotation.DeleteMapping',
    PatchMapping: 'org.springframework.web.bind.annotation.PatchMapping',
    PathVariable: 'org.springframework.web.bind.annotation.PathVariable',
    RequestParam: 'org.springframework.web.bind.annotation.RequestParam',
    RequestBody: 'org.springframework.web.bind.annotation.RequestBody',
    SpringBootApplication: 'org.springframework.boot.autoconfigure.SpringBootApplication',
    ResponseEntity: 'org.springframework.http.ResponseEntity',
    HttpStatus: 'org.springframework.http.HttpStatus',
  };

  function extractJavacClassSymbol(message) {
    const msg = String(message || '');
    const idx = msg.indexOf('symbol:');
    if (idx < 0) return '';
    const rest = msg.slice(idx + 'symbol:'.length).trim();
    const parts = rest.split(/\s+/);
    if (!parts.length) return '';
    if (parts[0] === 'class' || parts[0] === 'interface') return parts[1] || '';
    return parts[0];
  }

  function javaImportInsertEdit(model, fqcn) {
    const importLine = `import ${fqcn};`;
    const lineCount = model.getLineCount();
    let insertAfter = 0;
    let lastImport = 0;
    for (let i = 1; i <= lineCount; i++) {
      const trimmed = model.getLineContent(i).trim();
      if (trimmed.startsWith('package ')) insertAfter = i;
      else if (trimmed.startsWith('import ')) lastImport = i;
    }
    const insertLine = lastImport ? lastImport + 1 : Math.max(1, insertAfter + 1);
    const text = insertLine > 1 ? `\n${importLine}` : `${importLine}\n`;
    return {
      start_line: insertLine,
      start_column: 1,
      end_line: insertLine,
      end_column: 1,
      text,
    };
  }

  function memberKindFromItem(item) {
    const k = item?.kind;
    if (typeof k === 'number' && typeof monaco !== 'undefined') {
      if (k === monaco.languages.CompletionItemKind.Method) return 'method';
      if (k === monaco.languages.CompletionItemKind.Field
        || k === monaco.languages.CompletionItemKind.Property) return 'field';
    }
    const s = String(k || '').toLowerCase();
    if (s.includes('method')) return 'method';
    if (s.includes('field') || s.includes('property')) return 'field';
    return 'member';
  }

  function memberDotQualifierFromEditor(ed, model) {
    const position = ed?.getPosition?.();
    if (!model || !position) return '';
    const linePrefix = editorLinePrefix(model, position);
    const ctx = dotQualifierFromLinePrefix(linePrefix);
    return ctx?.qualifier || '';
  }

  function memberDisplaySignature(item, qualifier) {
    const label = typeof item.label === 'string' ? item.label : (item.label?.label || '');
    const detail = item.detail ? String(item.detail) : '';
    if (detail.includes('(') || detail.includes(';')) return detail;
    const base = String(qualifier || detail.split('.')[0] || '').split('.')[0];
    const fieldType = JDK_STATIC_FIELD_TYPES[base]?.[label];
    if (fieldType) {
      return `public static final ${fieldType} ${label}`;
    }
    if (detail.includes('.')) return detail;
    return detail || label;
  }

  function memberDocFallback(item, qualifier) {
    const label = typeof item.label === 'string' ? item.label : (item.label?.label || '');
    const base = String(qualifier || '').split('.')[0]
      || String(item.detail || '').split('.')[0]
      || '';
    const key = base ? `${base}.${label}` : label;
    if (JDK_MEMBER_SUMMARIES[key]) return JDK_MEMBER_SUMMARIES[key];
    const kind = memberKindFromItem(item);
    if (kind === 'field') {
      const ft = JDK_STATIC_FIELD_TYPES[base]?.[label];
      if (ft) return `${kind} · ${ft}`;
    }
    if (kind === 'method') return 'No Javadoc in source.';
    if (kind === 'field') return 'No Javadoc in source.';
    return '';
  }

  const JDK_CLASS_STATIC_MEMBERS = {
    System: ['out', 'in', 'err'],
    Math: ['PI', 'E', 'abs', 'max', 'min', 'random', 'sqrt'],
    Arrays: ['asList', 'sort', 'copyOf', 'equals', 'stream'],
  };

  function effectiveEditorLang(path, model) {
    const fromPath = langForPath(path || '');
    if (fromPath !== 'plaintext') return fromPath;
    const fromModel = model?.getLanguageId?.();
    if (fromModel && fromModel !== 'plaintext') return fromModel;
    const base = String(path || '').split('/').pop() || '';
    if (/\.java$/i.test(base)) return 'java';
    if (/\.(kt|kts)$/i.test(base)) return 'kotlin';
    if (/\.(groovy|gradle)$/i.test(base)) return 'groovy';
    return fromPath || 'plaintext';
  }

  function ensureModelLanguageForPath(path, model) {
    const expected = langForPath(path || '');
    if (!expected || expected === 'plaintext' || !model) return expected;
    if (model.getLanguageId() !== expected) {
      ensureMonacoBasicLanguage(expected);
      monaco.editor.setModelLanguage(model, expected);
    }
    return expected;
  }

  function jdkStaticMemberItems(qualifier, memberPrefix, content = '') {
    const q = String(qualifier || '').trim();
    if (!q) return [];
    if (q.includes('.')) {
      const type = resolveJavaQualifierType(content, q);
      return type ? jdkBuiltinMembersForType(type, memberPrefix) : [];
    }
    const base = q.split('.')[0];
    if (!base) return [];
    const members = JDK_CLASS_STATIC_MEMBERS[base];
    if (!members) return [];
    const prefixLower = (memberPrefix || '').toLowerCase();
    const staticTypes = JDK_STATIC_FIELD_TYPES[base] || {};
    return members
      .filter((m) => !memberPrefix || m.toLowerCase().startsWith(prefixLower))
      .map((m) => ({
        label: m,
        kind: staticTypes[m] ? 'field' : 'method',
        detail: `${base}.${m}`,
      }));
  }

  function memberPreviewItems(content, qualifier, memberPrefix, path, model) {
    return localMemberCompletions(content, qualifier, memberPrefix, path, model);
  }

  function resolveJavaQualifierType(content, qualifier) {
    const q = String(qualifier || '').trim();
    if (!q) return '';
    const parts = q.split('.');
    if (parts.length === 1) {
      const declared = inferJavaDeclaredMemberType(content, parts[0]);
      if (declared) return declared;
      const param = inferJavaMethodParameterType(content, parts[0]);
      if (param) return param;
      if (JDK_CLASS_STATIC_MEMBERS[parts[0]]) return parts[0];
      return '';
    }
    const staticFields = JDK_STATIC_FIELD_TYPES[parts[0]];
    if (staticFields && parts.length === 2 && staticFields[parts[1]]) {
      return staticFields[parts[1]];
    }
    if (parts.length >= 2) {
      const parent = parts.slice(0, -1).join('.');
      const parentType = resolveJavaQualifierType(content, parent);
      if (parentType && parentType !== parts[0]) return parentType;
    }
    return '';
  }

  const JDK_TYPE_MEMBERS = {
    String: ['charAt', 'length', 'substring', 'toLowerCase', 'toUpperCase', 'equals', 'isEmpty', 'trim', 'split', 'concat', 'replace', 'startsWith', 'endsWith', 'indexOf', 'lastIndexOf', 'contains', 'format', 'valueOf', 'getBytes', 'toCharArray', 'compareTo', 'strip', 'repeat'],
    Integer: ['intValue', 'longValue', 'toString', 'parseInt', 'valueOf', 'compare', 'equals', 'hashCode'],
    Long: ['longValue', 'intValue', 'toString', 'parseLong', 'valueOf', 'compare', 'equals'],
    Boolean: ['booleanValue', 'toString', 'valueOf', 'equals'],
    Object: ['toString', 'equals', 'hashCode', 'getClass', 'clone', 'notify', 'notifyAll', 'wait'],
    List: ['add', 'get', 'size', 'isEmpty', 'clear', 'remove', 'iterator', 'contains', 'addAll', 'removeAll'],
    ArrayList: ['add', 'get', 'size', 'isEmpty', 'clear', 'remove', 'iterator', 'contains'],
    Map: ['get', 'put', 'size', 'isEmpty', 'clear', 'remove', 'containsKey', 'containsValue', 'keySet', 'values', 'entrySet'],
    HashMap: ['get', 'put', 'size', 'isEmpty', 'clear', 'remove', 'containsKey'],
    Set: ['add', 'size', 'isEmpty', 'clear', 'remove', 'contains', 'iterator'],
    PrintStream: ['print', 'println', 'printf', 'format', 'write', 'flush', 'close'],
    Scanner: ['next', 'nextLine', 'nextInt', 'hasNext', 'close'],
  };

  function jdkBuiltinMembersForType(typeName, memberPrefix) {
    const simple = simplifyJavaTypeName(typeName);
    const members = JDK_TYPE_MEMBERS[simple];
    if (!members) return [];
    const prefixLower = (memberPrefix || '').toLowerCase();
    return members
      .filter((m) => !memberPrefix || m.toLowerCase().startsWith(prefixLower))
      .map((m) => ({ label: m, kind: 'method', detail: `${simple}.${m}` }));
  }

  function localMemberCompletions(content, qualifier, memberPrefix, path, model) {
    const items = [];
    const seen = new Set();
    const prefixLower = (memberPrefix || '').toLowerCase();
    const lang = effectiveEditorLang(path, model);

    for (const m of jdkStaticMemberItems(qualifier, memberPrefix, content)) {
      if (seen.add(m.label)) items.push(m);
    }

    const needles = [`${qualifier}.`, `@${qualifier}.`];
    const lines = String(content || '').split(/\r?\n/);
    for (const lineText of lines) {
      for (const needle of needles) {
        let search = 0;
        for (;;) {
          const idx = lineText.indexOf(needle, search);
          if (idx < 0) break;
          const start = idx + needle.length;
          const member = readIdentAtStart(lineText.slice(start));
        if (member
          && (!memberPrefix || member.toLowerCase().startsWith(prefixLower))
          && !seen.has(member)) {
          seen.add(member);
          const after = lineText.slice(start);
          const isMethod = after.length > member.length && after[member.length] === '(';
          items.push({ label: member, kind: isMethod ? 'method' : 'field', detail: `${qualifier}.${member}` });
        }
        search = start + 1;
        }
      }
    }

    if (qualifier === 'this' || qualifier === 'self' || qualifier === 'super') {
      for (const lineText of lines) {
        const trimmed = lineText.trim();
        if (lang === 'ruby') {
          const defM = trimmed.match(/^def\s+([A-Za-z_]\w*)/);
          if (defM && (!memberPrefix || defM[1].toLowerCase().startsWith(prefixLower)) && seen.add(defM[1])) {
            items.push({ label: defM[1], kind: 'method', detail: 'method' });
          }
          const attrM = trimmed.match(/^attr_(?:reader|writer|accessor)\s+(.+)/);
          if (attrM) {
            for (const part of attrM[1].split(/[\s,]+/)) {
              const name = part.trim();
              if (name && (!memberPrefix || name.toLowerCase().startsWith(prefixLower)) && seen.add(name)) {
                items.push({ label: name, kind: 'field', detail: 'attribute' });
              }
            }
          }
        }
        if (lang === 'python') {
          const defM = trimmed.match(/^def\s+([A-Za-z_]\w*)/);
          if (defM && defM[1] !== 'self'
            && (!memberPrefix || defM[1].toLowerCase().startsWith(prefixLower)) && seen.add(defM[1])) {
            items.push({ label: defM[1], kind: 'method', detail: 'def' });
          }
        }
      }
    }

    if (lang === 'java' || lang === 'kotlin') {
      const type = resolveJavaQualifierType(content, qualifier);
      if (type) {
        for (const m of jdkBuiltinMembersForType(type, memberPrefix)) {
          if (seen.add(m.label)) {
            items.push(m);
          }
        }
      }
      const base = qualifier.split('.')[0];
      if (base && qualifier.indexOf('.') < 0 && JDK_CLASS_STATIC_MEMBERS[base]) {
        const staticTypes = JDK_STATIC_FIELD_TYPES[base] || {};
        for (const m of JDK_CLASS_STATIC_MEMBERS[base]) {
          if ((!memberPrefix || m.toLowerCase().startsWith(prefixLower)) && seen.add(m)) {
            const kind = staticTypes[m] ? 'field' : 'method';
            items.push({ label: m, kind, detail: `${base}.${m}` });
          }
        }
      }
    }

    return items.slice(0, 40);
  }

  function syntaxInlineSuffix(path, linePrefix) {
    const trimmed = String(linePrefix || '').trimEnd();
    if (!trimmed) return '';
    const lang = langForPath(path);

    if (/\w\s*\($/.test(trimmed) && !trimmed.endsWith('()')) return ')';
    if (trimmed.endsWith('{') && !trimmed.endsWith('${')) return '}';
    if (trimmed.endsWith('[')) return ']';
    const dq = (trimmed.match(/"/g) || []).length;
    if (dq % 2 === 1 && !trimmed.endsWith('\\"')) return '"';
    const sq = (trimmed.match(/'/g) || []).length;
    if (sq % 2 === 1 && !trimmed.endsWith('\\')) return "'";

    if (lang === 'html' || lang === 'xml') {
      const tag = trimmed.match(/<([A-Za-z][\w-]*)$/);
      if (tag) return `></${tag[1]}>`;
      if (trimmed.endsWith('<')) return '/>';
    }
    if (lang === 'css' || lang === 'scss' || lang === 'less') {
      if (trimmed.endsWith(':')) return ' ;';
      if (trimmed.endsWith(';')) return '\n';
    }
    if (lang === 'json' && trimmed.endsWith('"')) return ': ""';
    if (lang === 'yaml' && /:\s*$/.test(trimmed)) return ' ';
    return '';
  }

  /** Prefer AI over local templates for statements and block bodies (all languages). */
  function isInsideControlParen(linePrefix) {
    const trimmed = String(linePrefix || '').trimEnd();
    if (dotQualifierFromLinePrefix(trimmed)) return false;
    return /\b(for|if|while|switch)\s*\([^)]*$/i.test(trimmed);
  }

  function collectJavaScopeVariables(content, throughLine) {
    const out = [];
    const seen = new Set();
    const lines = String(content || '').split(/\r?\n/);
    const limit = Math.min(throughLine, lines.length);
    const add = (name, typeHint) => {
      if (!name || seen.has(name)) return;
      seen.add(name);
      out.push({ name, typeHint: typeHint || '' });
    };
    const parseLocal = (seg) => {
      const decl = seg.split('=')[0].trim().replace(/,$/, '');
      if (!decl || decl.includes('(') || /\b(class|interface|enum|record)\b/.test(decl)) return;
      const parts = decl.split(/\s+/);
      const mods = new Set(['public', 'private', 'protected', 'static', 'final', 'volatile', 'transient']);
      while (parts.length > 2 && mods.has(parts[0])) parts.shift();
      if (parts.length < 2) return;
      const name = parts[parts.length - 1];
      const ty = parts.slice(0, -1).join(' ');
      if (name && ty && !isControlKeywordPrefix(name)) add(name, ty);
    };
    for (let i = 0; i < limit; i += 1) {
      const trimmed = lines[i].trim();
      if (!trimmed || trimmed.startsWith('//')) continue;
      const parenOpen = trimmed.indexOf('(');
      const parenClose = trimmed.lastIndexOf(')');
      if (parenOpen >= 0 && parenClose > parenOpen) {
        trimmed.slice(parenOpen + 1, parenClose).split(',').forEach((param) => {
          const p = param.trim();
          const parts = p.split(/\s+/);
          if (parts.length >= 2) {
            const name = parts[parts.length - 1].replace(/\.\.\.$/, '');
            const ty = parts.slice(0, -1).join(' ').replace(/\.\.\.$/, '[]');
            if (name && !isControlKeywordPrefix(name)) add(name, ty);
          }
        });
      }
      trimmed.split(';').forEach((seg) => {
        const s = seg.trim();
        if (!s || /^(for|if|while|switch|return)\b/.test(s)) return;
        parseLocal(s);
      });
    }
    return out;
  }

  function shouldPreferAiStatementInline(path, linePrefix, content, lineNumber) {
    if (dotQualifierFromLinePrefix(linePrefix)) return false;
    const trimmed = linePrefix.trimEnd();
    if (hasCompleteControlKeyword(trimmed)) return true;
    if (isInsideControlParen(linePrefix)) return true;
    if (isWhitespaceOnlyLine(linePrefix) && findEnclosingBlock(content, lineNumber)) return true;
    const token = extractInlinePartialToken(linePrefix);
    if (token && isControlKeywordPrefix(token)) return true;
    return false;
  }

  const JAVA_PRIMITIVE_TYPES = new Set([
    'int', 'long', 'boolean', 'char', 'byte', 'short', 'float', 'double', 'void', 'var',
  ]);

  /** Typing `String greeting` — suppress AI ghost that would glue `greeting` onto the type. */
  function isJavaDeclarationTyping(path, linePrefix) {
    const lang = langForPath(path);
    if (lang !== 'java' && lang !== 'kotlin' && lang !== 'kts') return false;
    const trimmed = linePrefix.trimStart();
    if (!trimmed || trimmed.includes('=') || trimmed.includes(';')) return false;
    if (/\.\w/.test(trimmed)) return false;
    const declRe = /^(?:(?:public|private|protected|static|final|volatile|transient)\s+)*([\w][\w.<>\[\]]*)\s*([\w$]*)$/;
    const m = trimmed.match(declRe);
    if (!m) return false;
    const typePart = m[1];
    if (/^(if|for|while|return|new|throw|catch|switch|try)\b/.test(typePart)) return false;
    return JAVA_PRIMITIVE_TYPES.has(typePart)
      || /^[A-Z]/.test(typePart)
      || typePart.includes('<')
      || typePart.includes('[');
  }

  /** Ghost must not start with an identifier char when the line already ends on one. */
  function inlineGhostSafeAfter(linePrefix, ghostText) {
    const ghost = String(ghostText ?? '');
    if (!ghost) return false;
    const trimmed = String(linePrefix || '').trimEnd();
    if (!trimmed) return true;
    const last = trimmed[trimmed.length - 1];
    if (!/[A-Za-z0-9_$]/.test(last)) return true;
    return !/[A-Za-z0-9_$]/.test(ghost[0]);
  }

  function shouldSuppressInlineGhost(path, linePrefix, ghostText) {
    if (isJavaDeclarationTyping(path, linePrefix)) return true;
    if (!inlineGhostSafeAfter(linePrefix, ghostText)) return true;
    return false;
  }

  function buildInlineCacheKey(repo, path, lineNumber, column, linePrefix) {
    return `${repo}:${path}:${lineNumber}:${column}:${linePrefix}`;
  }

  /** Keep ghost text while typed chars match the pending suggestion. */
  function inlineGhostFromCache(cache, repo, path, lineNumber, column, linePrefix) {
    if (!cache?.text) return '';
    const key = buildInlineCacheKey(repo, path, lineNumber, column, linePrefix);
    let ghost = '';
    if (cache.key === key) {
      ghost = cache.text;
    } else if (
      cache.repo === repo
      && cache.path === path
      && cache.lineNumber === lineNumber
      && column >= cache.column
      && linePrefix.startsWith(cache.linePrefix)
    ) {
      const typed = linePrefix.slice(cache.linePrefix.length);
      if (cache.text.startsWith(typed)) {
        ghost = cache.text.slice(typed.length);
      }
    }
    if (!ghost) return '';
    if (shouldSuppressInlineGhost(path, linePrefix, ghost)) return '';
    return ghost;
  }

  function memberInlineSuffix(content, linePrefix, path) {
    const dot = dotQualifierFromLinePrefix(linePrefix);
    if (!dot || !dot.memberPrefix) return '';
    const members = localMemberCompletions(content, dot.qualifier, dot.memberPrefix, path);
    const best = members.find((m) =>
      m.label.toLowerCase().startsWith(dot.memberPrefix.toLowerCase())
      && m.label.length > dot.memberPrefix.length,
    );
    if (!best) return '';
    let rest = best.label.slice(dot.memberPrefix.length);
    if (best.kind === 'method' && !rest.endsWith('(')) rest += '()';
    return rest;
  }

  function buildLocalCompletionSuggestions(model, position, path, helpers, content) {
    const { linePrefix, prefix, range, memberCtx } = completionContext(model, position);
    const seen = new Set();
    const suggestions = [];
    const minKwChars = 1;
    const javaLevel = inlineJavaLevel(helpers);
    const text = content || model.getValue();

    function push(label, kind, detail, insertText, sortKey = '2', filterText) {
      if (!label || seen.has(label)) return;
      if (!isCodeLikeCompletion(label, kind)) return;
      seen.add(label);
      const text = insertText ?? label;
      const item = {
        label,
        kind: completionKind(kind),
        detail: detail || undefined,
        insertText: text,
        range,
        sortText: `${sortKey}_${label}`,
      };
      if (filterText) item.filterText = filterText;
      if (
        kind === 'snippet'
        || text.includes('\n')
        || text.includes('$0')
        || text.includes('$1')
      ) {
        item.insertTextRules = monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet;
      }
      enrichJavaSuggestion(item, text, path);
      suggestions.push(item);
    }

    const dot = memberCtx || dotQualifierFromLinePrefix(linePrefix);
    if (dot) {
      for (const m of jdkStaticMemberItems(dot.qualifier, dot.memberPrefix, text)) {
        const insert = m.kind === 'method' ? `${m.label}()` : m.label;
        const filterText = `${dot.memberPrefix || ''}${m.label}`;
        push(m.label, m.kind, m.detail || `${dot.qualifier}.${m.label}`, insert, '0', filterText);
      }
      for (const m of localMemberCompletions(text, dot.qualifier, dot.memberPrefix, path, model)) {
        const insert = m.kind === 'method' ? `${m.label}()` : m.label;
        const filterText = `${dot.memberPrefix || ''}${m.label}`;
        push(m.label, m.kind, m.detail || `${dot.qualifier}.${m.label}`, insert, '0', filterText);
      }
      return { suggestions, linePrefix, prefix, range, seen };
    }

    if (langForPath(path) === 'java' && isInsideControlParen(linePrefix)) {
      const trimmed = linePrefix.trimEnd();
      if (trimmed.endsWith('for (') || trimmed.endsWith('for(')) {
        for (const prim of ['int', 'long', 'boolean', 'char', 'String', 'var']) {
          push(prim, 'keyword', 'type', prim, '0');
        }
      } else if (/\bfor\s*\([^)]*:/.test(trimmed)) {
        collectJavaScopeVariables(text, position.lineNumber).forEach(({ name, typeHint }) => {
          if (!prefix || name.toLowerCase().startsWith(prefix.toLowerCase())) {
            push(name, 'variable', typeHint || 'variable', name, '0');
          }
        });
      }
    }

    for (const sym of extractSymbols(text, path)) {
      if (!prefix || sym.name.toLowerCase().startsWith(prefix.toLowerCase())) {
        push(sym.name, sym.kind, sym.kind, sym.name, '1');
      }
    }

    for (const kw of clientKeywordsForPath(path)) {
      if (
        !shouldPreferAiStatementInline(path, linePrefix, text, position.lineNumber)
        && prefix.length >= minKwChars
        && kw.toLowerCase().startsWith(prefix.toLowerCase())
      ) {
        push(kw, 'keyword', 'keyword', kw, '2');
      }
    }

    return { suggestions, linePrefix, prefix, range, seen };
  }

  const JAVA_COLLECTION_PREFIXES = [
    'List<', 'ArrayList<', 'LinkedList<', 'Set<', 'HashSet<', 'LinkedHashSet<',
    'Collection<', 'Iterable<', 'Queue<', 'Deque<', 'ArrayDeque<',
  ];
  const JAVA_MAP_PREFIXES = ['Map<', 'HashMap<', 'LinkedHashMap<', 'TreeMap<', 'ConcurrentHashMap<'];

  function scanJavaGenericDecl(ctx, prefixes) {
    let best = null;
    for (const prefix of prefixes) {
      let search = 0;
      for (;;) {
        const idx = ctx.indexOf(prefix, search);
        if (idx < 0) break;
        const after = ctx.slice(idx + prefix.length);
        const endGt = after.indexOf('>');
        if (endGt < 0) {
          search = idx + prefix.length;
          continue;
        }
        const inner = after.slice(0, endGt).trim();
        const rest = after.slice(endGt + 1).trimStart();
        const nameMatch = rest.match(/^([A-Za-z_$]\w*)/);
        if (nameMatch) {
          if (!best || idx > best.idx) {
            best = { idx, inner, name: nameMatch[1] };
          }
        }
        search = idx + prefix.length;
      }
    }
    return best;
  }

  function lastJavaCollection(ctx) {
    return scanJavaGenericDecl(ctx, JAVA_COLLECTION_PREFIXES);
  }

  function lastJavaMap(ctx) {
    const hit = scanJavaGenericDecl(ctx, JAVA_MAP_PREFIXES);
    if (!hit) return null;
    const parts = hit.inner.split(',');
    const key = (parts[0] || 'Object').trim();
    const val = (parts[1] || 'Object').trim();
    return { key, val, name: hit.name, idx: hit.idx };
  }

  /** Latest List/array target for indexed `for` — prefers nearest declaration (e.g. `s` over `args`). */
  function bestJavaIndexedForTarget(ctx) {
    let best = null;
    const consider = (idx, name, useSize) => {
      if (!name) return;
      if (!best || idx > best.idx) best = { idx, name, useSize };
    };

    const typeArrayRe = /\b(?:String|int|byte|char|short|long|float|double|boolean|[A-Za-z_$][\w$]*)\s*\[\s*\]\s+([A-Za-z_$]\w*)/g;
    for (const m of ctx.matchAll(typeArrayRe)) {
      consider(m.index ?? 0, m[1], false);
    }
    const varargsRe = /\b(?:String|[A-Za-z_$][\w$]*)\s+\.\.\.\s+([A-Za-z_$]\w*)/g;
    for (const m of ctx.matchAll(varargsRe)) {
      consider(m.index ?? 0, m[1], false);
    }
    const cStyleRe = /\b([A-Za-z_$]\w*)\s*\[\s*\]\s+([A-Za-z_$]\w*)\s*[,)]/g;
    for (const m of ctx.matchAll(cStyleRe)) {
      consider(m.index ?? 0, m[2], false);
    }

    const coll = scanJavaGenericDecl(ctx, JAVA_COLLECTION_PREFIXES);
    if (coll) consider(coll.idx, coll.name, true);

    const typed = lastJavaTypedArray(ctx);
    if (typed) consider(typed.idx, typed.name, false);

    const arrayDecl = ctx.match(/\b([A-Za-z_]\w*)\s*\[\s*\]/);
    if (arrayDecl) consider(arrayDecl.index ?? 0, arrayDecl[1], false);

    if (!best) return null;
    const bound = best.useSize ? `${best.name}.size()` : `${best.name}.length`;
    return { idx: best.idx, init: `int i = 0; i < ${bound}; i++` };
  }

  function lastJavaTypedArray(ctx) {
    let best = null;
    const lines = ctx.split(/\r?\n/);
    let offset = 0;
    for (const line of lines) {
      const trimmed = line.trim();
      const bracket = trimmed.indexOf('[');
      if (bracket >= 0) {
        const before = trimmed.slice(0, bracket).trimEnd();
        const typePart = before.split(/\s+/).pop() || '';
        const rest = trimmed.slice(bracket);
        if (rest.startsWith('[') && rest.includes(']')) {
          const after = rest.slice(rest.indexOf(']') + 1).trimStart();
          const nameMatch = after.match(/^([A-Za-z_$]\w*)/);
          if (typePart && nameMatch) {
            const pos = offset + line.indexOf(trimmed);
            if (!best || pos > best.idx) {
              best = { idx: pos, type: typePart, name: nameMatch[1] };
            }
          }
        }
      }
      offset += line.length + 1;
    }
    return best;
  }

  function forEachVarName(init) {
    const t = String(init || '').trim();
    if (!t.includes(':') || t.includes(';')) return null;
    const lhs = t.slice(0, t.indexOf(':')).trim();
    const parts = lhs.split(/\s+/);
    return parts.length ? parts[parts.length - 1] : null;
  }

  function inferInlineCondition(path, content, lineNumber, kind, javaLevel = 17) {
    const lang = langForPath(path);
    const ctx = editorContextAroundLine(content, lineNumber);
    const boolVars = [];
    const addBool = (name) => {
      if (name && !boolVars.includes(name)) boolVars.push(name);
    };
    for (const m of ctx.matchAll(/\bboolean\s+([A-Za-z_]\w*)/g)) addBool(m[1]);
    for (const m of ctx.matchAll(/\bbool\s+([A-Za-z_]\w*)/g)) addBool(m[1]);
    for (const m of ctx.matchAll(/(?:let|var|const)\s+([A-Za-z_]\w*)\s*=\s*(?:true|false)/gi)) addBool(m[1]);

    const conds = [];
    for (const m of ctx.matchAll(/\b(?:if|while)\s*\(\s*([^);]+)\)/g)) {
      const c = m[1].trim();
      if (c && c !== 'condition' && c.length < 72) conds.push(c);
    }

    const isJavaLike = lang === 'java' || lang === 'kotlin' || lang === 'groovy';
    const isCLike = isJavaLike || lang === 'c' || lang === 'cpp' || lang === 'csharp' || lang === 'swift';

    if (kind === 'while') {
      if (isJavaLike) {
        const iterAssign = ctx.match(/\b([A-Za-z_]\w*)\s*=\s*[^;]*\.iterator\s*\(\s*\)/);
        if (iterAssign) return `${iterAssign[1]}.hasNext()`;
        if (/\bIterator\s*</.test(ctx) || /\biterator\s*\(\)/.test(ctx)) return 'iterator.hasNext()';
        const listDecl = ctx.match(/\b(?:List|ArrayList|LinkedList)<[^>]*>\s+([A-Za-z_]\w*)/);
        if (listDecl) return `i < ${listDecl[1]}.size()`;
        const coll = lastJavaCollection(ctx);
        if (coll) return `i < ${coll.name}.size()`;
        const arrayDecl = ctx.match(/\b([A-Za-z_]\w*)\s*\[\s*\]/);
        if (arrayDecl) return `i < ${arrayDecl[1]}.length`;
      }
      if (lang === 'javascript' || lang === 'typescript') {
        const arr = ctx.match(/\bconst\s+([A-Za-z_]\w*)\s*=\s*\[/);
        if (arr) return `i < ${arr[1]}.length`;
      }
      const flags = ['running', 'done', 'hasMore', 'active', 'valid', 'found'];
      for (const f of flags) {
        if (new RegExp(`\\b${f}\\b`).test(ctx)) {
          if (f === 'hasMore' && isJavaLike) return 'iterator.hasNext()';
          return f;
        }
      }
      if (boolVars.length) return boolVars[boolVars.length - 1];
      if (conds.length) return conds[conds.length - 1];
      if (isCLike) return 'true';
      if (lang === 'python') return 'True';
      return 'condition';
    }

    if (kind === 'if') {
      if (conds.length) return conds[conds.length - 1];
      if (boolVars.length) return boolVars[boolVars.length - 1];
      if (isCLike) return 'true';
      if (lang === 'python') return 'True';
      return 'condition';
    }

    return 'condition';
  }

  function keywordInlineSuffix(path, token) {
    if (!token) return '';
    const lower = token.toLowerCase();
    let best = null;
    for (const kw of clientKeywordsForPath(path)) {
      const kl = kw.toLowerCase();
      if (kl.startsWith(lower) && kw.length > token.length) {
        const suffix = kw.slice(token.length);
        if (!best || suffix.length < best.length) best = suffix;
      }
    }
    return best || '';
  }

  function isWhitespaceOnlyLine(linePrefix) {
    return !extractInlinePartialToken(linePrefix);
  }

  function findEnclosingBlock(content, lineNumber) {
    const lines = String(content || '').split(/\r?\n/);
    const cur = lineNumber - 1;
    if (cur < 0 || cur >= lines.length) return null;
    const curIndent = lineIndent(lines[cur]);

    for (let i = cur - 1; i >= 0; i--) {
      const line = lines[i];
      const ind = lineIndent(line);
      const trimmed = line.trimEnd();
      if (ind.length < curIndent.length && trimmed.endsWith('{')) {
        let m;
        if ((m = trimmed.match(/\bwhile\s*\(\s*([^)]*)\)\s*\{?\s*$/))) {
          return { type: 'while', cond: m[1].trim(), indent: curIndent };
        }
        if ((m = trimmed.match(/\bfor\s*\(\s*([^)]*)\)\s*\{?\s*$/))) {
          const inside = m[1].trim();
          const type = inside.includes(':') && !inside.includes(';') ? 'for-each' : 'for';
          return { type, init: inside, indent: curIndent };
        }
        if ((m = trimmed.match(/\bif\s*\(\s*([^)]*)\)\s*\{?\s*$/))) {
          return { type: 'if', cond: m[1].trim(), indent: curIndent };
        }
        if (/\belse\s*\{?\s*$/.test(trimmed)) {
          return { type: 'else', indent: curIndent };
        }
      }
      if (trimmed === '{' && i > 0) {
        const prev = lines[i - 1].trimEnd();
        let m;
        if ((m = prev.match(/\bwhile\s*\(\s*([^)]*)\)\s*$/))) {
          return { type: 'while', cond: m[1].trim(), indent: curIndent };
        }
        if ((m = prev.match(/\bfor\s*\(\s*([^)]*)\)\s*$/))) {
          const inside = m[1].trim();
          const type = inside.includes(':') && !inside.includes(';') ? 'for-each' : 'for';
          return { type, init: inside, indent: curIndent };
        }
        if ((m = prev.match(/\bif\s*\(\s*([^)]*)\)\s*$/))) {
          return { type: 'if', cond: m[1].trim(), indent: curIndent };
        }
      }
    }
    return null;
  }

  function sameIndentLinesInBlock(content, lineNumber, indent) {
    const lines = String(content || '').split(/\r?\n/);
    const cur = lineNumber - 1;
    const above = [];
    const below = [];
    for (let i = 0; i < lines.length; i++) {
      if (i === cur || !lines[i].trim()) continue;
      if (lineIndent(lines[i]) !== indent) continue;
      const entry = { i, text: lines[i].trim() };
      if (i < cur) above.push(entry);
      else below.push(entry);
    }
    return { above, below };
  }

  function inferEmptyLineContinuationFullLine(path, content, lineNumber, linePrefix, javaLevel = 17) {
    const block = findEnclosingBlock(content, lineNumber);
    if (!block) return '';
    const indent = lineIndent(linePrefix) || block.indent;
    const lang = langForPath(path);
    const { above, below } = sameIndentLinesInBlock(content, lineNumber, indent);

    if (lang === 'java' || lang === 'kotlin' || lang === 'groovy') {
      if (block.type === 'while') {
        const cond = block.cond;
        let m;
        if ((m = cond.match(/i\s*<\s*(\w+)\.size\s*\(\s*\)/))) {
          const list = m[1];
          const hasGet = above.some((x) => x.text.includes('.get(i)'));
          if (!hasGet) {
            return javaLevel >= 10
              ? `${indent}var item = ${list}.get(i);`
              : `${indent}${list}.get(i);`;
          }
          if (!above.some((x) => x.text === 'i++;' || /\bi\s*\+\+\s*;/.test(x.text))) {
            return `${indent}i++;`;
          }
        }
        if ((m = cond.match(/(\w+)\.hasNext\s*\(\s*\)/))) {
          const iter = m[1];
          if (!above.some((x) => x.text.includes('.next()'))) {
            return javaLevel >= 10
              ? `${indent}var item = ${iter}.next();`
              : `${indent}Object item = ${iter}.next();`;
          }
        }
      }
      if (block.type === 'for-each' || (block.type === 'for' && block.init.includes(':') && !block.init.includes(';'))) {
        const varName = forEachVarName(block.init);
        if (varName && !above.some((x) => x.text.includes(varName))) {
          if (block.init.includes('.entrySet()')) {
            return `${indent}var key = ${varName}.getKey();`;
          }
          return `${indent}System.out.println(${varName});`;
        }
      }
      if (block.type === 'for') {
        const init = block.init;
        let m;
        if ((m = init.match(/(?:var\s+)?(\w+)\s*:\s*(\w+)\s*$/))) {
          const varName = m[1];
          if (!above.length) return `${indent}System.out.println(${varName});`;
        }
        if (init.includes('i = 0') && init.includes('i++')) {
          const m2 = init.match(/i\s*<\s*(\w+)/);
          if (m2 && !above.some((x) => x.text.includes('.get(i)'))) {
            return `${indent}${m2[1]}.get(i);`;
          }
          if (m2 && above.some((x) => x.text.includes('.get(i)') && !above.some((x) => /\bi\s*\+\+\s*;/.test(x.text)))) {
            return `${indent}i++;`;
          }
        }
      }
      if ((block.type === 'if' || block.type === 'else') && !above.length) {
        return `${indent}// TODO`;
      }
    }

    if (lang === 'python') {
      if ((block.type === 'while' || block.type === 'for' || block.type === 'if') && !above.length) {
        return `${indent}pass`;
      }
    }

    if (!above.length && below.length) {
      const belowText = below[0].text;
      if (belowText.startsWith('return ')) return `${indent}// ...`;
    }

    return '';
  }

  function inferEmptyLineContinuationSuffix(path, content, lineNumber, linePrefix, javaLevel = 17) {
    const full = inferEmptyLineContinuationFullLine(path, content, lineNumber, linePrefix, javaLevel);
    if (!full) return '';
    if (full.startsWith(linePrefix)) return full.slice(linePrefix.length);
    const wantIndent = lineIndent(full);
    const haveIndent = lineIndent(linePrefix);
    if (wantIndent.length >= haveIndent.length) {
      return full.slice(haveIndent.length);
    }
    return full.trimStart();
  }

  /** Synchronous local ghost text — single-line suffixes only (blocks go in the suggest menu). */
  function localInlineSuggestion(path, linePrefix, content, lineNumber, javaLevel = 17, column = 0) {
    const springInline = springConfigLocalInline(path, linePrefix, content, lineNumber, column);
    if (springInline) return springInline;
    const member = memberInlineSuffix(content, linePrefix, path);
    if (member) return member;
    const syntax = syntaxInlineSuffix(path, linePrefix);
    if (syntax) return syntax;
    const trimmed = linePrefix.trimEnd();
    const token = extractInlinePartialToken(linePrefix);
    if (token && !hasCompleteControlKeyword(trimmed) && !isControlKeywordPrefix(token)) {
      const kwSuffix = keywordInlineSuffix(path, token);
      if (kwSuffix) return kwSuffix;
    }
    return '';
  }

  function isCodeLikeCompletion(label, kind) {
    const k = String(kind || '').toLowerCase();
    if (k === 'keyword' || k === 'method' || k === 'field' || k === 'class' || k === 'interface'
      || k === 'variable' || k === 'property' || k === 'value' || k === 'enum' || k === 'function' || k === 'type'
      || k === 'snippet' || k === 'statement') {
      return true;
    }
    const t = String(label ?? '').trim();
    if (!t || t.length > 120) return false;
    if (/^[A-Za-z_$@][\w$.]*$/.test(t)) return true;
    if (/^[A-Za-z_$][\w$]*\(/.test(t)) return true;
    if (/[();.=\[\]{}<>]/.test(t)) return true;
    return false;
  }

  function isKeywordPrefixTyping(path, prefix) {
    if (!prefix) return false;
    const lower = prefix.toLowerCase();
    return clientKeywordsForPath(path).some((kw) => kw.toLowerCase().startsWith(lower));
  }

  function buildInlineItems(model, position, linePrefix, text) {
    const suffix = capInlineText(sanitizeInlineGhostText(text));
    if (!suffix) return { items: [] };
    const token = extractInlinePartialToken(linePrefix);
    const filterText = token && !suffix.startsWith(token) ? token + suffix : suffix;
    return {
      items: [{
        insertText: suffix,
        filterText,
        range: new monaco.Range(
          position.lineNumber,
          position.column,
          position.lineNumber,
          position.column,
        ),
      }],
    };
  }

  function shouldFetchCompletions(linePrefix, prefix) {
    const p = prefix || '';
    if (p.length >= MIN_AUTOCOMPLETE_CHARS) return true;
    return AUTOCOMPLETE_TRIGGER_RE.test(linePrefix || '');
  }

  function localSmartCompletions(path, content, lineNumber, linePrefix, javaLevel) {
    const inline = localInlineSuggestion(path, linePrefix, content, lineNumber, javaLevel);
    if (!inline) return [];
    const firstLine = inline.split('\n')[0];
    const label = firstLine.length > 52 ? `${firstLine.slice(0, 49)}…` : firstLine;
    return [{
      label,
      kind: 'snippet',
      detail: 'Next code',
      insert: inline,
    }];
  }

  const INDEX_COMPLETE_CACHE_MS = 2000;
  const INDEX_COMPLETE_CACHE_MAX = 96;
  const indexCompleteCache = new Map();

  function indexCompleteCacheKey(repo, path, line, column, prefix, contentLen) {
    return `${repo}:${path}:${line}:${column}:${prefix}:${contentLen}`;
  }

  function pruneIndexCompleteCache() {
    if (indexCompleteCache.size <= INDEX_COMPLETE_CACHE_MAX) return;
    const first = indexCompleteCache.keys().next().value;
    if (first) indexCompleteCache.delete(first);
  }

  function readCachedIndexCompletions(helpers, model, position, prefix) {
    const repo = helpers.getRepo?.();
    const path = helpers.getActivePath?.() || '';
    if (!repo || !path) return null;
    const contentLen = model.getValue()?.length ?? 0;
    const cacheKey = indexCompleteCacheKey(
      repo, path, position.lineNumber, position.column, prefix || '', contentLen,
    );
    const cached = indexCompleteCache.get(cacheKey);
    if (cached && Date.now() - cached.time < INDEX_COMPLETE_CACHE_MS) {
      return cached.items;
    }
    return null;
  }

  function mergeIndexItemsIntoSuggestions(items, range, seen, memberContext, suggestions, limit = 80) {
    if (!Array.isArray(items)) return;
    for (const item of items) {
      const mapped = mapIndexItemToSuggestion(item, range, seen, memberContext);
      if (mapped) suggestions.push(mapped);
      if (suggestions.length >= limit) break;
    }
  }

  async function fetchAiCompletions(helpers, model, position, linePrefix, prefix, cache) {
    if (!helpers.getGeminiConfigured?.()) return [];
    const repo = helpers.getRepo();
    const path = helpers.getActivePath?.() || '';
    if (!repo || !path || !helpers.repoApi) return [];

    const cacheKey = `${repo}:${path}:${position.lineNumber}:${position.column}:${linePrefix}:${prefix}`;
    if (cache.key === cacheKey && Date.now() - cache.time < 1200) {
      return cache.items;
    }

    const url = helpers.repoApi(repo, '/workspace/ai-completions');
    const payload = contentForApiPayload(model.getValue(), position.lineNumber);
    const items = await helpers.api(url, {
      method: 'POST',
      body: JSON.stringify({
        path,
        line: payload.line,
        column: position.column,
        content: payload.content,
        line_prefix: linePrefix,
        prefix: prefix || '',
      }),
    });
    const list = Array.isArray(items) ? items : [];
    cache.key = cacheKey;
    cache.items = list;
    cache.time = Date.now();
    return list;
  }

  function clearIndexCompleteCache() {
    indexCompleteCache.clear();
  }

  function overlayFingerprint(helpers, excludePath) {
    const overlays = helpers.getJavaSourceOverlays?.(excludePath) || [];
    let hash = 2166136261;
    for (const o of overlays) {
      for (let i = 0; i < o.path.length; i += 1) {
        hash ^= o.path.charCodeAt(i);
        hash = Math.imul(hash, 16777619);
      }
      hash ^= o.content.length;
      hash = Math.imul(hash, 16777619);
    }
    return hash >>> 0;
  }

  async function fetchCompletions(helpers, model, position, prefix) {
    const repo = helpers.getRepo();
    const path = helpers.getActivePath?.() || '';
    if (!repo || !path) return [];

    const line = position.lineNumber;
    const column = position.column;
    const dirty = helpers.isFileDirty?.(path);
    const content = model.getValue();
    const needsLiveContent = dirty || /\.(java|kt|kts|groovy)$/i.test(path) || isSpringConfigFile(path);
    const contentLen = content?.length ?? 0;
    const overlayHash = overlayFingerprint(helpers, path);
    const cacheKey = `${indexCompleteCacheKey(repo, path, line, column, prefix || '', contentLen)}:${overlayHash}`;
    const cached = indexCompleteCache.get(cacheKey);
    if (cached && Date.now() - cached.time < INDEX_COMPLETE_CACHE_MS) {
      return cached.items;
    }

    const url = helpers.repoApi(repo, '/workspace/completions');
    let items;

    try {
      if (needsLiveContent) {
        const payload = contentForApiPayload(content, line);
        const overlays = helpers.getJavaSourceOverlays?.(path) || [];
        items = await helpers.api(url, {
          method: 'POST',
          body: JSON.stringify({
            path,
            line: payload.line,
            column,
            prefix: prefix || '',
            content: payload.content,
            overlays,
          }),
        });
      } else {
        const q = new URLSearchParams({
          path,
          line: String(line),
          column: String(column),
          prefix: prefix || '',
        });
        items = await helpers.api(`${url}?${q}`);
      }
    } catch {
      return [];
    }

    const list = Array.isArray(items) ? items : [];
    indexCompleteCache.set(cacheKey, { items: list, time: Date.now() });
    pruneIndexCompleteCache();
    return list;
  }

  const COMPLETION_FETCH_TIMEOUT_MS = 8000;
  const INDEX_COMPLETION_BUDGET_MS = 600;

  function fetchCompletionsWithTimeout(helpers, model, position, prefix, timeoutMs = COMPLETION_FETCH_TIMEOUT_MS) {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error('Completions timed out')), timeoutMs);
      fetchCompletions(helpers, model, position, prefix)
        .then((items) => {
          clearTimeout(timer);
          resolve(items);
        })
        .catch((err) => {
          clearTimeout(timer);
          reject(err);
        });
    });
  }

  const definitionCache = new Map();
  const hoverCache = new Map();
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

  async function lookupHoverInfo(helpers, model, position, { member, symbol } = {}) {
    if (!helpers.repoApi || !helpers.getRepo) return null;
    const repo = helpers.getRepo();
    if (!repo) return null;
    const path = helpers.getActivePath?.() || '';
    if (!path) return null;

    const line = position.lineNumber;
    const column = position.column;
    const dirty = helpers.isFileDirty?.(path);
    const content = dirty ? model.getValue() : undefined;
    const extraKey = member ? `:member:${member}` : symbol ? `:symbol:${symbol}` : '';
    const cacheKey = `${definitionCacheKey(repo, path, line, column, content ?? model.getValue())}${extraKey}`;
    if (hoverCache.has(cacheKey)) {
      return hoverCache.get(cacheKey);
    }

    try {
      const url = helpers.repoApi(repo, '/workspace/hover');
      const payload = {
        path,
        line,
        column,
        content,
        ...(member ? { member } : {}),
        ...(symbol ? { symbol } : {}),
      };
      const hit = dirty || member || symbol
        ? await helpers.api(url, { method: 'POST', body: JSON.stringify(payload) })
        : await helpers.api(`${url}?${new URLSearchParams({
            path,
            line: String(line),
            column: String(column),
            ...(member ? { member } : {}),
            ...(symbol ? { symbol } : {}),
          })}`);
      const info = hit?.name ? hit : null;
      if (info) {
        if (hoverCache.size >= DEF_CACHE_MAX) hoverCache.clear();
        hoverCache.set(cacheKey, info);
      }
      return info;
    } catch {
      return null;
    }
  }

  async function lookupSymbolHover(helpers, model, position) {
    const info = await lookupHoverInfo(helpers, model, position);
    return info?.name ? info : null;
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
    completionDebug(helpers, ['debug active', `build=${document.querySelector('meta[name="reaper-ui-build"]')?.content?.trim() || '?'}`]);

    registerGroovy();

    const langs = new Set(ALL_EDITOR_LANGS);
    const activeEditor = () => helpers.getEditor?.() || editor;

    function runSuggestAction(ed) {
      const action = ed.getAction('editor.action.triggerSuggest');
      if (action) {
        action.run();
        return;
      }
      ed.trigger('keyboard', 'editor.action.triggerSuggest', {});
    }

    function openSuggestWidget(ed, { delayMs = 0 } = {}) {
      const run = () => {
        requestAnimationFrame(() => {
          runSuggestAction(ed);
          setTimeout(() => {
            const visible = ed._contextKeyService?.getContextKeyValue('suggestWidgetVisible');
            if (!visible) runSuggestAction(ed);
          }, 100);
        });
      };
      if (delayMs > 0) setTimeout(run, delayMs);
      else run();
    }

    let memberFallbackEl = null;
    let memberFallbackItems = [];
    let memberFallbackAllItems = [];
    let memberFallbackIndex = 0;
    let memberDocFetchGen = 0;

    function hideMemberSuggestFallback() {
      if (memberFallbackEl) {
        memberFallbackEl.remove();
        memberFallbackEl = null;
      }
      memberFallbackItems = [];
      memberFallbackAllItems = [];
      memberFallbackIndex = 0;
    }

    function filterVisibleSuggestions(items, model, position) {
      return items.filter((item) => {
        const label = memberFallbackLabel(item);
        const range = item.range || completionContext(model, position).range;
        const typed = model.getValueInRange(range);
        if (!typed) return true;
        if (typed === label || typed.startsWith(`${label}(`)) return false;
        return label.toLowerCase().startsWith(typed.toLowerCase());
      });
    }

    function dismissMonacoSuggestWidget(ed) {
      if (!ed) return;
      try {
        ed.trigger('keyboard', 'hideSuggestWidget', null);
      } catch {
        /* best-effort */
      }
    }

    function presentCompletionSuggestions(ed, items, { content, path } = {}) {
      if (!items?.length) return false;
      const model = ed?.getModel();
      const position = ed?.getPosition();
      if (!model || !position) return false;
      const text = content ?? model.getValue();
      const filePath = path || helpers.getActivePath?.() || '';
      for (const item of items) {
        enrichJavaSuggestion(item, text, filePath);
      }
      showMemberSuggestFallback(ed, items);
      return true;
    }

    function memberFallbackLabel(item) {
      if (!item) return '';
      let label = typeof item.label === 'string' ? item.label : (item.label?.label || '');
      label = String(label).trim();
      if (label.endsWith('()')) label = label.slice(0, -2);
      const dot = label.lastIndexOf('.');
      if (dot >= 0) label = label.slice(dot + 1);
      return label;
    }

    function memberKindKey(item) {
      return memberKindFromItem(item);
    }

    function memberKindLetter(kindKey) {
      if (kindKey === 'method') return 'm';
      if (kindKey === 'field') return 'f';
      return '·';
    }

    function memberItemDetail(item) {
      return item.detail ? String(item.detail) : '';
    }

    async function updateMemberFallbackDoc() {
      if (!memberFallbackEl) return;
      const sigEl = memberFallbackEl.querySelector('.reaper-member-suggest-doc-sig');
      const bodyEl = memberFallbackEl.querySelector('.reaper-member-suggest-doc-body');
      const item = memberFallbackItems[memberFallbackIndex];
      if (!item || !bodyEl) return;
      const kindKey = memberKindKey(item);
      const label = memberFallbackLabel(item);
      const fetchGen = ++memberDocFetchGen;

      const ed = activeEditor();
      const model = ed?.getModel();
      const position = ed?.getPosition();
      const path = helpers.getActivePath?.() || '';
      const content = model?.getValue() || '';
      const qualifier = memberDotQualifierFromEditor(ed, model);

      enrichItemDocumentationFromSource(item, content, path);
      let info = hoverInfoFromItem(item, content, path);
      info.signature = memberDisplaySignature(item, qualifier) || info.signature;
      if (!info.documentation) {
        info.documentation = memberDocFallback(item, qualifier);
      }

      applyHoverInfoToDocPanel(sigEl, bodyEl, info, kindKey);
      const hasLocalDoc = !!(info.documentation || info.documentationHtml);
      if (!hasLocalDoc && !info.signature) {
        bodyEl.innerHTML = '<div class="reaper-doc-loading">Loading documentation…</div>';
      } else if (!hasLocalDoc) {
        const loading = document.createElement('div');
        loading.className = 'reaper-doc-loading';
        loading.textContent = 'Loading documentation…';
        bodyEl.innerHTML = '';
        bodyEl.appendChild(loading);
      }

      if (!model || !position || !path) return;

      const hoverOpts = qualifier
        ? { member: label }
        : { symbol: label };
      const remote = await lookupHoverInfo(helpers, model, position, hoverOpts);
      if (fetchGen !== memberDocFetchGen || !memberFallbackEl) return;

      const fallbackDoc = memberDocFallback(item, qualifier);
      const fallbackSig = memberDisplaySignature(item, qualifier) || info.signature;
      const localDoc = info.documentation || info.documentationHtml || '';

      if (!remote) {
        applyHoverInfoToDocPanel(sigEl, bodyEl, {
          ...info,
          signature: fallbackSig,
          documentation: info.documentation || fallbackDoc,
        }, kindKey);
        return;
      }

      if (remote.signature) item.detail = remote.signature;
      if (remote.documentation) item.documentation = remote.documentation;
      info = {
        name: remote.name || label,
        kind: remote.kind || kindKey,
        signature: remote.signature || fallbackSig,
        documentation: remote.documentation || info.documentation || fallbackDoc,
        documentationHtml: remote.documentation ? '' : info.documentationHtml,
      };
      if (!info.documentation && !info.documentationHtml && !fallbackDoc) {
        applyHoverInfoToDocPanel(sigEl, bodyEl, info, kindKey);
        return;
      }
      if (!info.documentation && !info.documentationHtml && fallbackDoc) {
        info.documentation = fallbackDoc;
      }
      applyHoverInfoToDocPanel(sigEl, bodyEl, info, kindKey);
    }

    function focusMemberFallbackRow() {
      if (!memberFallbackEl) return;
      const rows = memberFallbackEl.querySelectorAll('.reaper-member-suggest-row');
      rows.forEach((row, i) => {
        row.classList.toggle('focused', i === memberFallbackIndex);
      });
      const focused = rows[memberFallbackIndex];
      if (focused) focused.scrollIntoView({ block: 'nearest' });
      updateMemberFallbackDoc();
    }

    function acceptMemberFallbackItem(ed) {
      const item = memberFallbackItems[memberFallbackIndex];
      const model = ed.getModel();
      const position = ed.getPosition();
      if (!item || !model || !position) return;
      let text = item.insertText ?? memberFallbackLabel(item);
      if (typeof text !== 'string') text = memberFallbackLabel(item);
      const range = item.range || completionContext(model, position).range;
      ed.executeEdits('reaper-member', [{ range, text }]);
      ed._reaperMemberAcceptedUntil = Date.now() + 500;
      hideMemberSuggestFallback();
      ed.focus();
      requestAnimationFrame(() => hideMemberSuggestFallback());
    }

    function memberSuggestCoords(ed, position) {
      const coords = ed.getScrolledVisiblePosition(position);
      if (coords) {
        return { left: coords.left, top: coords.top + coords.height + 2 };
      }
      try {
        const layout = ed.getLayoutInfo();
        const top = ed.getTopForLineNumber(position.lineNumber) - ed.getScrollTop() + layout.contentTop;
        const left = ed.getOffsetForColumn(position.lineNumber, position.column)
          - ed.getScrollLeft() + layout.contentLeft;
        if (Number.isFinite(top) && Number.isFinite(left)) {
          const lineHeight = ed.getOption(monaco.editor.EditorOption.lineHeight);
          return { left, top: top + lineHeight + 2 };
        }
      } catch {
        /* layout not ready */
      }
      const dom = ed.getDomNode()?.querySelector('.view-lines');
      if (dom) {
        const rect = dom.getBoundingClientRect();
        const root = document.getElementById('editor-overflow-root')?.getBoundingClientRect();
        if (root) {
          return { left: rect.left - root.left + 8, top: rect.top - root.top + 24 };
        }
      }
      return null;
    }

    function showMemberSuggestFallback(ed, items) {
      hideMemberSuggestFallback();
      if (!items.length) {
        completionDebug(helpers, ['fallback', 'skip: no items']);
        return;
      }
      const model = ed.getModel();
      const position = ed.getPosition();
      const root = document.getElementById('editor-overflow-root');
      if (!model || !position || !root) {
        completionDebug(helpers, [
          'fallback', 'skip: no model/root',
          !model ? 'no-model' : '',
          !position ? 'no-pos' : '',
          !root ? 'no-root' : '',
        ], { warn: true });
        return;
      }
      dismissMonacoSuggestWidget(ed);
      const pt = memberSuggestCoords(ed, position);
      if (!pt) {
        completionDebug(helpers, ['fallback', 'skip: no coords'], { warn: true });
        return;
      }

      memberFallbackAllItems = items;
      memberFallbackIndex = 0;
      const visibleItems = filterVisibleSuggestions(items, model, position);
      if (!visibleItems.length) {
        hideMemberSuggestFallback();
        return;
      }
      memberFallbackItems = visibleItems;

      memberFallbackEl = document.createElement('div');
      memberFallbackEl.className = 'reaper-member-suggest visible';
      memberFallbackEl.style.left = `${pt.left}px`;
      memberFallbackEl.style.top = `${pt.top}px`;

      const panel = document.createElement('div');
      panel.className = 'reaper-member-suggest-panel';

      const listEl = document.createElement('div');
      listEl.className = 'reaper-member-suggest-list';

      visibleItems.forEach((item, i) => {
        const row = document.createElement('div');
        row.className = `reaper-member-suggest-row${i === 0 ? ' focused' : ''}`;
        const kindKey = memberKindKey(item);
        const icon = document.createElement('span');
        icon.className = `reaper-member-suggest-kind ${kindKey}`;
        icon.textContent = memberKindLetter(kindKey);
        icon.setAttribute('aria-hidden', 'true');
        const name = document.createElement('span');
        name.className = 'reaper-member-suggest-name';
        name.textContent = memberFallbackLabel(item);
        row.appendChild(icon);
        row.appendChild(name);
        row.addEventListener('mouseenter', () => {
          memberFallbackIndex = i;
          focusMemberFallbackRow();
        });
        row.addEventListener('mousedown', (e) => {
          e.preventDefault();
          memberFallbackIndex = i;
          acceptMemberFallbackItem(ed);
        });
        listEl.appendChild(row);
      });

      const docPanel = document.createElement('div');
      docPanel.className = 'reaper-member-suggest-doc panel-scroll';
      const docHead = document.createElement('div');
      docHead.className = 'reaper-member-suggest-doc-head';
      docHead.textContent = 'Documentation';
      const sigEl = document.createElement('div');
      sigEl.className = 'reaper-member-suggest-doc-sig reaper-java-hover-sig';
      const bodyEl = document.createElement('div');
      bodyEl.className = 'reaper-member-suggest-doc-body reaper-java-hover-doc';
      docPanel.appendChild(docHead);
      docPanel.appendChild(sigEl);
      docPanel.appendChild(bodyEl);

      panel.appendChild(listEl);
      panel.appendChild(docPanel);
      memberFallbackEl.appendChild(panel);

      root.appendChild(memberFallbackEl);
      updateMemberFallbackDoc();
      completionDebug(helpers, [
        'fallback',
        `n=${visibleItems.length}`,
        visibleItems.slice(0, 6).map((s) => memberFallbackLabel(s)).join(','),
      ]);
    }

    function suggestPopupShouldStayOpen(model, position) {
      const path = helpers.getActivePath?.() || '';
      const { linePrefix, prefix, range, memberCtx } = completionContext(model, position);
      if (position.lineNumber !== range.startLineNumber) return false;
      if (position.column < range.startColumn) return false;
      if (!shouldFetchIndexCompletions(linePrefix, prefix, path)) return false;
      if (memberCtx || dotQualifierFromLinePrefix(linePrefix)) return true;
      const tokenEnd = range.startColumn + Math.max((prefix || '').length, 0);
      return position.column <= tokenEnd;
    }

    function dismissSuggestUi(ed) {
      hideMemberSuggestFallback();
      dismissMonacoSuggestWidget(ed || activeEditor());
    }

    function completionUiBlocksTyping(ed) {
      if (!ed) return false;
      if (memberFallbackEl) return true;
      const ctx = ed._contextKeyService;
      return !!(ctx?.getContextKeyValue('suggestWidgetVisible')
        || ctx?.getContextKeyValue('inlineSuggestionVisible'));
    }

    function editorIsTypingTarget(ed) {
      if (!ed) return false;
      if (ed.hasTextFocus?.()) return true;
      const root = ed.getDomNode?.();
      return !!(root && root.contains(document.activeElement));
    }

    function cursorAfterIdentifier(model, position) {
      if (!model || !position || position.column <= 1) return false;
      const line = model.getLineContent(position.lineNumber);
      return /[\w$]/.test(line.charAt(position.column - 2));
    }

    function typeThroughCompletion(ed, text) {
      clearTimeout(ed._reaperSuggestTimer);
      hideMemberSuggestFallback();
      dismissMonacoSuggestWidget(ed);
      dismissInlineGhost(ed);
      const model = ed.getModel();
      const pos = ed.getPosition();
      if (!model || !pos || text == null || text === '') return;
      ed.executeEdits('reaper-type-through', [{
        range: new monaco.Range(pos.lineNumber, pos.column, pos.lineNumber, pos.column),
        text,
      }]);
      ed.focus();
    }

    function onCompletionTypeThroughKeydown(e) {
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      const ed = activeEditor();
      if (!ed || !editorIsTypingTarget(ed)) return;
      const model = ed.getModel();
      const pos = ed.getPosition();
      if (!model || !pos) return;

      const blocking = completionUiBlocksTyping(ed);
      const afterId = cursorAfterIdentifier(model, pos);
      if (!blocking && !afterId) return;

      if (e.key === ' ') {
        e.preventDefault();
        e.stopPropagation();
        e.stopImmediatePropagation();
        typeThroughCompletion(ed, ' ');
        return;
      }
      if (e.key.length === 1 && /[^a-zA-Z0-9_$]/.test(e.key)) {
        e.preventDefault();
        e.stopPropagation();
        e.stopImmediatePropagation();
        typeThroughCompletion(ed, e.key);
      }
    }

    function onMemberFallbackKeydown(e) {
      if (!memberFallbackEl) return;
      const ed = activeEditor();
      if (e.key === ' ' || (e.key.length === 1 && /[^a-zA-Z0-9_$]/.test(e.key))) {
        return;
      }
      if (e.key === 'ArrowLeft' || e.key === 'ArrowRight' || e.key === 'Home' || e.key === 'End') {
        hideMemberSuggestFallback();
        return;
      }
      if (e.key === 'Escape' || e.key === 'Tab') {
        hideMemberSuggestFallback();
        if (e.key === 'Escape') {
          e.preventDefault();
          e.stopPropagation();
        }
        return;
      }
      if (e.key === 'ArrowDown') {
        memberFallbackIndex = (memberFallbackIndex + 1) % memberFallbackItems.length;
        focusMemberFallbackRow();
        e.preventDefault();
        e.stopPropagation();
        return;
      }
      if (e.key === 'ArrowUp') {
        memberFallbackIndex = (memberFallbackIndex - 1 + memberFallbackItems.length)
          % memberFallbackItems.length;
        focusMemberFallbackRow();
        e.preventDefault();
        e.stopPropagation();
        return;
      }
      if (e.key === 'Enter') {
        acceptMemberFallbackItem(ed);
        e.preventDefault();
        e.stopPropagation();
      }
    }

    function refreshSuggestFallbackFilter() {
      if (!memberFallbackEl || !memberFallbackAllItems.length) return;
      const ed = activeEditor();
      const model = ed?.getModel();
      const position = ed?.getPosition();
      if (!model || !position) {
        hideMemberSuggestFallback();
        return;
      }
      if (!suggestPopupShouldStayOpen(model, position)) {
        hideMemberSuggestFallback();
        return;
      }
      const visibleItems = filterVisibleSuggestions(memberFallbackAllItems, model, position);
      if (!visibleItems.length) {
        hideMemberSuggestFallback();
        return;
      }
      if (visibleItems.length === memberFallbackItems.length
        && visibleItems.every((item, i) => item === memberFallbackItems[i])) {
        return;
      }
      memberFallbackIndex = Math.min(memberFallbackIndex, visibleItems.length - 1);
      showMemberSuggestFallback(ed, memberFallbackAllItems);
    }

    document.addEventListener('keydown', onCompletionTypeThroughKeydown, true);
    document.addEventListener('keydown', onMemberFallbackKeydown, true);
    document.addEventListener('mousedown', (e) => {
      if (!memberFallbackEl) return;
      if (memberFallbackEl.contains(e.target)) return;
      hideMemberSuggestFallback();
    }, true);
    editor.onDidChangeModelContent(() => {
      if (!memberFallbackEl) return;
      refreshSuggestFallbackFilter();
    });
    editor.onDidChangeCursorPosition(() => {
      if (!memberFallbackEl) return;
      refreshSuggestFallbackFilter();
    });
    editor.onDidBlurEditorWidget(() => hideMemberSuggestFallback());

    let lastMemberSuggestKey = '';
    let lastMemberSuggestAt = 0;

    function fireCompletionsSuggest(ed, { force = false } = {}) {
      const model = ed.getModel();
      const position = ed.getPosition();
      if (!model || !position) return;
      const path = helpers.getActivePath?.() || '';
      if (!path) {
        if (force) helpers.toast?.('Open a file first', 'info');
        return;
      }
      const repo = helpers.getRepo?.();

      const { linePrefix, prefix } = completionContext(model, position);
      const memberContext = dotQualifierFromLinePrefix(linePrefix);
      const completionPrefix = memberContext
        ? (memberContext.memberPrefix || prefix || '')
        : (prefix || '');
      if (
        !force
        && !memberContext
        && !shouldFetchIndexCompletions(linePrefix, completionPrefix, path)
      ) {
        return;
      }

      if (memberContext && !force) {
        ensureModelLanguageForPath(path, model);
        const dedupeKey = `${path}:${position.lineNumber}:${position.column}:${memberContext.qualifier}`;
        const now = Date.now();
        if (dedupeKey === lastMemberSuggestKey && now - lastMemberSuggestAt < 180) {
          return;
        }
        lastMemberSuggestKey = dedupeKey;
        lastMemberSuggestAt = now;
      } else if (memberContext) {
        ensureModelLanguageForPath(path, model);
      }

      const content = editorContent(ed, model);
      const effLang = effectiveEditorLang(path, model);
      let localN = 0;
      let localLabels = '';
      let jdkN = 0;
      let localSuggestions = [];
      const local = buildLocalCompletionSuggestions(model, position, path, helpers, content);
      localSuggestions = local.suggestions;
      localN = localSuggestions.length;
      localLabels = localSuggestions.slice(0, 6).map((s) => s.label).join(',');
      if (memberContext) {
        jdkN = memberPreviewItems(
          content, memberContext.qualifier, memberContext.memberPrefix, path, model,
        ).length;
      }

      completionDebug(helpers, [
        'fireSuggest',
        force ? 'force' : 'auto',
        memberContext ? `member=${memberContext.qualifier}` : '',
        memberContext ? `jdkN=${jdkN}` : '',
        memberContext ? `localN=${localN}` : '',
        `lang=${effLang}`,
        `model=${model.getLanguageId()}`,
        path.split('/').pop() || path,
        localLabels ? `items=${localLabels}` : (memberContext ? 'items=(none)' : ''),
      ], { warn: !!(memberContext && localN === 0 && jdkN === 0) });

      const showSuggestPopup = (merged) => {
        if (!merged.length) return;
        for (const item of merged) {
          enrichJavaSuggestion(item, content, path);
        }
        showMemberSuggestFallback(ed, merged);
      };

      const showMemberMerged = (fromIndex) => {
        if (!fromIndex.length && !localSuggestions.length) return;
        const merged = localSuggestions.length > 0
          ? [...localSuggestions]
          : fromIndex;
        if (localSuggestions.length > 0 && fromIndex.length > 0) {
          const labels = new Set(localSuggestions.map((s) => s.label));
          for (const item of fromIndex) {
            if (!labels.has(item.label)) merged.push(item);
          }
        }
        showSuggestPopup(merged);
      };

      void (repo ? fetchCompletionsWithTimeout(helpers, model, position, completionPrefix)
        .then((items) => {
          if (!Array.isArray(items) || items.length === 0) return;
          const { range } = completionContext(model, position);
          const seen = new Set(localSuggestions.map((s) => s.label));
          const fromIndex = [];
          mergeIndexItemsIntoSuggestions(items, range, seen, memberContext, fromIndex);
          if (memberContext) {
            showMemberMerged(fromIndex);
            return;
          }
          const merged = localSuggestions.length > 0
            ? [...localSuggestions]
            : fromIndex;
          if (localSuggestions.length > 0) {
            for (const item of fromIndex) {
              if (!seen.has(item.label)) merged.push(item);
            }
          }
          showSuggestPopup(merged);
        })
        .catch(() => {}) : Promise.resolve());
      if (memberContext) {
        if (localSuggestions.length > 0) {
          showSuggestPopup(localSuggestions);
        }
        return;
      }
      if (localN > 0) {
        showSuggestPopup(localSuggestions);
      }
    }

    function scheduleSpringConfigInline(ed) {
      clearTimeout(ed._reaperSpringInlineTimer);
      ed._reaperSpringInlineTimer = setTimeout(async () => {
        const model = ed?.getModel?.();
        const position = ed?.getPosition?.();
        const path = helpers.getActivePath?.() || '';
        const repo = helpers.getRepo?.();
        if (!model || !position || !path || !repo || !isSpringConfigFile(path)) return;
        if (!helpers.repoApi) return;
        const linePrefix = editorLinePrefix(model, position);
        const content = editorContent(ed, model);
        const keyPrefix = springConfigKeyPrefix(
          path, linePrefix, content, position.lineNumber, position.column,
        );
        if (!keyPrefix || keyPrefix.length < 1) return;
        const local = springConfigLocalInline(
          path, linePrefix, content, position.lineNumber, position.column,
        );
        const cacheKey = buildInlineCacheKey(
          repo, path, position.lineNumber, position.column, linePrefix,
        );
        const meta = inlineCacheMeta(repo, path, position, linePrefix);
        if (local) {
          setInlineCache(ed, cacheKey, local, '', meta);
          queueInlineSuggestion(ed);
          return;
        }
        try {
          const items = await fetchCompletionsWithTimeout(
            helpers, model, position, keyPrefix, 500,
          );
          if (!Array.isArray(items) || items.length === 0) return;
          const suffix = completionSuffixFromLabel(items[0].label, keyPrefix);
          if (!suffix) return;
          setInlineCache(ed, cacheKey, suffix, '', meta);
          queueInlineSuggestion(ed);
        } catch {
          // best-effort
        }
      }, 120);
    }

    function scheduleMemberCompletions(ed) {
      clearTimeout(ed._reaperMemberSuggestTimer);
      ed._reaperMemberSuggestTimer = setTimeout(() => {
        if (typeof reaperDotCompletionHandler === 'function') {
          reaperDotCompletionHandler(ed);
        } else {
          fireCompletionsSuggest(ed, { force: true });
        }
      }, 20);
    }

    function scheduleAutocompleteSuggest(ed, force) {
      clearTimeout(ed._reaperSuggestTimer);
      const delay = force ? 0 : 280;
      ed._reaperSuggestTimer = setTimeout(() => {
        fireCompletionsSuggest(ed, { force });
      }, delay);
    }

    function handleDotCompletion(ed = editor) {
      if (ed._reaperMemberAcceptedUntil && Date.now() < ed._reaperMemberAcceptedUntil) {
        return 0;
      }
      const model = ed?.getModel?.();
      const position = ed?.getPosition?.();
      const path = helpers.getActivePath?.() || '';
      if (!model || !position || !path) {
        completionDebug(helpers, ['handleDot', 'skip: no model/path']);
        return 0;
      }
      ensureModelLanguageForPath(path, model);
      const expectedLang = langForPath(path);
      const actualLang = model.getLanguageId();
      if (expectedLang && actualLang !== expectedLang) {
        ensureMonacoBasicLanguage(expectedLang);
        monaco.editor.setModelLanguage(model, expectedLang);
      }
      const { linePrefix } = completionContext(model, position);
      const memberContext = dotQualifierFromLinePrefix(linePrefix);
      if (!memberContext) {
        completionDebug(helpers, [
          'handleDot',
          'skip: not member ctx',
          `line=…${linePrefix.slice(-24)}`,
        ]);
        return 0;
      }
      const content = editorContent(ed, model);
      const local = buildLocalCompletionSuggestions(model, position, path, helpers, content);
      completionDebug(helpers, [
        'handleDot',
        `lang=${model.getLanguageId()}`,
        `member=${memberContext.qualifier}`,
        `n=${local.suggestions.length}`,
        local.suggestions.slice(0, 6).map((s) => s.label).join(',') || '(none)',
      ], { warn: local.suggestions.length === 0 });
      if (local.suggestions.length > 0) {
        for (const item of local.suggestions) {
          enrichJavaSuggestion(item, content, path);
        }
        showMemberSuggestFallback(ed, local.suggestions);
      }
      fireCompletionsSuggest(ed, { force: true });
      return local.suggestions.length;
    }

    reaperDotCompletionHandler = handleDotCompletion;

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

    function positionInMarker(position, marker) {
      if (position.lineNumber < marker.startLineNumber
        || position.lineNumber > marker.endLineNumber) {
        return false;
      }
      if (position.lineNumber === marker.startLineNumber
        && position.column < marker.startColumn) {
        return false;
      }
      if (position.lineNumber === marker.endLineNumber
        && position.column > marker.endColumn) {
        return false;
      }
      return true;
    }

    function markersAtEditorPosition(model, position) {
      return monaco.editor.getModelMarkers({ resource: model.uri })
        .filter((m) => position.lineNumber >= m.startLineNumber
          && position.lineNumber <= m.endLineNumber);
    }

    function hoverResultForMarkers(model, position, markers, range) {
      const html = buildEditorHoverHtml(null, markers);
      const primary = markers[0];
      const hoverRange = primary
        ? new monaco.Range(
            primary.startLineNumber,
            primary.startColumn,
            primary.endLineNumber,
            primary.endColumn,
          )
        : range;
      return {
        range: hoverRange,
        contents: [{
          value: html,
          supportHtml: true,
          isTrusted: true,
        }],
      };
    }

    monaco.languages.registerHoverProvider(REAPER_DOC_SELECTOR, {
      provideHover(model, position) {
        const range = wordRangeAt(model, position);
        const markers = markersAtEditorPosition(model, position);

        // Return synchronously on error lines — async symbol lookup triggers
        // Monaco's "Loading..." placeholder and slows the problem hover.
        if (markers.length) {
          return hoverResultForMarkers(model, position, markers, range);
        }

        return lookupSymbolHover(helpers, model, position).then((symInfo) => {
          if (!symInfo?.name) return null;
          const html = buildEditorHoverHtml(symInfo, []);
          if (!html) return null;
          return {
            range,
            contents: [{
              value: html,
              supportHtml: true,
              isTrusted: true,
            }],
          };
        });
      },
    });

    const quickFixCache = new Map();
    const quickFixInflight = new Map();
    const QUICK_FIX_CACHE_MS = 120000;
    const QUICK_FIX_EMPTY_CACHE_MS = 8000;
    const QUICK_FIX_CACHE_MAX = 64;
    const QUICK_FIX_FETCH_TIMEOUT_MS = 20000;

    function quickFixCacheKey(repo, path, contentLen, sig) {
      return `${repo}:${path}:${contentLen}:${sig}`;
    }

    function diagnosticSignature(items) {
      return items.map((d) => `${d.line}:${d.column}:${d.message}`).join('|');
    }

    function markersInRange(model, range) {
      return monaco.editor.getModelMarkers({ resource: model.uri }).filter((m) => {
        if (range.endLineNumber < m.startLineNumber || range.startLineNumber > m.endLineNumber) {
          return false;
        }
        return true;
      });
    }

    function markerFromDiagnostic(model, d, span) {
      return {
        severity: d.severity === 'warning'
          ? monaco.MarkerSeverity.Warning
          : monaco.MarkerSeverity.Error,
        message: d.message,
        startLineNumber: span.startLineNumber,
        startColumn: span.startColumn,
        endLineNumber: span.endLineNumber,
        endColumn: span.endColumn,
      };
    }

    function markersToDiagnosticPayload(markers) {
      return markers.map((m) => ({
        line: m.startLineNumber,
        column: m.startColumn,
        message: m.message,
        severity: m.severity === monaco.MarkerSeverity.Warning ? 'warning' : 'error',
      }));
    }

    function quickFixActionTitle(fix) {
      const title = String(fix?.title || 'Fix');
      if (/^(AI|Cursor):/i.test(title)) return title;
      if (fix?.provider === 'local') return title;
      const src = fix?.provider === 'cursor' ? 'Cursor' : 'AI';
      return `${src}: ${title}`;
    }

    function readEditField(edit, snake, camel) {
      const v = edit?.[snake] ?? edit?.[camel];
      return v == null ? null : Number(v);
    }

    function lineMaxColumn(model, line) {
      return model.getLineContent(line).length + 1;
    }

    function lineDeletionEdit(model, line) {
      const lineCount = model.getLineCount();
      if (line < 1 || line > lineCount) return null;
      if (line < lineCount) {
        return {
          start_line: line,
          start_column: 1,
          end_line: line + 1,
          end_column: 1,
          text: '',
        };
      }
      const lineText = model.getLineContent(line);
      if (lineCount === 1 && !lineText.length) return null;
      return {
        start_line: line,
        start_column: 1,
        end_line: line,
        end_column: lineMaxColumn(model, line),
        text: '',
      };
    }

    function clampQuickFixEdit(model, raw) {
      const lineCount = model.getLineCount();
      let sl = Math.max(1, readEditField(raw, 'start_line', 'startLine') ?? 1);
      let sc = Math.max(1, readEditField(raw, 'start_column', 'startColumn') ?? 1);
      let el = Math.max(1, readEditField(raw, 'end_line', 'endLine') ?? sl);
      let ec = Math.max(1, readEditField(raw, 'end_column', 'endColumn') ?? sc);
      if (sl === 0 || sc === 0) {
        sl = Math.max(1, sl);
        sc = Math.max(1, sc);
      }
      sl = Math.min(sl, lineCount);
      el = Math.min(el, lineCount);
      sc = Math.min(sc, lineMaxColumn(model, sl));
      ec = Math.min(ec, lineMaxColumn(model, el));
      if (el === sl && ec < sc) ec = sc;
      return {
        start_line: sl,
        start_column: sc,
        end_line: el,
        end_column: ec,
        text: raw?.text ?? '',
      };
    }

    function editRangeText(model, edit) {
      return model.getValueInRange(new monaco.Range(
        edit.start_line,
        edit.start_column,
        edit.end_line,
        edit.end_column,
      ));
    }

    function fixWouldChangeModel(model, fix) {
      return (fix?.edits || []).some((e) => {
        const clamped = clampQuickFixEdit(model, e);
        return editRangeText(model, clamped) !== (clamped.text ?? '');
      });
    }

    function isRemovalQuickFix(fix) {
      const title = String(fix?.title || '').toLowerCase();
      const edits = fix?.edits || [];
      const emptyText = edits.every((e) => !String(e?.text ?? '').length);
      return emptyText && (
        /remove|delete|drop|strip|clear|invalid statement|empty statement|not a statement/.test(title)
      );
    }

    function normalizeQuickFixForMarkers(model, fix, markers) {
      if (!fix?.edits?.length) return fix;
      if (fix.provider === 'local') {
        return {
          ...fix,
          edits: fix.edits.map((e) => clampQuickFixEdit(model, e)),
        };
      }
      const title = String(fix.title || '').toLowerCase();
      const marker = markers?.find((m) => m.severity === monaco.MarkerSeverity.Error)
        || markers?.[0];
      if (marker && isRemovalQuickFix(fix)) {
        const del = lineDeletionEdit(model, marker.startLineNumber);
        if (del) {
          return { ...fix, edits: [del] };
        }
        if (marker.endLineNumber > marker.startLineNumber
            || marker.endColumn > marker.startColumn) {
          return {
            ...fix,
            edits: [{
              start_line: marker.startLineNumber,
              start_column: marker.startColumn,
              end_line: marker.endLineNumber,
              end_column: marker.endColumn,
              text: '',
            }],
          };
        }
      }
      let edits = fix.edits.map((e) => clampQuickFixEdit(model, e));
      if (marker && /insert.*;|add.*semicolon|missing semicolon/.test(title)) {
        const line = marker.startLineNumber;
        const lineText = model.getLineContent(line);
        const insertCol = lineText.trimEnd().length + 1;
        edits = [{
          start_line: line,
          start_column: insertCol,
          end_line: line,
          end_column: insertCol,
          text: edits[0]?.text?.includes(';') ? edits[0].text : ';',
        }];
      } else if (marker && /cannot find symbol|systemout|system\.out/.test(
        String(marker.message || '').toLowerCase() + title,
      )) {
        const msg = String(marker.message || '');
        const sym = msg.match(/symbol:\s*(?:class|interface|variable|method)?\s*(\S+)/i)?.[1];
        const line = marker.startLineNumber;
        const lineText = model.getLineContent(line);
        if (sym && lineText.includes(sym)) {
          let replacement = null;
          if (/^systemout$/i.test(sym)) {
            replacement = 'System.out';
          } else if (/^System[A-Z]\w*/.test(sym)) {
            replacement = `System.${sym.slice(6)}`;
          } else {
            replacement = sym.replace(/([a-z])([A-Z])/g, '$1.$2');
          }
          if (replacement && replacement !== sym) {
            const idx = lineText.indexOf(sym);
            edits = [{
              start_line: line,
              start_column: idx + 1,
              end_line: line,
              end_column: idx + sym.length + 1,
              text: replacement,
            }];
          }
        }
      }
      return { ...fix, edits };
    }

    function sanitizeFixList(model, fixes, markers) {
      const out = [];
      const seen = new Set();
      for (const fix of fixes || []) {
        const normalized = normalizeQuickFixForMarkers(model, fix, markers);
        if (!normalized?.edits?.length || !fixWouldChangeModel(model, normalized)) continue;
        const key = `${normalized.provider || 'ai'}:${normalized.title}:${normalized.edits.map(
          (e) => `${e.start_line}:${e.start_column}:${e.end_line}:${e.end_column}:${e.text}`,
        ).join('|')}`;
        if (seen.has(key)) continue;
        seen.add(key);
        out.push(normalized);
      }
      return out;
    }

    function readCachedQuickFixes(key, model, markers) {
      const cached = quickFixCache.get(key);
      if (!cached) return null;
      if (cached.empty) {
        if (Date.now() - cached.time < QUICK_FIX_EMPTY_CACHE_MS) return [];
        quickFixCache.delete(key);
        return null;
      }
      if (Date.now() - cached.time >= QUICK_FIX_CACHE_MS) {
        quickFixCache.delete(key);
        return null;
      }
      return sanitizeFixList(model, cached.fixes || [], markers);
    }

    function storeQuickFixCache(key, fixes) {
      if (fixes.length > 0) {
        quickFixCache.set(key, { fixes, time: Date.now() });
      } else {
        quickFixCache.set(key, { fixes: [], time: Date.now(), empty: true });
      }
      if (quickFixCache.size > QUICK_FIX_CACHE_MAX) {
        const first = quickFixCache.keys().next().value;
        if (first) quickFixCache.delete(first);
      }
    }

    function localQuickFixesForMarkers(model, markers) {
      const fixes = [];
      const seen = new Set();
      for (const m of markers) {
        const msg = String(m.message || '');
        const msgLower = msg.toLowerCase();
        const line = m.startLineNumber;
        const lineText = model.getLineContent(line);
        const trimmedEnd = lineText.trimEnd();
        const insertCol = trimmedEnd.length + 1;

        if (msgLower.includes(';') && msgLower.includes('expected')) {
          const trimmed = trimmedEnd.trim();
          if (trimmed && !trimmed.endsWith(';') && !trimmed.endsWith('{') && !trimmed.endsWith('}')) {
            const key = `semi:${line}:${insertCol}`;
            if (!seen.has(key)) {
              seen.add(key);
              fixes.push({
                title: "Insert ';'",
                provider: 'local',
                edits: [{
                  start_line: line,
                  start_column: insertCol,
                  end_line: line,
                  end_column: insertCol,
                  text: ';',
                }],
              });
            }
          }
        }

        if (msgLower.includes('empty statement') || msgLower.includes('not a statement')
            || msgLower.includes('illegal start of expression')) {
          const trimmed = lineText.trim();
          const rangeKey = `drop-range:${m.startLineNumber}:${m.startColumn}:${m.endLineNumber}:${m.endColumn}`;
          if ((m.endLineNumber > m.startLineNumber || m.endColumn > m.startColumn)
              && !seen.has(rangeKey)) {
            seen.add(rangeKey);
            fixes.push({
              title: 'Remove invalid token',
              provider: 'local',
              edits: [{
                start_line: m.startLineNumber,
                start_column: m.startColumn,
                end_line: m.endLineNumber,
                end_column: m.endColumn,
                text: '',
              }],
            });
          }
          const delEdit = lineDeletionEdit(model, line);
          if (delEdit && (trimmed === ';' || trimmed === '')) {
            const key = `drop:${line}`;
            if (!seen.has(key)) {
              seen.add(key);
              fixes.push({
                title: 'Remove empty statement',
                provider: 'local',
                edits: [delEdit],
              });
            }
          } else if (delEdit && (msgLower.includes('not a statement')
              || msgLower.includes('illegal start'))) {
            const key = `drop-line:${line}`;
            if (!seen.has(key)) {
              seen.add(key);
              fixes.push({
                title: 'Remove invalid statement',
                provider: 'local',
                edits: [delEdit],
              });
            }
          }
        }

        if (msgLower.includes("'}' expected") || msgLower.includes('reached end of file while parsing')) {
          const key = `brace:${line}`;
          if (!seen.has(key)) {
            seen.add(key);
            let braceIndent = (lineText.match(/^(\s*)/) || ['', ''])[1];
            if (!braceIndent) {
              const nextLine = line < model.getLineCount()
                ? model.getLineContent(line + 1)
                : '';
              const nextMatch = nextLine.match(/^(\s*)\}/);
              if (nextMatch) {
                braceIndent = nextMatch[1] || '    ';
              } else {
                for (let scan = line - 1; scan >= 1; scan--) {
                  const prev = model.getLineContent(scan);
                  const block = prev.match(/^(\s*)(?:public |private |protected |static |\w.*\{\s*)$/);
                  if (block) {
                    braceIndent = block[1];
                    break;
                  }
                  if (prev.trim()) {
                    const m = prev.match(/^(\s*)/);
                    braceIndent = m?.[1]?.length >= 4 ? m[1].slice(0, -4) : (m?.[1] || '    ');
                    break;
                  }
                }
              }
            }
            if (!braceIndent) braceIndent = '    ';
            const insertText = lineText.trim()
              ? `\n${braceIndent}}`
              : `${braceIndent}}`;
            const col = lineText.trim() ? Math.max(1, insertCol) : 1;
            fixes.push({
              title: "Insert missing '}'",
              provider: 'local',
              edits: [{
                start_line: line,
                start_column: col,
                end_line: line,
                end_column: col,
                text: insertText,
              }],
            });
          }
        }

        if (msgLower.includes('cannot find symbol')) {
          const symbol = extractJavacClassSymbol(msg);
          const fqcn = symbol && COMMON_JAVA_IMPORTS[symbol];
          if (fqcn && !model.getValue().includes(`import ${fqcn};`)) {
            const key = `import:${fqcn}`;
            if (!seen.has(key)) {
              seen.add(key);
              fixes.push({
                title: `Add import for ${symbol}`,
                provider: 'local',
                edits: [javaImportInsertEdit(model, fqcn)],
              });
            }
          }
        }
      }
      return fixes;
    }

    function buildAiQuickFixCommandAction(markers) {
      return {
        title: 'Quick fix (Cursor / Gemini)…',
        kind: reaperCodeActionKind('QuickFix'),
        diagnostics: markers.map((m) => ({
          severity: m.severity,
          message: m.message,
          startLineNumber: m.startLineNumber,
          startColumn: m.startColumn,
          endLineNumber: m.endLineNumber,
          endColumn: m.endColumn,
        })),
        command: {
          id: 'reaper.aiQuickFix',
          title: 'Quick fix',
        },
      };
    }

    function markersForQuickFix(model, ed, scopeMarkers) {
      const pos = ed?.getPosition?.();
      const all = scopeMarkers?.length ? scopeMarkers : allFileMarkers(model);
      if (!pos) return all;
      const atLine = all.filter(
        (m) => pos.lineNumber >= m.startLineNumber && pos.lineNumber <= m.endLineNumber,
      );
      return atLine.length ? atLine : all;
    }

    function mergeQuickFixLists(...lists) {
      const out = [];
      const seen = new Set();
      for (const list of lists) {
        for (const fix of list || []) {
          if (!fix?.edits?.length) continue;
          const key = `${fix.provider || 'ai'}:${fix.title}:${fix.edits.map(
            (e) => `${e.start_line}:${e.start_column}:${e.end_line}:${e.end_column}:${e.text}`,
          ).join('|')}`;
          if (seen.has(key)) continue;
          seen.add(key);
          out.push(fix);
        }
      }
      return out;
    }

    const mergeQuickFixes = mergeQuickFixLists;

    function collectQuickFixes(model, markers, { includeCachedAi = true } = {}) {
      const local = sanitizeFixList(model, localQuickFixesForMarkers(model, markers), markers);
      if (!includeCachedAi) return local;
      const repo = helpers.getRepo();
      const path = helpers.getActivePath?.() || '';
      const cacheKey = repo && path ? fileQuickFixCacheKey(repo, path, model) : '';
      const cached = cacheKey ? readCachedQuickFixes(cacheKey, model, markers) : null;
      if (cached === null) return local;
      return mergeQuickFixes(local, cached);
    }

    function hasAiQuickFixes(fixes) {
      return fixes.some((f) => f.provider && f.provider !== 'local');
    }

    const QUICK_FIX_LOADING = Object.freeze({
      title: 'Fetching AI fixes…',
      provider: 'loading',
    });

    function presentQuickFixes(ed, fixes, {
      alwaysMenu = false,
      anchorEl = null,
      markers = null,
    } = {}) {
      const model = ed.getModel();
      const scoped = model && markers
        ? sanitizeFixList(model, fixes || [], markers)
        : (fixes || []).filter((f) => f.edits?.length);
      const list = scoped;
      const actionable = list.filter((f) => f.edits?.length);
      const pending = (fixes || []).some((f) => f.provider === 'loading');

      if (!actionable.length && !pending) {
        helpers.toast?.(
          'No quick fixes available — configure Cursor or Gemini in Settings',
          'error',
        );
        return;
      }
      const showMenu = alwaysMenu
        || actionable.length > 1
        || hasAiQuickFixes(actionable)
        || pending;
      if (!showMenu && actionable.length === 1) {
        if (applyQuickFixEdits(ed, actionable[0], markers)) {
          helpers.toast?.(`Applied: ${quickFixActionTitle(actionable[0])}`, 'success');
          helpers.scheduleDiagnostics?.();
        } else {
          helpers.toast?.('Could not apply fix — try again', 'error');
        }
        return;
      }
      const menuItems = pending
        ? [...actionable, ...(fixes || []).filter((f) => f.provider === 'loading')]
        : actionable;
      helpers.showQuickFixMenu?.(menuItems, (fix) => {
        if (applyQuickFixEdits(ed, fix, markers)) {
          helpers.toast?.(`Applied: ${quickFixActionTitle(fix)}`, 'success');
          helpers.scheduleDiagnostics?.();
        } else {
          helpers.toast?.('Could not apply fix', 'error');
        }
      }, anchorEl);
    }

    function runQuickFixFlow(ed, {
      fetchAi = true,
      scopeMarkers = null,
      alwaysMenu = true,
      anchorEl = null,
      line = null,
    } = {}) {
      const model = ed.getModel();
      if (!model) return;
      let markers = scopeMarkers;
      if (line != null) {
        markers = allFileMarkers(model).filter((m) => m.startLineNumber === line);
      }
      markers = markersForQuickFix(model, ed, markers);
      if (!markers.length) {
        helpers.toast?.('No compiler errors on this line', 'info');
        return;
      }
      const fixes = collectQuickFixes(model, markers);
      const needsAiFetch = fetchAi && !hasAiQuickFixes(fixes);

      if (fixes.length) {
        presentQuickFixes(ed, fixes, { alwaysMenu, anchorEl, markers });
      } else if (needsAiFetch) {
        presentQuickFixes(ed, [QUICK_FIX_LOADING], { alwaysMenu, anchorEl, markers });
      } else {
        presentQuickFixes(ed, fixes, { alwaysMenu, anchorEl, markers });
        return;
      }

      if (!needsAiFetch) return;

      const allMarkers = allFileMarkers(model);
      fetchQuickFixes(
        model, allMarkers.length ? allMarkers : markers, { silent: true, scopeMarkers: markers },
      ).then((aiFixes) => {
        const merged = mergeQuickFixes(fixes, aiFixes);
        const menuOpen = helpers.isQuickFixMenuOpen?.();
        const hadInitialFixes = fixes.length > 0;

        if (merged.length && (menuOpen || !hadInitialFixes)) {
          presentQuickFixes(ed, merged, { alwaysMenu, anchorEl, markers });
        } else if (!merged.length && !hadInitialFixes) {
          helpers.hideQuickFixMenu?.();
          helpers.toast?.(
            'No quick fixes available — configure Cursor or Gemini in Settings',
            'error',
          );
        }
      }).catch((err) => {
        if (!fixes.length) {
          helpers.hideQuickFixMenu?.();
          helpers.toast?.(err?.message || 'AI quick fix failed', 'error');
        }
      });
    }

    function syncQuickFixActions(model, markers) {
      const errorMarkers = markers.filter(
        (m) => m.severity === monaco.MarkerSeverity.Error,
      );
      const repo = helpers.getRepo();
      const path = helpers.getActivePath?.() || '';
      const cacheKey = repo && path ? fileQuickFixCacheKey(repo, path, model) : '';
      const cached = cacheKey ? quickFixCache.get(cacheKey) : null;
      const cacheFresh = cached && (
        (cached.empty && Date.now() - cached.time < QUICK_FIX_EMPTY_CACHE_MS)
        || (cached.fixes?.length && Date.now() - cached.time < QUICK_FIX_CACHE_MS)
      );

      if (!cacheFresh) {
        const allMarkers = allFileMarkers(model);
        fetchQuickFixes(
          model, allMarkers.length ? allMarkers : markers,
          { silent: true, scopeMarkers: markers },
        );
      }

      const fixes = collectQuickFixes(model, markers);
      const actions = fixes.length
        ? buildQuickFixActions(model, markers, fixes)
        : [];
      if (errorMarkers.length && !hasAiQuickFixes(fixes)) {
        actions.push(buildAiQuickFixCommandAction(markers));
      }
      return actions;
    }

    function codeActionFromFix(model, fix, linkedDiagnostics) {
      const edits = (fix.edits || []).map((e) => ({
        resource: model.uri,
        textEdit: {
          range: new monaco.Range(
            e.start_line,
            e.start_column,
            e.end_line,
            e.end_column,
          ),
          text: e.text ?? '',
        },
      }));
      if (!edits.length) return null;
      return {
        title: quickFixActionTitle(fix),
        kind: reaperCodeActionKind('QuickFix'),
        isPreferred: true,
        diagnostics: linkedDiagnostics,
        edit: { edits },
      };
    }

    function buildQuickFixActions(model, markers, fixes) {
      const linked = markers.map((m) => ({
        severity: m.severity,
        message: m.message,
        startLineNumber: m.startLineNumber,
        startColumn: m.startColumn,
        endLineNumber: m.endLineNumber,
        endColumn: m.endColumn,
      }));
      const actions = [];
      for (const fix of fixes) {
        const action = codeActionFromFix(model, fix, linked);
        if (action) actions.push(action);
      }
      return actions;
    }

    function allFileMarkers(model) {
      return monaco.editor.getModelMarkers({ resource: model.uri });
    }

    function fileQuickFixCacheKey(repo, path, model) {
      const markers = allFileMarkers(model);
      const payload = markersToDiagnosticPayload(markers);
      return quickFixCacheKey(
        repo, path, model.getValue().length, diagnosticSignature(payload),
      );
    }

    function applyQuickFixEdits(ed, fix, markers) {
      const model = ed.getModel();
      if (!model) return false;
      const normalized = normalizeQuickFixForMarkers(model, fix, markers);
      if (!fixWouldChangeModel(model, normalized)) return false;
      const editBatch = (normalized.edits || [])
        .map((e) => ({
          range: new monaco.Range(
            e.start_line,
            e.start_column,
            e.end_line,
            e.end_column,
          ),
          text: e.text ?? '',
        }))
        .sort((a, b) => {
          if (a.range.startLineNumber !== b.range.startLineNumber) {
            return b.range.startLineNumber - a.range.startLineNumber;
          }
          return b.range.startColumn - a.range.startColumn;
        });
      if (!editBatch.length) return false;
      ed.pushUndoStop?.();
      return ed.executeEdits('reaper-quick-fix', editBatch);
    }

    async function fetchQuickFixes(model, markers, {
      silent = false,
      scopeMarkers = null,
    } = {}) {
      const repo = helpers.getRepo();
      const path = helpers.getActivePath?.() || '';
      if (!repo || !path || !helpers.repoApi || !markers.length) return [];
      const content = model.getValue();
      const payload = markersToDiagnosticPayload(markers);
      const key = quickFixCacheKey(repo, path, content.length, diagnosticSignature(payload));
      const anchorMarkers = scopeMarkers || markers;
      const cached = readCachedQuickFixes(key, model, anchorMarkers);
      if (cached !== null) return cached;
      const inflight = quickFixInflight.get(key);
      if (inflight) return inflight;

      const promise = (async () => {
        const controller = new AbortController();
        const timer = setTimeout(() => controller.abort(), QUICK_FIX_FETCH_TIMEOUT_MS);
        try {
          const fixes = await helpers.api(helpers.repoApi(repo, '/workspace/quick-fixes'), {
            method: 'POST',
            body: JSON.stringify({ path, content, diagnostics: payload }),
            signal: controller.signal,
          });
          const list = sanitizeFixList(
            model,
            Array.isArray(fixes) ? fixes : [],
            anchorMarkers,
          );
          storeQuickFixCache(key, list);
          return list;
        } catch (err) {
          if (!silent) {
            const msg = err?.name === 'AbortError'
              ? 'AI quick fix timed out — try again or add Gemini in Settings'
              : (err?.message || 'AI quick fix failed');
            helpers.toast?.(msg, 'error');
          }
          return [];
        } finally {
          clearTimeout(timer);
          quickFixInflight.delete(key);
        }
      })();

      quickFixInflight.set(key, promise);
      return promise;
    }

    function prefetchQuickFixes(model, diags) {
      if (!model || !diags?.length || !helpers.repoApi || !helpers.getRepo()) return;
      const repo = helpers.getRepo();
      const path = helpers.getActivePath?.() || '';
      if (!repo || !path) return;
      const key = fileQuickFixCacheKey(repo, path, model);
      const cached = quickFixCache.get(key);
      const cacheFresh = cached && (
        (cached.empty && Date.now() - cached.time < QUICK_FIX_EMPTY_CACHE_MS)
        || (cached.fixes?.length && Date.now() - cached.time < QUICK_FIX_CACHE_MS)
      );
      if (cacheFresh || quickFixInflight.has(key)) return;
      const markers = allFileMarkers(model);
      const markerInput = markers.length
        ? markers
        : diags.map((d) => {
          const span = helpers.diagnosticSpan?.(model, d);
          return {
            startLineNumber: span?.startLineNumber ?? d.line,
            startColumn: (span?.startColumn ?? d.column) || 1,
            endLineNumber: span?.endLineNumber ?? d.line,
            endColumn: (span?.endColumn ?? (d.column || 1) + 1),
            message: d.message,
            severity: d.severity === 'warning'
              ? monaco.MarkerSeverity.Warning
              : monaco.MarkerSeverity.Error,
          };
        });
      fetchQuickFixes(model, markerInput, { silent: true }).then((fixes) => {
        if (fixes.length) editor._reaperQuickFixReady = Date.now();
      });
    }

    editor.prefetchAiQuickFixes = (diags) => {
      const model = editor.getModel();
      if (model) prefetchQuickFixes(model, diags);
    };

    const QUICK_FIX_BULB_SVG = '<svg class="ij-ai-bulb-icon" viewBox="0 0 24 24" aria-hidden="true">'
      + '<g class="ij-ai-sparkles">'
      + '<path class="ij-ai-sparkle ij-ai-sparkle-a" fill="currentColor" d="M4.5 7.5l1-2 1 2-2 1 2 1-1 2-1-2-2-1z"/>'
      + '<path class="ij-ai-sparkle ij-ai-sparkle-b" fill="currentColor" d="M19.5 4l.75-1.5.75 1.5-1.5.75 1.5.75-.75 1.5-.75-1.5-1.5-.75z"/>'
      + '</g>'
      + '<path class="ij-ai-bulb-glass" fill="currentColor" fill-opacity="0.28" stroke="currentColor" stroke-width="2" stroke-linecap="round" d="M9 18h6M10 22h4M12 2a7 7 0 017 7c0 2.5-1.3 4.7-3.3 6H8.3A7 7 0 0112 2z"/>'
      + '</svg>';

    function clearQuickFixBulbs(ed) {
      if (!ed?._reaperQuickFixBulbs?.length) {
        ed._reaperQuickFixBulbs = [];
        return;
      }
      for (const widget of ed._reaperQuickFixBulbs) {
        try {
          ed.removeGlyphMarginWidget(widget);
        } catch {
          /* stale widget */
        }
      }
      ed._reaperQuickFixBulbs = [];
    }

    function createQuickFixBulbWidget(line) {
      const domNode = document.createElement('button');
      domNode.type = 'button';
      domNode.className = 'ij-quickfix-glyph-bulb ij-ai-bulb-btn is-glowing';
      domNode.title = 'Quick fix — show all fixes (⌘.)';
      domNode.setAttribute('aria-label', 'Quick fix');
      domNode.innerHTML = QUICK_FIX_BULB_SVG;
      domNode.addEventListener('mousedown', (e) => {
        e.preventDefault();
        e.stopPropagation();
      });
      domNode.addEventListener('click', (e) => {
        e.preventDefault();
        e.stopPropagation();
        editor.runAiQuickFix?.({ line, anchorEl: domNode });
      });
      const lane = monaco.editor.GlyphMarginLane?.Left ?? 1;
      return {
        getId: () => `reaper-quickfix-bulb-${line}`,
        getDomNode: () => domNode,
        getPosition: () => ({
          range: new monaco.Range(line, 1, line, 1),
          lane,
        }),
      };
    }

    function refreshQuickFixBulbs(diags) {
      clearQuickFixBulbs(editor);
      if (!diags?.length || typeof editor.addGlyphMarginWidget !== 'function') return;
      const model = editor.getModel();
      if (!model) return;
      const lines = new Set();
      for (const d of diags) {
        if (d.severity === 'warning') continue;
        const span = helpers.diagnosticSpan?.(model, d);
        const line = span?.startLineNumber ?? d.line ?? 1;
        if (line > 0) lines.add(line);
      }
      editor._reaperQuickFixBulbs = [];
      for (const line of [...lines].sort((a, b) => a - b)) {
        const widget = createQuickFixBulbWidget(line);
        editor.addGlyphMarginWidget(widget);
        editor._reaperQuickFixBulbs.push(widget);
      }
    }

    editor.refreshQuickFixBulbs = refreshQuickFixBulbs;
    editor.clearQuickFixBulbs = () => clearQuickFixBulbs(editor);

    editor.runAiQuickFix = async (opts) => {
      const line = typeof opts === 'number' ? opts : opts?.line;
      const anchorEl = opts?.anchorEl || null;
      await runQuickFixFlow(editor, {
        fetchAi: true,
        line: line ?? null,
        alwaysMenu: true,
        anchorEl,
      });
    };

    editor.runQuickFix = editor.runAiQuickFix;

    try {
    monaco.languages.registerCodeActionProvider(
      [{ pattern: '**' }],
      {
        provideCodeActions(model, range, context) {
          if (!codeActionOnlyWantsQuickFix(context.only)) {
            return { actions: [], dispose: () => {} };
          }
          const markers = markersInRange(model, range);
          if (!markers.length) {
            return { actions: [], dispose: () => {} };
          }
          return {
            actions: syncQuickFixActions(model, markers),
            dispose: () => {},
          };
        },
      },
      {
        providedCodeActionKinds: [reaperCodeActionKind('QuickFix')],
      },
    );
    } catch (err) {
      console.warn('[Reaper] code action provider skipped:', err);
    }

    let aiInlineTimer = null;
    let aiInlineCache = {
      key: '', text: '', controlSnippet: '',
      repo: '', path: '', lineNumber: 0, column: 0, linePrefix: '',
    };
    const AI_INLINE_DEBOUNCE_MS = 200;
    const AI_INLINE_STATEMENT_DEBOUNCE_MS = 70;
    let aiInlineSeq = 0;
    const aiCompleteCache = { key: '', items: [], time: 0 };
    let inlineShowTimer = null;

    function queueInlineSuggestion(ed) {
      clearTimeout(inlineShowTimer);
      inlineShowTimer = setTimeout(() => {
        requestAnimationFrame(() => {
          ed.trigger('reaper', 'editor.action.inlineSuggest.trigger', {});
          setTimeout(() => {
            ed.trigger('reaper', 'editor.action.inlineSuggest.trigger', {});
          }, 48);
        });
      }, 16);
    }

    function aiInlineFetchEnabled() {
      return helpers.getAiInlineComplete?.() && helpers.getGeminiConfigured?.();
    }

    function aiInlineEnabled() {
      return helpers.getAiInlineComplete?.() && helpers.repoApi && helpers.getRepo?.();
    }

    function inlineCacheKey(model, position, linePrefix) {
      const repo = helpers.getRepo();
      const path = helpers.getActivePath?.() || '';
      return buildInlineCacheKey(repo, path, position.lineNumber, position.column, linePrefix);
    }

    function inlineCacheMeta(repo, path, position, linePrefix) {
      return {
        repo,
        path,
        lineNumber: position.lineNumber,
        column: position.column,
        linePrefix,
      };
    }

    function dismissInlineGhost(ed) {
      clearInlineCache(ed);
      if (!ed) return;
      ed.trigger('reaper', 'editor.action.inlineSuggest.hide', {});
    }

    function clearInlineCache(ed) {
      aiInlineCache = {
        key: '', text: '', controlSnippet: '',
        repo: '', path: '', lineNumber: 0, column: 0, linePrefix: '',
      };
      if (ed) ed._reaperPendingControlSnippet = null;
    }

    function setInlineCache(ed, cacheKey, text, controlSnippet, meta) {
      const path = meta?.path ?? (helpers.getActivePath?.() || '');
      const linePrefix = meta?.linePrefix ?? '';
      const clean = capInlineText(sanitizeInlineGhostText(text));
      const snippet = controlSnippet || '';
      if (!clean || shouldSuppressInlineGhost(path, linePrefix, clean)) {
        if (aiInlineCache.text) {
          clearInlineCache(ed);
        }
        return;
      }
      if (
        aiInlineCache.key === cacheKey
        && aiInlineCache.text === clean
        && aiInlineCache.controlSnippet === snippet
      ) {
        return;
      }
      aiInlineCache = {
        key: cacheKey,
        text: clean,
        controlSnippet: snippet,
        repo: meta?.repo ?? aiInlineCache.repo,
        path: meta?.path ?? aiInlineCache.path,
        lineNumber: meta?.lineNumber ?? aiInlineCache.lineNumber,
        column: meta?.column ?? aiInlineCache.column,
        linePrefix: meta?.linePrefix ?? aiInlineCache.linePrefix,
      };
      if (snippet) {
        ed._reaperPendingControlSnippet = { key: cacheKey, snippet };
      } else {
        ed._reaperPendingControlSnippet = null;
      }
      queueInlineSuggestion(ed);
    }

    function acceptInlineOrControlSnippet(ed) {
      const model = ed.getModel();
      const position = ed.getPosition();
      if (!model || !position) return;

      const linePrefix = editorLinePrefix(model, position);
      const cacheKey = inlineCacheKey(model, position, linePrefix);
      const pending = ed._reaperPendingControlSnippet;
      const insertRange = new monaco.Range(
        position.lineNumber, position.column, position.lineNumber, position.column,
      );

      const clearInline = () => {
        clearInlineCache(ed);
      };

      if (pending?.snippet && pending.key === cacheKey) {
        ed.trigger('reaper', 'editor.action.insertSnippet', { snippet: pending.snippet });
        clearInline();
        return;
      }

      if (aiInlineCache.key === cacheKey && aiInlineCache.controlSnippet) {
        ed.trigger('reaper', 'editor.action.insertSnippet', { snippet: aiInlineCache.controlSnippet });
        clearInline();
        return;
      }

      if (aiInlineCache.key === cacheKey && aiInlineCache.text) {
        ed.executeEdits('reaper', [{ range: insertRange, text: aiInlineCache.text }]);
        clearInline();
        return;
      }

      ed.trigger('reaper', 'editor.action.inlineSuggest.commit', null);
    }

    function hasActiveInlineSuggestion(ed) {
      const model = ed.getModel();
      const position = ed.getPosition();
      if (model && position) {
        const cacheKey = inlineCacheKey(model, position, editorLinePrefix(model, position));
        if (ed._reaperPendingControlSnippet?.snippet && ed._reaperPendingControlSnippet.key === cacheKey) {
          return true;
        }
        if (aiInlineCache.key === cacheKey && (aiInlineCache.text || aiInlineCache.controlSnippet)) {
          return true;
        }
      }
      const ctx = ed._contextKeyService;
      return ctx?.getContextKeyValue('inlineSuggestionVisible') ?? false;
    }

    function handleEditorTab(ed) {
      if (memberFallbackEl) {
        hideMemberSuggestFallback();
        ed.trigger('reaper', 'tab', null);
        return;
      }
      const ctx = ed._contextKeyService;
      if (ctx?.getContextKeyValue('suggestWidgetVisible')) {
        ed.trigger('reaper', 'acceptSelectedSuggestion', null);
        return;
      }
      if (ctx?.getContextKeyValue('inSnippetMode')) {
        ed.trigger('reaper', 'jumpToNextSnippetPlaceholder', null);
        return;
      }
      if (hasActiveInlineSuggestion(ed)) {
        acceptInlineOrControlSnippet(ed);
        return;
      }
      ed.trigger('reaper', 'tab', null);
    }

    function shouldAutoOpenSuggest(model, position, path, content) {
      const linePrefix = editorLinePrefix(model, position);
      if (
        path && content && shouldPreferAiStatementInline(
          path, linePrefix, content, position.lineNumber,
        )
      ) {
        return false;
      }
      const word = model.getWordUntilPosition(position);
      const p = word.word || extractInlinePartialToken(linePrefix) || '';
      if (isControlKeywordPrefix(p)) return true;
      if (dotQualifierFromLinePrefix(linePrefix)) return true;
      if (AUTOCOMPLETE_TRIGGER_RE.test(linePrefix)) return true;
      if (p.length >= 2) return true;
      if (p.length === 1 && isKeywordPrefixTyping(path || helpers.getActivePath?.() || '', p)) return true;
      return false;
    }

    // Local + index completions — member access returns instantly (no loading spinner).
    monaco.languages.registerCompletionItemProvider(REAPER_DOC_SELECTOR, {
      triggerCharacters: ['.', '@', ':', '-', '<', '"', "'", '/', '#', '*', '=', '(', '[', '{'],
      provideCompletionItems(model, position, context) {
        const path = helpers.getActivePath?.() || '';
        if (!path) {
          completionDebug(helpers, ['provider', completionTriggerLabel(context), 'skip: no active path']);
          return { suggestions: [], incomplete: false };
        }

        const linePrefixRaw = editorLinePrefix(model, position);
        const trig = monaco.languages.CompletionTriggerKind;
        // Monaco fires on '.' before the dot is in the model — filters against "System".
        if (
          context.triggerKind === trig.TriggerCharacter
          && context.triggerCharacter === '.'
          && !linePrefixRaw.trimEnd().endsWith('.')
        ) {
          return { suggestions: [], incomplete: false };
        }

        const { linePrefix, prefix, range, memberCtx } = completionContext(model, position);
        const manual = context.triggerKind === monaco.languages.CompletionTriggerKind.Invoke;
        const memberContext = memberCtx || dotQualifierFromLinePrefix(linePrefix);
        const completionPrefix = memberContext
          ? (memberContext.memberPrefix || prefix || '')
          : (prefix || '');
        const springConfig = isSpringConfigFile(path);
        if (!memberContext && !shouldFetchIndexCompletions(linePrefix, completionPrefix, path) && !manual) {
          completionDebug(helpers, [
            'provider', completionTriggerLabel(context), 'skip: fetch gate',
            `line=…${linePrefix.slice(-24)}`,
          ]);
          return { suggestions: [], incomplete: false };
        }

        const ed = activeEditor();
        const content = editorContent(ed, model);
        const local = buildLocalCompletionSuggestions(model, position, path, helpers, content);
        const suggestions = [...local.suggestions];
        const seen = new Set(suggestions.map((s) => s.label));

        const report = (tag, extra = '') => {
          const labels = suggestions.slice(0, 6).map((s) => s.label).join(', ');
          completionDebug(helpers, [
            'provider', completionTriggerLabel(context), tag,
            memberContext ? `member=${memberContext.qualifier}` : '',
            `n=${suggestions.length}`,
            labels ? `items=${labels}` : 'items=(none)',
            path.split('/').pop(),
            extra,
          ], { warn: !!(memberContext && suggestions.length === 0) });
        };

        if (!helpers.repoApi || !helpers.getRepo?.()) {
          if (suggestions.length > 0 && ed) {
            presentCompletionSuggestions(ed, suggestions, { content, path });
          }
          report(memberContext ? 'member no repo' : 'no repo');
          return { suggestions: [], incomplete: false };
        }

        const cached = readCachedIndexCompletions(helpers, model, position, completionPrefix);
        if (cached) {
          mergeIndexItemsIntoSuggestions(cached, range, seen, memberContext, suggestions);
        }

        if (memberContext) {
          const ed = activeEditor();
          if (!cached) {
            void fetchCompletionsWithTimeout(helpers, model, position, completionPrefix)
              .then((items) => {
                const n = Array.isArray(items) ? items.length : 0;
                completionDebug(helpers, [
                  'index fetch done',
                  `member=${memberContext.qualifier}`,
                  `n=${n}`,
                ], { warn: n === 0 });
                if (n > 0 && ed) {
                  const fromIndex = [];
                  const idxSeen = new Set(suggestions.map((s) => s.label));
                  mergeIndexItemsIntoSuggestions(items, range, idxSeen, memberContext, fromIndex);
                  if (fromIndex.length > 0) {
                    const merged = suggestions.length > 0
                      ? [...suggestions, ...fromIndex.filter((i) => !suggestions.some((s) => s.label === i.label))]
                      : fromIndex;
                    presentCompletionSuggestions(ed, merged, { content, path });
                  }
                }
              })
              .catch((err) => {
                completionDebug(helpers, [
                  'index fetch failed',
                  memberContext.qualifier,
                  String(err?.message || err),
                ], { warn: true });
              });
          }
          if (suggestions.length > 0 && ed) {
            presentCompletionSuggestions(ed, suggestions, { content, path });
          }
          report(cached ? 'member+cache' : 'member');
          return { suggestions: [], incomplete: false };
        }

        if (manual && !springConfig) {
          report('manual');
          return { suggestions: [], incomplete: false };
        }

        if (springConfig && manual && suggestions.length > 0 && ed) {
          presentCompletionSuggestions(ed, suggestions, { content, path });
        }

        if (suggestions.length > 0 && ed) {
          presentCompletionSuggestions(ed, suggestions, { content, path });
        }

        return (async () => {
          try {
            const budget = suggestions.length > 0
              ? INDEX_COMPLETION_BUDGET_MS
              : COMPLETION_FETCH_TIMEOUT_MS;
            const items = await Promise.race([
              fetchCompletionsWithTimeout(
                helpers, model, position, completionPrefix, COMPLETION_FETCH_TIMEOUT_MS,
              ),
              new Promise((resolve) => setTimeout(() => resolve(null), budget)),
            ]);
            if (items) {
              mergeIndexItemsIntoSuggestions(items, range, seen, memberContext, suggestions);
            }
            if (suggestions.length > 0 && ed) {
              presentCompletionSuggestions(ed, suggestions, { content, path });
            }
            report('async index');
          } catch {
            /* index completions are best-effort */
          }
          return { suggestions: [], incomplete: false };
        })();
      },
    });

    // AI completions — Ctrl+Space only (Gemini is too slow for every keystroke).
    monaco.languages.registerCompletionItemProvider(REAPER_DOC_SELECTOR, {
      async provideCompletionItems(model, position, context) {
        if (context.triggerKind !== monaco.languages.CompletionTriggerKind.Invoke) {
          return { suggestions: [], incomplete: false };
        }
        if (!helpers.getAiInlineComplete?.() || !helpers.getGeminiConfigured?.()) {
          return { suggestions: [], incomplete: false };
        }
        const path = helpers.getActivePath?.() || '';
        if (!path) return { suggestions: [] };

        const { linePrefix, prefix, range } = completionContext(model, position);
        const seen = new Set();
        try {
          const aiItems = await fetchAiCompletions(
            helpers, model, position, linePrefix, prefix, aiCompleteCache,
          );
          const suggestions = [];
          for (const item of aiItems) {
            const label = item.label;
            if (!label || seen.has(label)) continue;
            if (!isCodeLikeCompletion(label, item.kind)) continue;
            seen.add(label);
            const kind = String(item.kind || '').toLowerCase();
            let insertText = item.insert;
            if (!insertText) insertText = kind === 'method' ? `${label}()` : label;
            const suggestion = {
              label,
              kind: completionKind(item.kind),
              detail: item.detail ? `AI · ${item.detail}` : 'AI ·',
              insertText,
              range,
              sortText: `0_${label}`,
            };
            if (
              kind === 'snippet'
              || insertText.includes('\n')
              || insertText.includes('$0')
              || insertText.includes('$1')
            ) {
              suggestion.insertTextRules =
                monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet;
            }
            suggestions.push(suggestion);
            if (suggestions.length >= 12) break;
          }
          return { suggestions, incomplete: false };
        } catch {
          return { suggestions: [], incomplete: false };
        }
      },
    });
    completionDebug(helpers, ['setup', 'completion providers OK', `rev=${REAPER_COMPLETION_REV}`]);

    function scheduleEmptyLineInline(ed) {
      if (!helpers.getAiInlineComplete?.()) return;
      const model = ed.getModel();
      const position = ed.getPosition();
      const repo = helpers.getRepo();
      const path = helpers.getActivePath?.() || '';
      if (!model || !position || !repo || !path) return;
      const linePrefix = editorLinePrefix(model, position);
      if (!isWhitespaceOnlyLine(linePrefix)) return;
      const javaLevel = inlineJavaLevel(helpers);
      const content = editorContent(ed, model);
      const cacheKey = buildInlineCacheKey(repo, path, position.lineNumber, position.column, linePrefix);
      const meta = inlineCacheMeta(repo, path, position, linePrefix);
      const local = localInlineSuggestion(path, linePrefix, content, position.lineNumber, javaLevel);
      if (local) {
        setInlineCache(ed, cacheKey, local, '', meta);
        return;
      }
      scheduleAiInlineFetch();
    }

    function insertInlineContinuationAtCursor(ed) {
      const model = ed.getModel();
      const pos = ed.getPosition();
      if (!model || !pos) return false;
      const linePrefix = editorLinePrefix(model, pos);
      if (!isWhitespaceOnlyLine(linePrefix)) return false;
      const path = helpers.getActivePath?.() || '';
      if (!path) return false;
      const javaLevel = inlineJavaLevel(helpers);
      const content = editorContent(ed, model);
      const suffix = localInlineSuggestion(path, linePrefix, content, pos.lineNumber, javaLevel);
      if (!suffix) return false;
      ed.executeEdits('reaper', [{
        range: new monaco.Range(pos.lineNumber, pos.column, pos.lineNumber, pos.column),
        text: suffix,
      }]);
      const cacheKey = buildInlineCacheKey(helpers.getRepo(), path, pos.lineNumber, pos.column, linePrefix);
      clearInlineCache(ed);
      requestAnimationFrame(() => scheduleEmptyLineInline(ed));
      return true;
    }

    async function fetchInlineComplete(model, position, linePrefix) {
      const repo = helpers.getRepo();
      const path = helpers.getActivePath?.() || '';
      if (!repo || !path) return '';
      const payload = contentForApiPayload(editorContent(editor, model), position.lineNumber);
      const res = await helpers.api(helpers.repoApi(repo, '/workspace/inline-complete'), {
        method: 'POST',
        body: JSON.stringify({
          path,
          line: payload.line,
          column: position.column,
          content: payload.content,
          line_prefix: linePrefix,
        }),
      });
      return res?.text ?? '';
    }

    function scheduleAiInlineFetch() {
      if (!aiInlineFetchEnabled()) return;
      const model = editor.getModel();
      const position = editor.getPosition();
      if (model && position) {
        const linePrefix = editorLinePrefix(model, position);
        if (dotQualifierFromLinePrefix(linePrefix)) {
          clearTimeout(aiInlineTimer);
          return;
        }
      }
      clearTimeout(aiInlineTimer);
      const seq = ++aiInlineSeq;
      let debounceMs = AI_INLINE_DEBOUNCE_MS;
      if (model && position) {
        const path = helpers.getActivePath?.() || '';
        const linePrefix = editorLinePrefix(model, position);
        const content = editorContent(editor, model);
        if (
          path && shouldPreferAiStatementInline(
            path, linePrefix, content, position.lineNumber,
          )
        ) {
          debounceMs = AI_INLINE_STATEMENT_DEBOUNCE_MS;
        }
      }
      aiInlineTimer = setTimeout(async () => {
        if (seq !== aiInlineSeq) return;
        const model = editor.getModel();
        const position = editor.getPosition();
        if (!model || !position) return;
        const repo = helpers.getRepo();
        const path = helpers.getActivePath?.() || '';
        if (!repo || !path) return;
        const linePrefix = editorLinePrefix(model, position);
        if (dotQualifierFromLinePrefix(linePrefix)) return;
        if (!inlineTypingReady()) return;
        if (isJavaDeclarationTyping(path, linePrefix)) return;
        const cacheKey = buildInlineCacheKey(
          repo, path, position.lineNumber, position.column, linePrefix,
        );
        const meta = inlineCacheMeta(repo, path, position, linePrefix);
        if (aiInlineCache.key === cacheKey && aiInlineCache.text) return;
        const javaLevel = inlineJavaLevel(helpers);
        const content = editorContent(editor, model);
        const preferAi = shouldPreferAiStatementInline(
          path, linePrefix, content, position.lineNumber,
        );
        const local = localInlineSuggestion(path, linePrefix, content, position.lineNumber, javaLevel);
        if (local && local.length >= 4 && !preferAi) return;
        try {
          const text = await fetchInlineComplete(model, position, linePrefix);
          if (!text || seq !== aiInlineSeq) return;
          setInlineCache(editor, cacheKey, text, '', meta);
        } catch {
          // Inline completion is best-effort; network errors (e.g. "Load failed") are not user-actionable.
        }
      }, debounceMs);
    }

    monaco.languages.registerInlineCompletionsProvider(ALL_EDITOR_LANGS, {
      provideInlineCompletions: async (model, position, _context, token) => {
        const repo = helpers.getRepo();
        const path = helpers.getActivePath?.() || '';
        if (!repo || !path) return { items: [] };

        const linePrefix = editorLinePrefix(model, position);
        if (!inlineTypingReady()) return { items: [] };
        if (isJavaDeclarationTyping(path, linePrefix)) {
          clearInlineCache(editor);
          return { items: [] };
        }

        const cacheKey = buildInlineCacheKey(
          repo, path, position.lineNumber, position.column, linePrefix,
        );
        const javaLevel = inlineJavaLevel(helpers);
        const content = editorContent(editor, model);
        const memberContext = dotQualifierFromLinePrefix(linePrefix);
        const preferAi = !memberContext && shouldPreferAiStatementInline(
          path, linePrefix, content, position.lineNumber,
        );

        const cachedGhost = inlineGhostFromCache(
          aiInlineCache, repo, path, position.lineNumber, position.column, linePrefix,
        );
        if (cachedGhost) {
          editor._reaperPendingControlSnippet = null;
          if (token.isCancellationRequested) return { items: [] };
          return buildInlineItems(model, position, linePrefix, cachedGhost);
        }

        if (!preferAi) {
          const local = inlineGhostSuffix(
            path, linePrefix, content, position.lineNumber, javaLevel, position.column,
          );
          if (local && !shouldSuppressInlineGhost(path, linePrefix, local)) {
            const meta = inlineCacheMeta(repo, path, position, linePrefix);
            setInlineCache(editor, cacheKey, local, '', meta);
            editor._reaperPendingControlSnippet = null;
            if (token.isCancellationRequested) return { items: [] };
            return buildInlineItems(model, position, linePrefix, local);
          }
        }

        if (isSpringConfigFile(path)) {
          const keyPrefix = springConfigKeyPrefix(
            path, linePrefix, content, position.lineNumber, position.column,
          );
          if (keyPrefix.length >= 1) {
            try {
              const items = await fetchCompletionsWithTimeout(
                helpers, model, position, keyPrefix, 450,
              );
              const suffix = items?.[0]?.label
                ? completionSuffixFromLabel(items[0].label, keyPrefix)
                : '';
              if (suffix && !token.isCancellationRequested) {
                const meta = inlineCacheMeta(repo, path, position, linePrefix);
                setInlineCache(editor, cacheKey, suffix, '', meta);
                return buildInlineItems(model, position, linePrefix, suffix);
              }
            } catch {
              // fall through
            }
          }
        }

        if (!aiInlineFetchEnabled()) {
          return { items: [] };
        }

        scheduleAiInlineFetch();
        return { items: [] };
      },
      freeInlineCompletions: () => {},
    });

    editor.onDidChangeModelContent(() => {
      const model = editor.getModel();
      const position = editor.getPosition();
      if (model) editor._reaperContent = model.getValue();

      const repo = helpers.getRepo();
      const path = helpers.getActivePath?.() || '';
      let preferAi = false;
      let linePrefix = '';
      if (model && position) {
        linePrefix = editorLinePrefix(model, position);
      }
      if (model && position && repo && path) {
        const cacheKey = buildInlineCacheKey(
          repo, path, position.lineNumber, position.column, linePrefix,
        );
        const meta = inlineCacheMeta(repo, path, position, linePrefix);
        const javaLevel = inlineJavaLevel(helpers);
        const content = editor._reaperContent;
        preferAi = shouldPreferAiStatementInline(
          path, linePrefix, content, position.lineNumber,
        );
        if (isJavaDeclarationTyping(path, linePrefix)) {
          clearInlineCache(editor);
        } else {
          const local = preferAi
            ? ''
            : inlineGhostSuffix(
              path, linePrefix, content, position.lineNumber, javaLevel, position.column,
            );
          if (local && !shouldSuppressInlineGhost(path, linePrefix, local)) {
            setInlineCache(editor, cacheKey, local, '', meta);
            if (aiInlineFetchEnabled() && local.length >= 4) {
              scheduleAiInlineFetch();
            }
          } else {
            const stale = inlineGhostFromCache(
              aiInlineCache, repo, path, position.lineNumber, position.column, linePrefix,
            );
            if (stale) {
              aiInlineCache = {
                key: cacheKey,
                text: stale,
                controlSnippet: '',
                repo,
                path,
                lineNumber: position.lineNumber,
                column: position.column,
                linePrefix,
              };
              queueInlineSuggestion(editor);
              if (aiInlineFetchEnabled()) scheduleAiInlineFetch();
            } else {
              if (aiInlineCache.key && aiInlineCache.key !== cacheKey) {
                clearInlineCache(editor);
              }
              if (aiInlineFetchEnabled()) scheduleAiInlineFetch();
            }
          }
        }
      } else {
        clearInlineCache(editor);
      }

      if (dotQualifierFromLinePrefix(linePrefix)) {
        const memberGhost = inlineGhostSuffix(
          path, linePrefix, editor._reaperContent, position.lineNumber, inlineJavaLevel(helpers), position.column,
        );
        if (memberGhost) {
          const cacheKey = buildInlineCacheKey(
            repo, path, position.lineNumber, position.column, linePrefix,
          );
          setInlineCache(editor, cacheKey, memberGhost, '', inlineCacheMeta(repo, path, position, linePrefix));
        }
        scheduleMemberCompletions(editor);
      } else if (isSpringConfigFile(path) && !isJavaDeclarationTyping(path, linePrefix)) {
        scheduleSpringConfigInline(editor);
        scheduleAutocompleteSuggest(editor);
      } else if (!preferAi && !isJavaDeclarationTyping(path, linePrefix)) {
        scheduleAutocompleteSuggest(editor);
      }
    });

    editor.onKeyDown((e) => {
      const ev = e.browserEvent;
      if (ev?.key === ' ' && ev.ctrlKey && !ev.metaKey && !ev.altKey) {
        ev.preventDefault();
        ev.stopPropagation();
        setTimeout(() => fireCompletionsSuggest(activeEditor(), { force: true }), 0);
      }
    });

    editor.onDidChangeCursorPosition((e) => {
      const model = editor.getModel();
      if (!model) return;
      const linePrefix = editorLinePrefix(model, e.position);
      if (!isWhitespaceOnlyLine(linePrefix)) return;
      clearTimeout(editor._reaperEmptyLineTimer);
      editor._reaperEmptyLineTimer = setTimeout(() => scheduleEmptyLineInline(editor), 120);
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
      id: 'reaper.aiQuickFix',
      label: 'Quick fix',
      keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.Period],
      run: (ed) => ed.runAiQuickFix?.(),
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
      id: 'reaper.triggerSuggest',
      label: 'Trigger Suggest',
      keybindings: [
        monaco.KeyMod.Ctrl | monaco.KeyCode.Space,
        monaco.KeyMod.WinCtrl | monaco.KeyCode.Space,
        monaco.KeyMod.Alt | monaco.KeyCode.Space,
      ],
      run: (ed) => fireCompletionsSuggest(ed, { force: true }),
    });

    editor.addAction({
      id: 'reaper.goToDefinition',
      label: 'Go to Definition',
      keybindings: [monaco.KeyCode.F12],
      run: async () => {
        const pos = editor.getPosition();
        if (!pos) return;
        const model = editor.getModel();
        if (!model) return;
        const loc = await lookupDefinition(helpers, model, pos);
        if (!loc) {
          helpers.toast?.('No definition found — wait for Java index or add import', 'info');
        }
      },
    });

    editor.onKeyDown((e) => {
      if (e.keyCode !== monaco.KeyCode.Tab || e.shiftKey) return;
      const ev = e.browserEvent;
      if (ev.altKey || ev.ctrlKey || ev.metaKey) return;

      const ctx = editor._contextKeyService;
      const suggestVisible = ctx?.getContextKeyValue('suggestWidgetVisible');
      const snippetMode = ctx?.getContextKeyValue('inSnippetMode');
      const hasInline = hasActiveInlineSuggestion(editor);

      if (suggestVisible || snippetMode || hasInline) {
        e.preventDefault();
        e.stopPropagation();
        handleEditorTab(editor);
      }
    });

    editor.addAction({
      id: 'reaper.goToSymbol',
      label: 'Go to Symbol in File',
      keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyMod.Shift | monaco.KeyCode.KeyO],
      run: () => editor.getAction('editor.action.quickOutline')?.run(),
    });

    editor.addAction({
      id: 'reaper.inlineSuggest.acceptEnter',
      label: 'Accept Inline Suggestion',
      keybindings: [monaco.KeyCode.Enter],
      precondition: 'inlineSuggestionVisible && !suggestWidgetVisible',
      run: (ed) => {
        acceptInlineOrControlSnippet(ed);
      },
    });

    editor.onDidChangeModelContent(() => {
      const lang = editor.getModel()?.getLanguageId() || 'plaintext';
      helpers.setLanguageStatus?.(langLabel(lang));
    });

    if (reaperDotCompletionHandler) {
      helpers.setCompleteDebugStatus?.(`c${REAPER_COMPLETION_REV} · dot handler ready`);
    } else {
      console.warn('[Reaper] dot handler not attached');
    }
  }

  function isDiagnosablePath(path) {
    if (!path || path.startsWith('.reaper/')) return false;
    const base = (path.split('/').pop() || '').toLowerCase();
    if (!base || base.startsWith('.')) return false;
    if (base === 'dockerfile' || base.startsWith('dockerfile.')) return true;
    if (base === 'makefile' || base === 'gnumakefile' || base === 'cmakelists.txt') return true;
    return langForPath(path) !== 'plaintext';
  }

  function ensureMonacoBasicLanguage(lang) {
    if (typeof require === 'undefined' || !lang || lang === 'plaintext') return;
    const safe = String(lang).replace(/[^a-z0-9_-]/gi, '');
    if (!safe) return;
    try {
      require([`vs/basic-languages/${safe}/${safe}`], () => {});
    } catch {
      /* optional tokenizer module */
    }
  }

  Object.assign(window.ReaperLang || {}, {
    completionRev: () => REAPER_COMPLETION_REV,
    clearCompletionCache: clearIndexCompleteCache,
    coreOnly: false,
    jdkMemberPreview: (qualifier, memberPrefix = '', content = '', path = '', model = null) => {
      if (!qualifier) return [];
      if (qualifier.includes('.') || content) {
        return memberPreviewItems(content, qualifier, memberPrefix, path, model);
      }
      return jdkStaticMemberItems(qualifier, memberPrefix, content);
    },
    memberPreview: memberPreviewItems,
    dotQualifierFromLinePrefix,
    memberDotContext,
    editorLinePrefix,
    langForPath,
    langLabel,
    compilerToolIdsForPath,
    compilerLabelsForPath,
    isDiagnosablePath,
    registerGroovy,
    setupEditorFeatures,
    extractSymbols,
    ensureMonacoBasicLanguage,
    handleDotCompletion: (ed) => {
      if (typeof reaperDotCompletionHandler === 'function') {
        return reaperDotCompletionHandler(ed);
      }
      return window.ReaperLang?.handleDotCompletion?.(ed) ?? -1;
    },
  });
})();
