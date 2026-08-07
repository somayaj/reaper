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

  /** Local inline is vars/objects/keywords only — loops, blocks, empty lines use AI. */
  function skipLocalStatementTemplates() {
    return true;
  }

  /** Completion syntax level from language-context (max of configured JDK and project release). */
  function inlineJavaLevel(helpers) {
    const ctx = helpers.getLanguageContext?.();
    if (ctx?.java_level != null) return ctx.java_level;
    const configured = helpers.getJavaLanguageLevel?.();
    if (configured != null) return configured;
    return ctx?.jdk_level ?? 17;
  }

  /** Parse `for (…; i < coll.length|size(); i++)` — distinguishes arrays, lists, and strings. */
  function parseJavaIndexedForLoop(forInit, indexVar = 'i') {
    const init = String(forInit || '');
    const iv = indexVar.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    let m = init.match(new RegExp(`${iv}\\s*<\\s*(\\w+)\\.length\\b`));
    if (m) return { name: m[1], kind: 'array', indexVar };
    m = init.match(new RegExp(`${iv}\\s*<\\s*(\\w+)\\.size\\s*\\(\\s*\\)`));
    if (m) return { name: m[1], kind: 'list', indexVar };
    m = init.match(new RegExp(`${iv}\\s*<\\s*(\\w+)\\.length\\s*\\(\\s*\\)`));
    if (m) return { name: m[1], kind: 'string', indexVar };
    return null;
  }

  function javaIndexedAccessExpr(parsed, indexVar = 'i') {
    if (!parsed) return '';
    const iv = indexVar || parsed.indexVar || 'i';
    if (parsed.kind === 'array') return `${parsed.name}[${iv}]`;
    if (parsed.kind === 'list') return `${parsed.name}.get(${iv})`;
    if (parsed.kind === 'string') return `${parsed.name}.charAt(${iv})`;
    return '';
  }

  function javaIndexedLoopBodyLine(parsed, indent, javaLevel, lang = 'java') {
    if (!parsed) return '';
    const iv = parsed.indexVar || 'i';
    let access;
    if (lang === 'kotlin') {
      access = parsed.kind === 'list'
        ? `${parsed.name}.get(${iv})`
        : `${parsed.name}[${iv}]`;
    } else {
      access = javaIndexedAccessExpr(parsed, iv);
    }
    if (!access) return '';
    if (javaLevel >= 10 && lang !== 'kotlin') {
      return `${indent}var item = ${access};`;
    }
    return `${indent}${access};`;
  }

  function langForPath(path) {
    const base = (path.split('/').pop() || '').toLowerCase();
    if (!base) return 'plaintext';

    if (base === 'dockerfile' || base.startsWith('dockerfile.')) return 'dockerfile';
    if (base === 'makefile' || base === 'gnumakefile' || base.startsWith('makefile.') || base.endsWith('.mk')) {
      return 'makefile';
    }
    if (base === 'cmakelists.txt') return 'cmake';
    if (base === 'elide.pkl' || base.endsWith('.pkl')) return 'pkl';
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
      c: 'cpp', h: 'cpp',
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
      pkl: 'pkl',
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

  function isGradlePropertiesFile(path) {
    const base = (path.split('/').pop() || '').toLowerCase();
    return base.endsWith('gradle.properties') || base.endsWith('.gradle.properties');
  }

  function isGradleBuildFile(path) {
    if (!path) return false;
    const base = (path.split('/').pop() || '').toLowerCase();
    return base.endsWith('.gradle') || base.endsWith('.gradle.kts');
  }

  /** Languages where workspace symbol/classpath index inline ghosts make sense (not README/prose). */
  const INDEX_INLINE_LANGS = new Set([
    'java', 'kotlin', 'groovy', 'rust', 'javascript', 'typescript', 'python', 'go',
    'csharp', 'ruby', 'php', 'swift', 'c', 'cpp', 'shell', 'lua', 'dart', 'r', 'sql',
  ]);

  function isProseLanguage(path) {
    const lang = langForPath(path);
    return lang === 'markdown' || lang === 'plaintext';
  }

  /** Markup/config paths (md, yaml, pom.xml, etc.) — AI empty lines, skip heavy index work. */
  function isMarkupOrConfigPath(path) {
    if (!path) return false;
    const lang = langForPath(path);
    return [
      'markdown', 'plaintext', 'yaml', 'json', 'html', 'xml', 'toml', 'ini',
      'css', 'scss', 'less', 'sql', 'dockerfile', 'makefile', 'cmake', 'protobuf', 'graphql', 'pkl',
    ].includes(lang);
  }

  function supportsWorkspaceIndexInline(path) {
    if (!path) return false;
    if (isSpringConfigFile(path)) return true;
    return INDEX_INLINE_LANGS.has(langForPath(path));
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
    const minRow = Math.max(0, idx - INLINE_NEARBY_LINES);
    for (let i = idx - 1; i >= minRow; i -= 1) {
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

  function gradlePropertiesKeyPrefix(linePrefix) {
    const trimmed = String(linePrefix || '').split('#')[0].trimEnd();
    if (trimmed.includes('=')) return '';
    return trimmed.trim();
  }

  function collectNearbyGradlePropertyKeys(content, lineNumber) {
    const lines = String(content || '').split(/\r?\n/);
    const keys = [];
    const seen = new Set();
    const minRow = Math.max(0, lineNumber - 1 - INLINE_NEARBY_LINES);
    const maxRow = Math.min(lines.length - 1, lineNumber - 1 + INLINE_NEARBY_LINES);
    for (let i = minRow; i <= maxRow; i += 1) {
      if (i === lineNumber - 1) continue;
      const row = lines[i];
      if (!row.trim() || row.trim().startsWith('#')) continue;
      const key = row.split('=')[0].split('#')[0].trim();
      if (key && !seen.has(key)) {
        seen.add(key);
        keys.push({ key, line: i + 1 });
      }
    }
    return keys;
  }

  function gradlePropertiesLocalInline(path, linePrefix, content, lineNumber) {
    if (!isGradlePropertiesFile(path)) return '';
    const keyPrefix = gradlePropertiesKeyPrefix(linePrefix);
    if (!keyPrefix) return '';
    const lower = keyPrefix.toLowerCase();
    const candidates = collectNearbyGradlePropertyKeys(content, lineNumber)
      .filter((entry) =>
        entry.key.toLowerCase().startsWith(lower) && entry.key.length > keyPrefix.length,
      );
    if (!candidates.length) return '';
    const rankLine = (line) => {
      if (line < lineNumber) return lineNumber - line;
      if (line > lineNumber) {
        const below = line - lineNumber;
        return below <= 3 ? below : 1000 + below;
      }
      return 500;
    };
    candidates.sort((a, b) => {
      const rankA = rankLine(a.line);
      const rankB = rankLine(b.line);
      if (rankA !== rankB) return rankA - rankB;
      const suffixA = a.key.slice(keyPrefix.length);
      const suffixB = b.key.slice(keyPrefix.length);
      return suffixA.length - suffixB.length || a.key.localeCompare(b.key);
    });
    return candidates[0].key.slice(keyPrefix.length);
  }

  function completionSuffixFromLabel(label, prefix) {
    const p = prefix || '';
    if (!label) return '';
    if (!p) return label;
    if (label.startsWith(p)) return label.slice(p.length);
    if (label.toLowerCase().startsWith(p.toLowerCase())) {
      return label.slice(p.length);
    }
    return '';
  }

  const LSP_INLINE_FETCH_MS = 220;
  const LSP_INLINE_MEMBER_FETCH_MS = 140;

  /** Member access and indexed code paths — prefer jdtls/index top completion for inline ghost. */
  function shouldPreferLspInlineGhost(path, linePrefix, content = '', lineNumber = 0) {
    if (!path || !supportsWorkspaceIndexInline(path)) return false;
    if (isWhitespaceOnlyLine(linePrefix)) {
      return shouldFetchEmptyLineInline(path, linePrefix, content, lineNumber);
    }
    if (dotQualifierFromLinePrefix(linePrefix)) return true;
    const token = extractInlinePartialToken(linePrefix);
    if (token && shouldPreferControlKeywordInline(path, linePrefix, token)) return false;
    if (shouldPreferModifierKeywordInline(path, linePrefix, token)) return false;
    return shouldFetchIndexCompletions(linePrefix, token || '', path, { content, lineNumber });
  }

  function inlineGhostTextFromCompletionItem(item) {
    const label = String(item?.label || '');
    let insert = stripSnippetMarkers(String(item?.insert || label));
    if (insert.includes('(') && !label.includes('(')) {
      insert = insert.split('(')[0];
    }
    if (insert.includes('.')) insert = insert.split('.').pop() || insert;
    return insert || label;
  }

  function inlineSuffixFromLspItem(item, linePrefix, prefix) {
    const member = dotQualifierFromLinePrefix(linePrefix);
    const tokenPrefix = member
      ? (member.memberPrefix || '')
      : (prefix || extractInlinePartialToken(linePrefix) || '');
    const candidate = inlineGhostTextFromCompletionItem(item);
    if (!candidate) return '';
    const suffix = completionSuffixFromLabel(candidate, tokenPrefix);
    if (!suffix) return '';
    if (suffix === '()' || suffix.startsWith('(')) return '';
    return suffix;
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
      c: ['clangd', 'clang', 'gcc'],
      cpp: ['clangd', 'clang', 'gcc'],
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
      node: 'Node', tsc: 'tsc', php: 'PHP', clangd: 'clangd', clang: 'clang', gcc: 'gcc',
      swiftc: 'swiftc', luac: 'luac', csc: 'csc', dart: 'dart', bash: 'bash',
      yamllint: 'yamllint', jsonlint: 'jsonlint', ajv: 'ajv',
    };
    return ids.map((id) => labels[id] || id).join(', ');
  }

  function langLabel(lang) {
    const labels = {
      groovy: 'Groovy', kotlin: 'Kotlin', javascript: 'JavaScript', typescript: 'TypeScript',
      plaintext: 'Plain Text', ini: 'Properties', cpp: 'C/C++', pkl: 'Pkl',
    };
    return labels[lang] || (lang.charAt(0).toUpperCase() + lang.slice(1));
  }

  function langLabelForPath(path) {
    const base = (path.split('/').pop() || '').toLowerCase();
    if (base.endsWith('.c')) return 'C';
    if (base.endsWith('.h')) return 'C header';
    if (/\.(cpp|cc|cxx|hpp|hh|hxx)$/.test(base)) return 'C++';
    if (base === 'makefile' || base === 'gnumakefile') return 'Makefile';
    if (base.endsWith('.mk') || base.startsWith('makefile.')) return 'Makefile';
    if (base === 'elide.pkl' || base.endsWith('.pkl')) return 'Pkl (Elide)';
    return langLabel(langForPath(path));
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

  const MAKEFILE_KEYWORDS = [
    'ifeq', 'ifneq', 'ifdef', 'ifndef', 'else', 'endif',
    'include', 'sinclude', 'define', 'endef', 'export', 'unexport',
    'vpath', 'override', '-include', 'undefine',
    '.PHONY', '.SUFFIXES', '.DEFAULT', '.PRECIOUS', '.SECONDARY',
    '.INTERMEDIATE', '.IGNORE', '.SILENT', '.EXPORT_ALL_VARIABLES',
    '.DELETE_ON_ERROR', '.LOW_RESOLUTION_TIME',
  ];

  function registerMakefile() {
    if (window.__reaperMakefileRegistered) return;
    window.__reaperMakefileRegistered = true;

    try {
      monaco.languages.register({ id: 'makefile', aliases: ['Makefile', 'Make', 'make'] });
    } catch {
      /* already registered */
    }

    monaco.languages.setMonarchTokensProvider('makefile', {
      defaultToken: '',
      tokenPostfix: '.makefile',
      ignoreCase: true,
      keywords: MAKEFILE_KEYWORDS,
      varRef: /\$\([^)]*\)|\$\{[^}]*\}|\$[@*<+%?|!:]/,
      tokenizer: {
        root: [
          [/^\s*#.*$/, 'comment'],
          [/^\t+[^\n]*/, 'string'],
          [/^[ ]{4,}[^\s#].*/, 'string'],
          [/@varRef/, 'variable'],
          [/\.[A-Z][A-Z0-9_]*/, 'keyword'],
          [/[A-Za-z_.][\w.-]*(?=\s*(?::=|\?=|\+=|=))/, 'variable.name'],
          [/[A-Za-z_.][\w.-]*(?=\s*:)/, 'type.identifier'],
          [/@keywords/, 'keyword'],
          [/:=|\?=|\+=|=/, 'operator'],
          [/::/, 'delimiter'],
          [/:/, 'delimiter'],
          [/"([^"\\]|\\.)*"/, 'string'],
          [/'([^'\\]|\\.)*'/, 'string'],
        ],
      },
    });

    monaco.languages.setLanguageConfiguration('makefile', {
      comments: { lineComment: '#' },
      brackets: [['(', ')']],
      autoClosingPairs: [
        { open: '(', close: ')' },
        { open: '"', close: '"' },
        { open: "'", close: "'" },
      ],
    });
  }

  const PKL_KEYWORDS = [
    'amends', 'amend', 'import', 'as', 'module', 'extends', 'class', 'typealias',
    'local', 'const', 'fixed', 'hidden', 'open', 'abstract', 'external', 'new',
    'if', 'else', 'when', 'for', 'in', 'let', 'function', 'read', 'throw',
    'null', 'true', 'false', 'this', 'super', 'outer', 'Unknown',
  ];

  function registerPkl() {
    if (window.__reaperPklRegistered) return;
    window.__reaperPklRegistered = true;

    try {
      monaco.languages.register({ id: 'pkl', aliases: ['Pkl', 'pkl', 'Elide'] });
    } catch {
      /* already registered */
    }

    monaco.languages.setMonarchTokensProvider('pkl', {
      defaultToken: '',
      tokenPostfix: '.pkl',
      keywords: PKL_KEYWORDS,
      tokenizer: {
        root: [
          [/\/\/.*$/, 'comment'],
          [/\/\*/, 'comment', '@comment'],
          [/"([^"\\]|\\.)*"/, 'string'],
          [/"([^"\\]|\\.)*$/, 'string.invalid'],
          [/'([^'\\]|\\.)*'/, 'string'],
          [/\b\d+(\.\d+)?\b/, 'number'],
          // Match words via monarch cases (do not expand keywords into a regex).
          [/[A-Z][\w.]*/, 'type.identifier'],
          [/[a-z_][\w]*/, {
            cases: {
              '@keywords': 'keyword',
              '@default': 'identifier',
            },
          }],
          [/[\[\]]/, 'delimiter.square'],
          [/[{}]/, 'delimiter.curly'],
          [/[()]/, 'delimiter.parenthesis'],
          [/[=:;,.]/, 'delimiter'],
        ],
        comment: [
          [/[^/*]+/, 'comment'],
          [/\*\//, 'comment', '@pop'],
          [/[/*]/, 'comment'],
        ],
      },
    });

    monaco.languages.setLanguageConfiguration('pkl', {
      comments: { lineComment: '//', blockComment: ['/*', '*/'] },
      brackets: [
        ['{', '}'],
        ['[', ']'],
        ['(', ')'],
      ],
      autoClosingPairs: [
        { open: '{', close: '}' },
        { open: '[', close: ']' },
        { open: '(', close: ')' },
        { open: '"', close: '"' },
        { open: "'", close: "'" },
      ],
    });
  }

  function ensureReaperCustomLanguage(lang) {
    if (lang === 'groovy') registerGroovy();
    if (lang === 'makefile') registerMakefile();
    if (lang === 'pkl') registerPkl();
  }

  const REAPER_CUSTOM_LANGS = new Set(['groovy', 'makefile', 'pkl']);

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
    'dockerfile', 'makefile', 'cmake', 'protobuf', 'graphql', 'pkl', 'plaintext',
  ];

  /** Reaper uses one Monaco model + setModelLanguage per tab (inmemory:// URIs). */
  const REAPER_DOC_SELECTOR = ALL_EDITOR_LANGS;
  const REAPER_COMPLETION_REV = '354';
  let reaperDotCompletionHandler = null;

  /** Keywords that belong only in SQL buffers — never suggest in Java/other langs. */
  const SQL_ONLY_KEYWORDS = new Set([
    'SELECT', 'FROM', 'WHERE', 'INSERT', 'UPDATE', 'DELETE', 'CREATE', 'DROP', 'ALTER',
    'JOIN', 'INNER', 'OUTER', 'LEFT', 'RIGHT', 'FULL', 'CROSS', 'ON', 'USING',
    'GROUP', 'ORDER', 'HAVING', 'LIMIT', 'OFFSET', 'UNION', 'INTERSECT', 'EXCEPT',
    'INTO', 'VALUES', 'SET', 'TABLE', 'INDEX', 'VIEW', 'DATABASE', 'SCHEMA',
    'GRANT', 'REVOKE', 'COMMIT', 'ROLLBACK', 'BEGIN', 'TRANSACTION', 'TRUNCATE',
    'DISTINCT', 'AS', 'AND', 'OR', 'NOT', 'NULL', 'IS', 'IN', 'LIKE', 'ILIKE',
    'BETWEEN', 'EXISTS', 'CASE', 'WHEN', 'THEN', 'ELSE', 'END', 'CAST', 'COALESCE',
  ]);

  function editorLanguagesCompatible(expected, actual) {
    if (!expected || !actual) return true;
    if (expected === actual) return true;
    if (expected === 'cpp' && (actual === 'cpp' || actual === 'c')) return true;
    if (expected === 'javascript' && actual === 'typescript') return true;
    if (expected === 'typescript' && actual === 'javascript') return true;
    return false;
  }

  /** Keep completions/inline/hover scoped to the active tab's file language. */
  function editorScopeMatches(path, model, { sync = true } = {}) {
    const rel = String(path || '').trim();
    if (!rel) return false;
    const expected = langForPath(rel);
    if (!expected || expected === 'plaintext') return false;
    if (!model) return true;
    if (sync) ensureModelLanguageForPath(rel, model);
    const actual = model.getLanguageId?.() || '';
    return editorLanguagesCompatible(expected, actual);
  }

  function suggestionMatchesEditorLanguage(path, label) {
    const lang = langForPath(path || '');
    const text = String(label || '').trim();
    if (!text) return false;
    if (lang === 'sql') return true;
    const upper = text.toUpperCase();
    if (SQL_ONLY_KEYWORDS.has(upper)) return false;
    if (lang === 'java' || lang === 'kotlin' || lang === 'groovy') {
      if (lang !== 'groovy' && upper === 'DEF') return false;
      if (lang !== 'python' && (upper === 'ELIF' || upper === 'PASS')) return false;
      if (lang !== 'ruby' && upper === 'END') return false;
      if (lang !== 'php' && upper === 'ECHO') return false;
    }
    return true;
  }

  /** Google Java Format: spaces around `=` in assignments (not `==`, `+=`, …). */
  function maybeInsertJavaAssignSpace(ed) {
    if (ed._reaperAssignSpaceEdit || ed._reaperExpandingBrace) return;
    const path = ed._reaperHelpers?.getActivePath?.() || '';
    if (langForPath(path) !== 'java') return;
    const model = ed.getModel();
    const pos = ed.getPosition();
    if (!model || !pos || pos.column < 2) return;
    const line = model.getLineContent(pos.lineNumber);
    const col = pos.column;
    if (line[col - 2] !== '=') return;
    const prev = col >= 3 ? line[col - 3] : '';
    if ('!<>=+-*/%&|^'.includes(prev)) return;
    if (line[col - 1] === '=') return;
    const after = line[col - 1];
    const needBefore = !!prev && prev !== ' ' && prev !== '\t';
    const needAfter = after !== ' ' && after !== '\t';
    if (!needBefore && !needAfter) return;
    ed._reaperAssignSpaceEdit = true;
    try {
      // One replace keeps the caret stable: `name=x` → `name = x`, `name=` → `name = `.
      const startCol = col - 1; // `=` under caret-1
      const endCol = col;
      const text = `${needBefore ? ' ' : ''}=${needAfter ? ' ' : ''}`;
      ed.executeEdits('reaper-java-assign-space', [{
        range: new monaco.Range(pos.lineNumber, startCol, pos.lineNumber, endCol),
        text,
      }]);
    } finally {
      ed._reaperAssignSpaceEdit = false;
    }
  }

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
    if (typeof ed?._reaperContent === 'string') return ed._reaperContent;
    const tab = ed?._reaperHelpers?.getActiveTabContent?.();
    if (typeof tab === 'string') return tab;
    return model.getValue();
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
  const MIN_INDEX_INLINE_CHARS = 2;
  /** Inline identifier / empty-line templates use only this many lines above and below the cursor. */
  const INLINE_NEARBY_LINES = 5;
  const MAX_INLINE_LINES = 15;

  function isNearbyLine(line, cursorLine, radius = INLINE_NEARBY_LINES) {
    return line >= cursorLine - radius && line <= cursorLine + radius;
  }
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

  /** Statement/construct templates — local ghosts for if/for/while (AI optional). */
  function controlStructureInlineSuffix(path, linePrefix, content, lineNumber, javaLevel = 17) {
    const trimmed = String(linePrefix || '').trimEnd();
    if (!trimmed || !isCodeStatementLanguage(path)) return '';
    const lang = langForPath(path);
    const ctx = editorContextAroundLine(content, lineNumber);

    if (lang === 'java' || lang === 'kotlin' || lang === 'groovy') {
      if (lineEndsWithKeyword(trimmed, 'for')) {
        const target = bestJavaIndexedForTarget(ctx);
        if (target?.init) return ` (${target.init}) {`;
        return ' (int i = 0; i < length; i++) {';
      }
      if (lineEndsWithKeyword(trimmed, 'while')) {
        const cond = inferInlineCondition(path, content, lineNumber, 'while', javaLevel);
        return ` (${cond}) {`;
      }
      if (lineEndsWithKeyword(trimmed, 'if')) {
        const cond = inferInlineCondition(path, content, lineNumber, 'if', javaLevel);
        return ` (${cond}) {`;
      }
      if (lineEndsWithKeyword(trimmed, 'do')) return ' {';
      if (lineEndsWithKeyword(trimmed, 'switch')) return ' (value) {';
      if (lineEndsWithKeyword(trimmed, 'try')) return ' {';
      if (lang === 'kotlin' && lineEndsWithKeyword(trimmed, 'when')) return ' (x) {';
      if (trimmed.endsWith('for (') || trimmed.endsWith('for(')) {
        const target = bestJavaIndexedForTarget(ctx);
        if (target?.init) return target.init;
        return 'int i = 0; i < length; i++';
      }
      if (trimmed.endsWith('if (') || trimmed.endsWith('if(')) {
        return inferInlineCondition(path, content, lineNumber, 'if', javaLevel);
      }
      if (trimmed.endsWith('while (') || trimmed.endsWith('while(')) {
        return inferInlineCondition(path, content, lineNumber, 'while', javaLevel);
      }
    }

    if (lang === 'python') {
      if (lineEndsWithKeyword(trimmed, 'for')) return ' item in items:';
      if (lineEndsWithKeyword(trimmed, 'if')) return ' condition:';
      if (lineEndsWithKeyword(trimmed, 'while')) return ' condition:';
      if (lineEndsWithKeyword(trimmed, 'elif')) return ' condition:';
      if (lineEndsWithKeyword(trimmed, 'def')) return ' name():';
      if (lineEndsWithKeyword(trimmed, 'with')) return ' resource:';
    }

    if (lang === 'javascript' || lang === 'typescript') {
      if (lineEndsWithKeyword(trimmed, 'for')) return ' (let i = 0; i < length; i++) {';
      if (lineEndsWithKeyword(trimmed, 'while')) return ' (condition) {';
      if (lineEndsWithKeyword(trimmed, 'if')) return ' (condition) {';
      if (lineEndsWithKeyword(trimmed, 'switch')) return ' (value) {';
      if (lineEndsWithKeyword(trimmed, 'try')) return ' {';
      if (trimmed.endsWith('if (') || trimmed.endsWith('if(')) return 'condition';
      if (trimmed.endsWith('for (') || trimmed.endsWith('for(')) return 'let i = 0; i < length; i++';
    }

    if (lang === 'rust') {
      if (lineEndsWithKeyword(trimmed, 'for')) return ' item in items {';
      if (lineEndsWithKeyword(trimmed, 'while')) return ' condition {';
      if (lineEndsWithKeyword(trimmed, 'if')) return ' condition {';
      if (lineEndsWithKeyword(trimmed, 'match')) return ' value {';
      if (lineEndsWithKeyword(trimmed, 'loop')) return ' {';
    }

    if (lang === 'go') {
      if (lineEndsWithKeyword(trimmed, 'for')) return ' _, _ := range items {';
      if (lineEndsWithKeyword(trimmed, 'if')) return ' condition {';
      if (lineEndsWithKeyword(trimmed, 'switch')) return ' x {';
    }

    if (lang === 'csharp') {
      if (lineEndsWithKeyword(trimmed, 'for')) return ' (int i = 0; i < length; i++) {';
      if (lineEndsWithKeyword(trimmed, 'while')) return ' (condition) {';
      if (lineEndsWithKeyword(trimmed, 'if')) return ' (condition) {';
      if (lineEndsWithKeyword(trimmed, 'switch')) return ' (value) {';
      if (lineEndsWithKeyword(trimmed, 'foreach')) return ' (var item in items) {';
    }

    if (lang === 'c' || lang === 'cpp') {
      if (lineEndsWithKeyword(trimmed, 'for')) return ' (int i = 0; i < length; i++) {';
      if (lineEndsWithKeyword(trimmed, 'while')) return ' (condition) {';
      if (lineEndsWithKeyword(trimmed, 'if')) return ' (condition) {';
      if (lineEndsWithKeyword(trimmed, 'switch')) return ' (value) {';
    }

    if (lang === 'ruby') {
      if (lineEndsWithKeyword(trimmed, 'for')) return ' item in items';
      if (lineEndsWithKeyword(trimmed, 'if')) return ' condition';
      if (lineEndsWithKeyword(trimmed, 'while')) return ' condition';
      if (lineEndsWithKeyword(trimmed, 'def')) return ' name';
      if (lineEndsWithKeyword(trimmed, 'class')) return ' Name';
    }

    if (lang === 'php') {
      if (lineEndsWithKeyword(trimmed, 'if')) return ' (condition) {';
      if (lineEndsWithKeyword(trimmed, 'while')) return ' (condition) {';
      if (lineEndsWithKeyword(trimmed, 'for')) return ' ($i = 0; $i < count($items); $i++) {';
      if (lineEndsWithKeyword(trimmed, 'foreach')) return ' ($items as $item) {';
    }

    if (lang === 'swift') {
      if (lineEndsWithKeyword(trimmed, 'for')) return ' item in items {';
      if (lineEndsWithKeyword(trimmed, 'while')) return ' condition {';
      if (lineEndsWithKeyword(trimmed, 'if')) return ' condition {';
      if (lineEndsWithKeyword(trimmed, 'guard')) return ' condition else {';
      if (lineEndsWithKeyword(trimmed, 'switch')) return ' value {';
    }

    if (lang === 'shell') {
      if (lineEndsWithKeyword(trimmed, 'if')) return ' [ condition ]; then';
      if (lineEndsWithKeyword(trimmed, 'for')) return ' item in items; do';
      if (lineEndsWithKeyword(trimmed, 'function')) return ' name() {';
    }

    if (lang === 'lua') {
      if (lineEndsWithKeyword(trimmed, 'if')) return ' condition then';
      if (lineEndsWithKeyword(trimmed, 'while')) return ' condition do';
      if (lineEndsWithKeyword(trimmed, 'for')) return ' i, item in ipairs(items) do';
      if (lineEndsWithKeyword(trimmed, 'function')) return ' name()';
    }

    if (lang === 'dart') {
      if (lineEndsWithKeyword(trimmed, 'for')) return ' (var item in items) {';
      if (lineEndsWithKeyword(trimmed, 'while')) return ' (condition) {';
      if (lineEndsWithKeyword(trimmed, 'if')) return ' (condition) {';
    }

    return '';
  }

  function controlStructureInlineGhost(path, linePrefix, content, lineNumber, javaLevel = 17) {
    return controlStructureInlineSuffix(path, linePrefix, content, lineNumber, javaLevel);
  }

  function inlineGhostSuffix(
    path, linePrefix, content, lineNumber, javaLevel, column = 0, indexCtx = null,
    { fast = false } = {},
  ) {
    const inline = localInlineSuggestion(
      path, linePrefix, content, lineNumber, javaLevel, column, indexCtx, { fast },
    );
    if (!inline) return '';
    if (inline.includes('\n')) return inline.split('\n')[0];
    return inline;
  }

  function lineIndent(linePrefix) {
    const m = String(linePrefix ?? '').match(/^[\t ]*/);
    return m ? m[0] : '';
  }

  function extractInlinePartialToken(linePrefix) {
    const s = String(linePrefix || '');
    // Cursor after whitespace starts a new token — do not complete the previous word
    // (`private ` must not keep matching PrivateKeyEntry / privateField).
    if (/\s$/.test(s)) return '';
    const trimmed = s.trimEnd();
    if (!trimmed) return '';
    const m = trimmed.match(/([A-Za-z_$][\w$]*)$/);
    return m ? m[1] : '';
  }

  /** Line ends with a binary/logical operator — cursor may follow immediately or after spaces (`foo +|` / `foo + |`). */
  function operatorTailMatch(linePrefix) {
    const s = String(linePrefix || '');
    const trimmed = s.trimEnd();
    if (!trimmed) return null;

    const tryEnd = (text) => {
      const multi = ['===', '!==', '==', '!=', '<=', '>=', '&&', '||', '<<', '>>', '>>>', '++', '--'];
      for (const op of multi) {
        if (text.endsWith(op)) {
          return { op, lhs: text.slice(0, -op.length).trimEnd() };
        }
      }
      const last = text[text.length - 1];
      if ('+-*/%&|^<>?:'.includes(last)) {
        if (last === '-' && text.length === 1) return null;
        // Ternary `:`, not YAML keys / labels / `case x:` — those must keep Space free.
        if (last === ':' && !text.includes('?')) return null;
        return { op: last, lhs: text.slice(0, -1).trimEnd() };
      }
      if (
        text.endsWith('=')
        && !text.endsWith('==')
        && !text.endsWith('!=')
        && !text.endsWith('<=')
        && !text.endsWith('>=')
      ) {
        return { op: '=', lhs: text.slice(0, -1).trimEnd() };
      }
      return null;
    };

    const trailingSpace = s.length > trimmed.length;
    if (trailingSpace) {
      return tryEnd(trimmed);
    }
    return tryEnd(trimmed);
  }

  function isOperandContext(linePrefix) {
    const tail = operatorTailMatch(linePrefix);
    return tail != null && !!tail.lhs;
  }

  function isAfterOperatorWithSpace(linePrefix) {
    return isOperandContext(linePrefix) && /\s$/.test(String(linePrefix || ''));
  }

  function rankScopeCandidates(candidates, lineNumber, {
    token = '',
    preferValues = false,
    excludeNames = null,
    path = '',
  } = {}) {
    const lower = token.toLowerCase();
    const exclude = excludeNames || new Set();
    const filtered = candidates.filter((c) => {
      if (!c?.name || exclude.has(c.name)) return false;
      if (token && !c.name.toLowerCase().startsWith(lower)) return false;
      return c.name.length > token.length;
    });
    if (!filtered.length) return [];
    const tokenIsUpper = token ? /^[A-Z]/.test(token) : false;
    const stmtKeywords = new Set(statementKeywordsForPath(path).map((kw) => kw.toLowerCase()));
    filtered.sort((a, b) => {
      if (preferValues && a.isType !== b.isType) return a.isType ? 1 : -1;
      if (tokenIsUpper && token.length >= 3 && a.isType !== b.isType) return a.isType ? -1 : 1;
      const rankLine = (c) => {
        if (c.maxLine < lineNumber) return lineNumber - c.maxLine;
        if (c.minLine > lineNumber) {
          const below = c.minLine - lineNumber;
          if (below <= 3) return below;
          return 1000 + below;
        }
        return 500;
      };
      const kwA = stmtKeywords.has(a.name.toLowerCase()) ? 1 : 0;
      const kwB = stmtKeywords.has(b.name.toLowerCase()) ? 1 : 0;
      if (kwA !== kwB) return kwA - kwB;
      const lineRankA = rankLine(a);
      const lineRankB = rankLine(b);
      if (lineRankA !== lineRankB) return lineRankA - lineRankB;
      if (b.count !== a.count) return b.count - a.count;
      return a.name.length - b.name.length || a.name.localeCompare(b.name);
    });
    return filtered;
  }

  function operandInlineSuffix(content, path, lineNumber, linePrefix) {
    const tail = operatorTailMatch(linePrefix);
    if (!tail || !tail.lhs || !content || !isCodeStatementLanguage(path)) return '';
    const exclude = new Set();
    const lhsIdent = tail.lhs.match(/([A-Za-z_$][\w$]*)$/);
    if (lhsIdent) exclude.add(lhsIdent[1]);
    const preferValues = '+-*/%&|^'.includes(tail.op) || tail.op === '==';
    if (isJavaLikePath(path)) {
      const vars = scopeVarsForInline(content, lineNumber, path, {
        preferValues: true,
        excludeNames: exclude,
      });
      if (vars.length) return pickScopeVarSuffix(vars);
    }
    const candidates = collectContentIdentifierCandidates(content, path, lineNumber);
    const ranked = rankScopeCandidates(candidates, lineNumber, {
      token: '',
      preferValues,
      excludeNames: exclude,
      path,
    });
    return ranked[0]?.name || '';
  }

  /** Fast scope: small line window (keystroke path — avoids full nearby scan). */
  function fastLineWindowScopeSuffix(content, path, lineNumber, token) {
    if (!token || !content) return '';
    if (token.length === 1 && !/^[A-Z]/.test(token)) return '';
    const keywords = new Set(clientKeywordsForPath(path).map((kw) => kw.toLowerCase()));
    const lines = String(content).split(/\r?\n/);
    const cur = lineNumber - 1;
    if (!Number.isFinite(cur) || cur < 0) return '';
    const lower = token.toLowerCase();
    const radius = 2;
    const counts = new Map();
    const meta = new Map();
    const identRe = /\b([A-Za-z_][\w$]*)\b/g;
    for (let i = Math.max(0, cur - radius); i <= Math.min(lines.length - 1, cur + radius); i += 1) {
      const line = lines[i];
      identRe.lastIndex = 0;
      let m;
      for (;;) {
        m = identRe.exec(line);
        if (!m) break;
        const name = m[1];
        if (keywords.has(name.toLowerCase())) continue;
        if (!name.toLowerCase().startsWith(lower) || name.length <= token.length) continue;
        counts.set(name, (counts.get(name) || 0) + 1);
        const prev = meta.get(name);
        if (!prev) {
          meta.set(name, {
            name,
            isType: /^[A-Z]/.test(name),
            minLine: i + 1,
            maxLine: i + 1,
            count: 1,
          });
        } else {
          prev.minLine = Math.min(prev.minLine, i + 1);
          prev.maxLine = Math.max(prev.maxLine, i + 1);
          prev.count = counts.get(name);
        }
      }
    }
    const candidates = [...meta.values()];
    if (!candidates.length) return '';
    if (token.length === 1 && /^[A-Z]/.test(token) && candidates.length === 1) {
      return candidates[0].name.slice(1);
    }
    if (token.length < MIN_INDEX_INLINE_CHARS) return '';
    const ranked = rankScopeCandidates(candidates, lineNumber, { token, path });
    return ranked[0]?.name.slice(token.length) || '';
  }

  function isReturnContext(linePrefix) {
    const s = String(linePrefix || '');
    if (/\breturn\s+$/.test(s)) return true;
    const trimmed = s.trimEnd();
    return /\breturn(?:\s+[A-Za-z_$][\w$]*)?$/.test(trimmed);
  }

  /** Method/ctor call argument list — not control-flow parens (`if (`) and not after the call is closed. */
  function isCallArgContext(linePrefix, path) {
    if (!isJavaLikePath(path)) return false;
    if (isInsideControlParen(linePrefix)) return false;
    const trimmed = String(linePrefix || '').trimEnd();
    const idx = trimmed.lastIndexOf('(');
    if (idx < 0) return false;
    const before = trimmed.slice(0, idx).trimEnd();
    if (!before || !/[\w)\]]$/.test(before)) return false;
    const inside = trimmed.slice(idx + 1);
    if (inside.includes('(') || inside.includes('{')) return false;
    // `foo()` / `foo(x)` — call already closed; Space must type normally.
    if (inside.includes(')')) return false;
    return true;
  }

  function scopeVarsForInline(content, lineNumber, path, {
    token = '',
    preferValues = true,
    excludeNames = null,
  } = {}) {
    if (!content || !isJavaLikePath(path)) return [];
    const exclude = excludeNames || new Set();
    const vars = collectJavaScopeVariables(content, lineNumber)
      .filter((v) => v.name && !exclude.has(v.name));
    if (!token) return vars;
    const lower = token.toLowerCase();
    return vars.filter((v) => v.name.toLowerCase().startsWith(lower) && v.name.length > token.length);
  }

  function pickScopeVarSuffix(vars, token = '') {
    if (!vars.length) return '';
    const pick = vars[vars.length - 1];
    return token ? pick.name.slice(token.length) : pick.name;
  }

  function returnInlineSuffix(content, path, lineNumber, linePrefix) {
    if (!isReturnContext(linePrefix) || !content || !isCodeStatementLanguage(path)) return '';
    const token = extractInlinePartialToken(linePrefix);
    if (isJavaLikePath(path)) {
      const vars = scopeVarsForInline(content, lineNumber, path, { token, preferValues: true });
      if (vars.length) return pickScopeVarSuffix(vars, token);
    }
    if (token) {
      const scope = scopeIdentifierInlineSuffix(content, path, lineNumber, token);
      if (scope) return scope;
      const fast = fastLineWindowScopeSuffix(content, path, lineNumber, token);
      if (fast) return fast;
    }
    const candidates = collectContentIdentifierCandidates(content, path, lineNumber);
    const ranked = rankScopeCandidates(candidates, lineNumber, { token, preferValues: true, path });
    const name = ranked[0]?.name;
    if (!name) return '';
    return token ? name.slice(token.length) : name;
  }

  function callArgInlineSuffix(content, path, lineNumber, linePrefix) {
    if (!isCallArgContext(linePrefix, path) || !content) return '';
    const trimmed = linePrefix.trimEnd();
    const idx = trimmed.lastIndexOf('(');
    const inside = trimmed.slice(idx + 1);
    const token = extractInlinePartialToken(linePrefix);
    const used = new Set();
    inside.split(',').forEach((seg) => {
      const id = seg.trim().match(/^(\w+)/);
      if (id?.[1]) used.add(id[1]);
    });
    const vars = scopeVarsForInline(content, lineNumber, path, {
      token,
      preferValues: true,
      excludeNames: used,
    });
    if (vars.length) return pickScopeVarSuffix(vars, token);
    if (token) {
      const candidates = collectContentIdentifierCandidates(content, path, lineNumber)
        .filter((c) => !used.has(c.name));
      const ranked = rankScopeCandidates(candidates, lineNumber, {
        token,
        preferValues: true,
        excludeNames: used,
        path,
      });
      const name = ranked[0]?.name;
      if (name) return name.slice(token.length);
    }
    return '';
  }

  function inlineFilterUsesFullPrefix(linePrefix, path) {
    if (isWhitespaceOnlyLine(linePrefix)) return true;
    if (dotQualifierFromLinePrefix(linePrefix)) return true;
    if (isOperandContext(linePrefix)) return true;
    if (isReturnContext(linePrefix)) return true;
    if (isCallArgContext(linePrefix, path)) return true;
    return false;
  }

  function lineEndsWithKeyword(trimmed, kw) {
    const re = new RegExp(`(?:^|[\\s({])${kw}$`);
    return re.test(trimmed);
  }

  const CONTROL_KEYWORD_PREFIXES = [
    'if', 'else', 'for', 'while', 'do', 'switch', 'case', 'try', 'catch', 'finally',
    'elif', 'def', 'class', 'function', 'fun', 'func', 'when', 'match', 'guard', 'foreach',
    'break', 'continue', 'return', 'loop', 'then',
  ];

  /** Shared statement keywords merged with per-language CLIENT_KEYWORDS for inline ghosts. */
  const SHARED_STMT_KEYWORDS = [
    'if', 'else', 'for', 'while', 'do', 'switch', 'case', 'try', 'catch', 'finally',
    'elif', 'def', 'class', 'function', 'fun', 'func', 'when', 'match', 'guard', 'foreach',
    'return', 'break', 'continue', 'throw', 'with', 'loop', 'then', 'namespace', 'package',
    'import', 'export', 'struct', 'interface', 'enum', 'trait', 'impl', 'async', 'await',
  ];

  function isCodeStatementLanguage(path) {
    if (isProseLanguage(path)) return false;
    const lang = langForPath(path);
    return !['json', 'yaml', 'html', 'css', 'scss', 'less', 'xml', 'sql', 'ini', 'toml',
      'dockerfile', 'makefile', 'cmake', 'protobuf', 'graphql', 'plaintext'].includes(lang);
  }

  function statementKeywordsForPath(path) {
    if (!isCodeStatementLanguage(path)) return clientKeywordsForPath(path);
    const lang = langForPath(path);
    let shared = SHARED_STMT_KEYWORDS;
    if (lang === 'java' || lang === 'kotlin' || lang === 'groovy') {
      const exclude = new Set([
        'def', 'fun', 'func', 'fn', 'elif', 'pass', 'end', 'echo', 'unless', 'until',
        'yield', 'raise', 'except', 'nonlocal', 'lambda', 'trait', 'impl', 'mut', 'pub',
        'defer', 'go', 'chan', 'package', 'foreach', 'namespace', 'using', 'declare',
        'match', 'guard', 'then', 'loop', 'self', 'with',
      ]);
      shared = SHARED_STMT_KEYWORDS.filter((kw) => !exclude.has(kw.toLowerCase()));
    }
    return [...new Set([...clientKeywordsForPath(path), ...shared])];
  }

  function isControlKeywordPrefix(token) {
    if (!token) return false;
    // Uppercase tokens are Java/Kotlin types (File, String), not keyword prefixes.
    if (/[A-Z]/.test(token)) return false;
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

  function editorContextAroundLine(content, lineNumber, maxAbove = INLINE_NEARBY_LINES, maxBelow = INLINE_NEARBY_LINES) {
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

  /** Import/using lines — index ghost from project classpath (compiler version), never AI. */
  function isImportTypingLine(path, linePrefix) {
    const trimmed = String(linePrefix || '').trimStart();
    if (!trimmed) return false;
    const lang = langForPath(path || '');
    switch (lang) {
      case 'csharp':
        return /^using\s/.test(trimmed);
      case 'python':
        return /^import\s/.test(trimmed) || /^from\s+\S+\s+import\b/.test(trimmed);
      case 'rust':
        return /^use\s/.test(trimmed);
      case 'javascript':
      case 'typescript':
      case 'tsx':
      case 'jsx':
        return /^import\s/.test(trimmed) || /^export\s+.*\sfrom\s/.test(trimmed);
      default:
        return /^import(?:\s+static)?\s/.test(trimmed);
    }
  }

  function shouldFetchIndexCompletions(linePrefix, prefix, path, { afterPause = false, content = '', lineNumber = 0 } = {}) {
    if (path && !supportsWorkspaceIndexInline(path)) return false;
    {
      const tok = prefix || extractInlinePartialToken(linePrefix) || '';
      // Finished modifiers only — still show index while naming vars (`String value`).
      if (shouldPreferModifierKeywordInline(path, linePrefix, tok)
        && isModifierLeadInFreeTyping(path, linePrefix, tok)) {
        return false;
      }
    }
    if (path && isImportTypingLine(path, linePrefix)) return true;
    if (path && isSpringConfigFile(path)) {
      const p = prefix || extractInlinePartialToken(linePrefix) || '';
      return p.length >= 1 || AUTOCOMPLETE_TRIGGER_RE.test(linePrefix || '');
    }
    if (path && isWhitespaceOnlyLine(linePrefix) && supportsWorkspaceIndexInline(path)) {
      if (!content || !lineNumber) return afterPause;
      return shouldFetchEmptyLineInline(path, linePrefix, content, lineNumber);
    }
    const p = prefix || '';
    if (p.length >= MIN_INDEX_INLINE_CHARS) return true;
    if (afterPause && p.length >= 1) return true;
    if (AUTOCOMPLETE_TRIGGER_RE.test(linePrefix || '')) return true;
    if (dotQualifierFromLinePrefix(linePrefix)) return true;
    if (isAfterOperatorWithSpace(linePrefix)) return true;
    if (isOperandContext(linePrefix)) return true;
    if (isReturnContext(linePrefix)) return true;
    if (isCallArgContext(linePrefix, path)) return true;
    if (isInsideControlParen(linePrefix)) return true;
    return false;
  }

  /** Statement finished on this line (`foo();`) — no inline ghost after the semicolon. */
  function isLineInlineComplete(linePrefix) {
    return /;\s*$/.test(String(linePrefix || '').trimEnd());
  }

  function isWhitespaceOnlyLine(linePrefix) {
    return !String(linePrefix || '').trim();
  }

  /** Per-language characters / patterns that mark a finished line segment (no inline ghost). */
  function pauseInlineRulesForPath(path) {
    const lang = langForPath(path || '');
    switch (lang) {
      case 'python':
        return { semicolon: false, braces: false, bracket: true, paren: true, colonBlock: true };
      case 'ruby':
        return { semicolon: true, braces: false, bracket: true, paren: true, endKeyword: true };
      case 'shell':
      case 'dockerfile':
      case 'makefile':
      case 'cmake':
        return { semicolon: false, braces: true, bracket: false, paren: true, lineContinuation: true };
      case 'markdown':
      case 'plaintext':
        return { semicolon: false, braces: false, bracket: false, paren: false, proseLine: true };
      case 'yaml':
      case 'toml':
      case 'ini':
        return { semicolon: false, braces: false, bracket: false, paren: false, configLine: true };
      case 'xml':
      case 'html':
        return { semicolon: false, braces: false, bracket: false, paren: false, xmlTag: true };
      case 'json':
        return { semicolon: false, braces: true, bracket: true, paren: false, comma: true };
      case 'css':
      case 'scss':
      case 'less':
        return { semicolon: true, braces: true, bracket: false, paren: false, cssColon: true };
      case 'sql':
        return { semicolon: true, braces: false, bracket: false, paren: true };
      default:
        return { semicolon: true, braces: true, bracket: true, paren: true };
    }
  }

  /**
   * Caret after a natural line boundary for the active language.
   * Whitespace-only lines are not paused (empty-line templates / AI still apply).
   */
  function shouldPauseInlineAtCursor(path, linePrefix, content, lineNumber, column) {
    if (isWhitespaceOnlyLine(linePrefix)) return false;
    const trimmed = String(linePrefix || '').trimEnd();
    if (!trimmed) return false;
    const rules = pauseInlineRulesForPath(path);
    const lang = langForPath(path || '');
    const last = trimmed[trimmed.length - 1];

    if (rules.semicolon && /;\s*$/.test(trimmed)) return true;

    if (rules.braces && (last === '{' || last === '}')) return true;
    if (rules.bracket && last === ']') return true;

    if (rules.paren && last === ')') {
      if (!extractInlinePartialToken(linePrefix)) return true;
    }

    if (rules.colonBlock && lang === 'python' && /:\s*$/.test(trimmed)) {
      if (/\b(def|class|if|elif|else|for|while|try|except|finally|with|match|case)\b/.test(trimmed)) {
        return true;
      }
    }

    if (rules.endKeyword && lang === 'ruby' && /\b(end|do)\s*$/.test(trimmed)) return true;

    if (rules.lineContinuation && last === '\\') return true;

    if (rules.xmlTag && last === '>' && /<[^>]+>\s*$/.test(trimmed)) return true;

    if (rules.comma && last === ',' && (lang === 'json')) return true;

    if (rules.cssColon && last === ';') return true;

    if (rules.configLine) {
      if (lang === 'yaml' && /:\s*(\S.*)?$/.test(trimmed) && !trimmed.endsWith(':')) return true;
      if ((lang === 'toml' || lang === 'ini') && /=/.test(trimmed) && /\S=\S/.test(trimmed.replace(/\s/g, ''))) {
        return true;
      }
    }

    if (rules.proseLine && lang === 'markdown') {
      const body = trimmed.trimStart();
      if (/^[-*+]\s+\S/.test(body)) return true;
      if (/^\d+\.\s+\S/.test(body)) return true;
    }

    if (content && lineNumber && column) {
      const after = lineAfterCursor(content, lineNumber, column);
      if (!after.trim() && rules.paren && /\)\s*$/.test(trimmed) && !extractInlinePartialToken(linePrefix)) {
        return true;
      }
    }
    return false;
  }

  /** After a typing pause, try workspace index inline when there is something to complete. */
  function shouldPrefetchInlineOnPause(path, linePrefix, content, lineNumber) {
    if (!path) return false;
    {
      const tok = extractInlinePartialToken(linePrefix) || '';
      if (isModifierLeadInFreeTyping(path, linePrefix, tok)) return false;
    }
    if (isImportTypingLine(path, linePrefix)) return supportsWorkspaceIndexInline(path);
    if (shouldPauseInlineAtCursor(path, linePrefix, content, lineNumber)) return false;
    const trimmedAll = String(linePrefix || '').trim();
    if (!trimmedAll) {
      if (!content || !lineNumber) {
        return supportsWorkspaceIndexInline(path) || isSpringConfigFile(path);
      }
      return supportsWorkspaceIndexInline(path)
        || isSpringConfigFile(path)
        || shouldFetchEmptyLineInline(path, linePrefix, content, lineNumber);
    }
    const token = extractInlinePartialToken(linePrefix);
    if (shouldPreferModifierKeywordInline(path, linePrefix, token)) return false;
    if (dotQualifierFromLinePrefix(linePrefix)) {
      return supportsWorkspaceIndexInline(path);
    }
    if (!token) return false;
    return supportsWorkspaceIndexInline(path);
  }

  function inlineIndexPrefix(linePrefix) {
    const memberCtx = dotQualifierFromLinePrefix(linePrefix);

    if (memberCtx) return memberCtx.memberPrefix || '';
    return extractInlinePartialToken(linePrefix) || '';
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

  function mapIndexItemToSuggestion(item, range, seen, memberContext, path = '') {
    const label = item.label;
    if (!label || seen.has(label)) return null;
  
    if (path && !suggestionMatchesEditorLanguage(path, label)) return null;
    if (!isCodeLikeCompletion(label, item.kind)) return null;
    seen.add(label);
    const kind = String(item.kind || '').toLowerCase();
    let insertText = item.insert;
    if (!insertText) {
      insertText = kind === 'method' ? `${label}()` : label;
    }
    const sortRank = memberContext
      ? '0'
      : (kind === 'class' || kind === 'interface' || kind === 'type' || kind === 'enum' ? '0' : '1');
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
      suggestion.documentationHtml = `<div class="reaper-java-hover-doc">${documentationHtmlFromText(doc)}</div>`;
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

  function hoverDisplayName(info) {
    if (info?.name) return info.name;
    const sig = info?.signature || '';
    if (isJdtlsSourceLine(sig)) return '';
    const typeMatch = sig.match(/\b(?:class|interface|enum|record|@interface)\s+(\w+)/);
    if (typeMatch) return typeMatch[1];
    const methodMatch = sig.match(/(\w+)\s*\(/);
    return methodMatch ? methodMatch[1] : '';
  }

  function hoverInfoHasContent(info) {
    return !!(info && (info.name || info.signature || info.documentation));
  }

  function isJdtlsSourceLine(text) {
    if (window.ReaperAgentMarkdown?.isJdtlsSourceLine) {
      return window.ReaperAgentMarkdown.isJdtlsSourceLine(text);
    }
    const t = String(text || '').trim();
    if (!t) return false;
    if (/^source:/i.test(t) && (/jdt\.ls/i.test(t) || /file:\/\//i.test(t) || /jdt:\/\//i.test(t))) {
      return true;
    }
    return /jdt\.ls-java-project/i.test(t);
  }

  function documentationHtmlFromText(text) {
    const raw = window.ReaperAgentMarkdown?.stripJdtlsSourceAttribution?.(text) ?? String(text || '');
    const doc = raw.trim();
    if (!doc) return '';
    const md = window.ReaperAgentMarkdown;
    if (md?.looksLikeJavadocHtml?.(doc)) {
      return md.renderJavadocHtml(doc);
    }
    if (md?.looksLikeLspMarkdown?.(doc)) {
      return md.renderLspDocumentation(doc);
    }
    if (doc.includes('Parameters:')) {
      return doc.split('\n').map((line) => `<div class="reaper-javadoc-line">${escapeHtml(line)}</div>`).join('');
    }
    return javadocHtmlFromText(doc);
  }

  function lspMarkupContent(text) {
    const value = String(text || '').trim();
    if (!value) return undefined;
    const out = { value };
    if (window.ReaperAgentMarkdown?.looksLikeLspMarkdown?.(value)) {
      out.kind = (typeof monaco !== 'undefined' && monaco.MarkupKind)
        ? monaco.MarkupKind.Markdown
        : 'markdown';
    }
    return out;
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
    if (!hoverInfoHasContent(info)) return '';
    const displayName = hoverDisplayName(info);
    const kindRaw = info.kind && info.kind !== 'symbol' ? info.kind : '';
    const kind = kindRaw ? escapeHtml(kindRaw) : '';
    let html = '<div class="reaper-java-hover">';
    const signature = info.signature && !isJdtlsSourceLine(info.signature) ? info.signature : '';
    if (signature) {
      html += '<div class="reaper-java-hover-head">';
      if (kind) {
        html += `<span class="reaper-java-hover-kind-pill">${kind}</span>`;
      }
      html += `<div class="reaper-java-hover-sig">${escapeHtml(signature)}</div>`;
      html += '</div>';
    } else if (displayName) {
      html += '<div class="reaper-java-hover-title">';
      html += `<span class="reaper-java-hover-name">${escapeHtml(displayName)}</span>`;
      if (kind) {
        html += `<span class="reaper-java-hover-kind-pill">${kind}</span>`;
      }
      html += '</div>';
    }
    if (info.documentation) {
      const docHtml = documentationHtmlFromText(info.documentation);
      html += `<div class="reaper-java-hover-doc">${docHtml}</div>`;
    } else if (['method', 'field', 'function', 'macro', 'typedef', 'struct', 'class', 'enum', 'namespace'].includes(info.kind)) {
      html += '<div class="reaper-java-hover-empty">No documentation available.</div>';
    }
    html += '</div>';
    return html;
  }

  function buildEditorHoverHtml(symInfo, markers) {
    const parts = [];
    if (hoverInfoHasContent(symInfo)) {
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
    const rootCls = markers?.length && !hoverInfoHasContent(symInfo)
      ? 'reaper-editor-hover reaper-editor-hover--problems-only'
      : 'reaper-editor-hover';
    return `<div class="${rootCls}">${parts.join('')}</div>`;
  }

  function itemDocumentationParts(item) {
    if (item?.documentationHtml) {
      return { text: '', html: String(item.documentationHtml).trim() };
    }
    if (!item?.documentation) return { text: '', html: '' };
    const doc = item.documentation;
    if (typeof doc === 'string') {
      const trimmed = doc.trim();
      if (window.ReaperAgentMarkdown?.looksLikeLspMarkdown?.(trimmed)) {
        return { text: '', html: window.ReaperAgentMarkdown.renderLspDocumentation(trimmed) };
      }
      return { text: trimmed, html: '' };
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
    if (!hoverInfoHasContent(info)) return '';
    const displayName = hoverDisplayName(info);
    const parts = [];
    if (info.signature) {
      parts.push(`\`\`\`java\n${info.signature}\n\`\`\``);
    } else if (displayName) {
      parts.push(`**${displayName}**${info.kind && info.kind !== 'symbol' ? ` · ${info.kind}` : ''}`);
    }
    if (info.documentation) {
      const doc = window.ReaperAgentMarkdown?.stripJdtlsSourceAttribution?.(info.documentation)
        ?? info.documentation;
      if (doc) parts.push(formatJavadocMarkdown(doc));
    } else if (info.kind === 'method' || info.kind === 'field') {
      parts.push('*No documentation.*');
    }
    return parts.join('\n\n');
  }

  function renderJavadocBody(el, text) {
    const doc = String(text || '').trim();
    if (!doc) {
      el.innerHTML = '';
      return;
    }
    const md = window.ReaperAgentMarkdown;
    if (md?.looksLikeLspMarkdown?.(doc)) {
      el.innerHTML = md.renderLspDocumentation(doc);
      return;
    }
    el.innerHTML = '';
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

  const SQL_DOCS = {
    SELECT: {
      kind: 'keyword',
      signature: 'SELECT [ DISTINCT ] expr [, ...] | *',
      documentation: 'Retrieves rows from tables, views, or subqueries. Use DISTINCT to remove duplicate rows.',
    },
    FROM: {
      kind: 'keyword',
      signature: 'FROM table_reference [, ...]',
      documentation: 'Names the source relation(s) for the query — tables, views, subqueries, or JOIN expressions.',
    },
    WHERE: {
      kind: 'keyword',
      signature: 'WHERE condition',
      documentation: 'Filters rows after FROM/JOIN. Only rows matching the condition are returned.',
    },
    INSERT: {
      kind: 'keyword',
      signature: 'INSERT INTO table [( columns )] VALUES ( ... )',
      documentation: 'Adds new rows to a table. Column list is optional when supplying values for all columns.',
    },
    UPDATE: {
      kind: 'keyword',
      signature: 'UPDATE table SET column = expr [, ...] [ WHERE ... ]',
      documentation: 'Modifies existing rows. Always use WHERE unless you intend to update every row.',
    },
    DELETE: {
      kind: 'keyword',
      signature: 'DELETE FROM table [ WHERE ... ]',
      documentation: 'Removes rows from a table. Omitting WHERE deletes all rows.',
    },
    CREATE: {
      kind: 'keyword',
      signature: 'CREATE TABLE | INDEX | VIEW ...',
      documentation: 'Defines a new database object such as a table, index, view, or extension.',
    },
    DROP: {
      kind: 'keyword',
      signature: 'DROP TABLE | INDEX | VIEW ...',
      documentation: 'Removes an existing database object.',
    },
    ALTER: {
      kind: 'keyword',
      signature: 'ALTER TABLE table action [, ...]',
      documentation: 'Changes the structure of an existing table — add/drop/rename columns, constraints, etc.',
    },
    TABLE: {
      kind: 'keyword',
      signature: 'TABLE name ( column type [, ...] )',
      documentation: 'Declares a relational table with named columns and data types.',
    },
    INDEX: {
      kind: 'keyword',
      signature: 'CREATE INDEX name ON table ( column [, ...] )',
      documentation: 'Creates an index to speed up lookups and joins on the listed columns.',
    },
    VIEW: {
      kind: 'keyword',
      signature: 'CREATE VIEW name AS query',
      documentation: 'Defines a stored query exposed as a virtual table.',
    },
    JOIN: {
      kind: 'keyword',
      signature: '[ INNER | LEFT | RIGHT | FULL ] JOIN table ON condition',
      documentation: 'Combines rows from two relations. INNER keeps matches only; LEFT keeps all left rows.',
    },
    INNER: {
      kind: 'keyword',
      signature: 'INNER JOIN table ON condition',
      documentation: 'Returns rows where the join condition matches in both relations.',
    },
    LEFT: {
      kind: 'keyword',
      signature: 'LEFT [ OUTER ] JOIN table ON condition',
      documentation: 'Returns all rows from the left relation plus matching rows from the right (NULL if no match).',
    },
    RIGHT: {
      kind: 'keyword',
      signature: 'RIGHT [ OUTER ] JOIN table ON condition',
      documentation: 'Returns all rows from the right relation plus matching rows from the left.',
    },
    FULL: {
      kind: 'keyword',
      signature: 'FULL [ OUTER ] JOIN table ON condition',
      documentation: 'Returns rows when there is a match in either relation; unmatched sides are NULL-padded.',
    },
    OUTER: {
      kind: 'keyword',
      signature: 'LEFT | RIGHT | FULL OUTER JOIN',
      documentation: 'Used with JOIN to preserve non-matching rows from the outer side(s).',
    },
    ON: {
      kind: 'keyword',
      signature: 'ON join_condition',
      documentation: 'Specifies how two relations are matched in a JOIN.',
    },
    AS: {
      kind: 'keyword',
      signature: 'expr AS alias | table AS alias',
      documentation: 'Assigns a temporary name to a column or table reference in the query.',
    },
    AND: {
      kind: 'keyword',
      signature: 'condition AND condition',
      documentation: 'Logical AND — both conditions must be true.',
    },
    OR: {
      kind: 'keyword',
      signature: 'condition OR condition',
      documentation: 'Logical OR — at least one condition must be true.',
    },
    NOT: {
      kind: 'keyword',
      signature: 'NOT condition | NOT IN | NOT NULL',
      documentation: 'Negates a boolean expression or membership test.',
    },
    IN: {
      kind: 'keyword',
      signature: 'expr IN ( value [, ...] ) | expr IN ( subquery )',
      documentation: 'Tests whether a value equals any member of a list or subquery result.',
    },
    EXISTS: {
      kind: 'keyword',
      signature: 'EXISTS ( subquery )',
      documentation: 'True when the subquery returns at least one row.',
    },
    BETWEEN: {
      kind: 'keyword',
      signature: 'expr BETWEEN low AND high',
      documentation: 'True when expr is greater than or equal to low and less than or equal to high.',
    },
    LIKE: {
      kind: 'keyword',
      signature: "expr LIKE pattern [ ESCAPE 'char' ]",
      documentation: 'Pattern match using % (any sequence) and _ (single character). Case-sensitive in PostgreSQL.',
    },
    ILIKE: {
      kind: 'keyword',
      signature: "expr ILIKE pattern [ ESCAPE 'char' ]",
      documentation: 'Case-insensitive LIKE (PostgreSQL).',
    },
    IS: {
      kind: 'keyword',
      signature: 'expr IS NULL | IS NOT NULL | IS TRUE | IS FALSE',
      documentation: 'Tests NULL or boolean truth — use IS NULL instead of = NULL.',
    },
    NULL: {
      kind: 'keyword',
      signature: 'NULL',
      documentation: 'Represents a missing or unknown value. Comparisons with NULL yield NULL, not true/false.',
    },
    DISTINCT: {
      kind: 'keyword',
      signature: 'SELECT DISTINCT ... | COUNT(DISTINCT col)',
      documentation: 'Removes duplicate rows or counts unique values.',
    },
    ORDER: {
      kind: 'keyword',
      signature: 'ORDER BY column [ ASC | DESC ] [, ...]',
      documentation: 'Sorts the result set. ASC is default; DESC reverses order.',
    },
    BY: {
      kind: 'keyword',
      signature: 'ORDER BY ... | GROUP BY ...',
      documentation: 'Introduces sort keys (ORDER BY) or grouping columns (GROUP BY).',
    },
    GROUP: {
      kind: 'keyword',
      signature: 'GROUP BY column [, ...]',
      documentation: 'Collapses rows sharing the same grouping column values for aggregate queries.',
    },
    HAVING: {
      kind: 'keyword',
      signature: 'HAVING aggregate_condition',
      documentation: 'Filters groups after aggregation — like WHERE for grouped results.',
    },
    LIMIT: {
      kind: 'keyword',
      signature: 'LIMIT count [ OFFSET skip ]',
      documentation: 'Caps the number of rows returned.',
    },
    OFFSET: {
      kind: 'keyword',
      signature: 'OFFSET skip',
      documentation: 'Skips the first N rows — often paired with LIMIT for pagination.',
    },
    UNION: {
      kind: 'keyword',
      signature: 'query UNION [ ALL ] query',
      documentation: 'Combines result sets vertically. UNION removes duplicates; UNION ALL keeps them.',
    },
    ALL: {
      kind: 'keyword',
      signature: 'UNION ALL | SELECT ALL',
      documentation: 'With UNION, keeps duplicate rows from combined queries.',
    },
    EXCEPT: {
      kind: 'keyword',
      signature: 'query EXCEPT query',
      documentation: 'Returns rows from the first query not present in the second (PostgreSQL).',
    },
    INTERSECT: {
      kind: 'keyword',
      signature: 'query INTERSECT query',
      documentation: 'Returns rows common to both queries.',
    },
    VALUES: {
      kind: 'keyword',
      signature: 'VALUES ( row ), ( row ), ...',
      documentation: 'Constructs a inline table literal, often used with INSERT or as a subquery source.',
    },
    SET: {
      kind: 'keyword',
      signature: 'UPDATE ... SET column = expr [, ...]',
      documentation: 'Assigns new values to columns in an UPDATE statement.',
    },
    INTO: {
      kind: 'keyword',
      signature: 'INSERT INTO table ... | SELECT ... INTO TEMP',
      documentation: 'Target table for INSERT, or creates a table from a SELECT result.',
    },
    DEFAULT: {
      kind: 'keyword',
      signature: 'column DEFAULT expression',
      documentation: 'Uses the column default when no value is supplied on INSERT.',
    },
    PRIMARY: {
      kind: 'keyword',
      signature: 'PRIMARY KEY ( column [, ...] )',
      documentation: 'Uniquely identifies each row; indexed automatically.',
    },
    KEY: {
      kind: 'keyword',
      signature: 'PRIMARY KEY | FOREIGN KEY',
      documentation: 'Declares primary or foreign key constraints.',
    },
    FOREIGN: {
      kind: 'keyword',
      signature: 'FOREIGN KEY ( col ) REFERENCES other_table ( col )',
      documentation: 'Enforces referential integrity to another table column.',
    },
    REFERENCES: {
      kind: 'keyword',
      signature: 'REFERENCES table ( column )',
      documentation: 'Target table/column for a foreign key constraint.',
    },
    UNIQUE: {
      kind: 'keyword',
      signature: 'UNIQUE ( column [, ...] ) | column type UNIQUE',
      documentation: 'Ensures all values in the column(s) are distinct.',
    },
    CONSTRAINT: {
      kind: 'keyword',
      signature: 'CONSTRAINT name ...',
      documentation: 'Names a table constraint (PRIMARY KEY, UNIQUE, CHECK, FOREIGN KEY).',
    },
    CHECK: {
      kind: 'keyword',
      signature: 'CHECK ( condition )',
      documentation: 'Requires rows to satisfy a boolean expression.',
    },
    CASCADE: {
      kind: 'keyword',
      signature: 'ON DELETE CASCADE | ON UPDATE CASCADE',
      documentation: 'Propagates deletes/updates to dependent foreign-key rows.',
    },
    RETURNING: {
      kind: 'keyword',
      signature: 'INSERT | UPDATE | DELETE ... RETURNING column [, ...]',
      documentation: 'Returns modified rows from a write statement (PostgreSQL).',
    },
    WITH: {
      kind: 'keyword',
      signature: 'WITH [ RECURSIVE ] name AS ( query ) SELECT ...',
      documentation: 'Common Table Expression (CTE) — names a subquery used in the main statement.',
    },
    RECURSIVE: {
      kind: 'keyword',
      signature: 'WITH RECURSIVE cte AS ( ... )',
      documentation: 'Allows a CTE to reference itself for hierarchical/graph queries.',
    },
    CASE: {
      kind: 'keyword',
      signature: 'CASE WHEN cond THEN result ... [ ELSE default ] END',
      documentation: 'Conditional expression — SQL equivalent of if/else.',
    },
    WHEN: {
      kind: 'keyword',
      signature: 'CASE WHEN condition THEN result',
      documentation: 'Branch condition inside a CASE expression.',
    },
    THEN: {
      kind: 'keyword',
      signature: 'WHEN condition THEN result',
      documentation: 'Result value when the matching WHEN condition is true.',
    },
    ELSE: {
      kind: 'keyword',
      signature: 'CASE ... ELSE default END',
      documentation: 'Fallback result when no WHEN branch matches.',
    },
    END: {
      kind: 'keyword',
      signature: 'CASE ... END',
      documentation: 'Closes a CASE expression.',
    },
    TRUE: {
      kind: 'literal',
      signature: 'TRUE',
      documentation: 'Boolean true literal.',
    },
    FALSE: {
      kind: 'literal',
      signature: 'FALSE',
      documentation: 'Boolean false literal.',
    },
    COUNT: {
      kind: 'function',
      signature: 'COUNT(*) | COUNT(column) | COUNT(DISTINCT column)',
      documentation: 'Aggregate: counts rows or non-NULL values.',
    },
    SUM: {
      kind: 'function',
      signature: 'SUM(numeric_column)',
      documentation: 'Aggregate: total of numeric values (ignores NULL).',
    },
    AVG: {
      kind: 'function',
      signature: 'AVG(numeric_column)',
      documentation: 'Aggregate: arithmetic mean of numeric values.',
    },
    MIN: {
      kind: 'function',
      signature: 'MIN(expr)',
      documentation: 'Aggregate or scalar: smallest value.',
    },
    MAX: {
      kind: 'function',
      signature: 'MAX(expr)',
      documentation: 'Aggregate or scalar: largest value.',
    },
    COALESCE: {
      kind: 'function',
      signature: 'COALESCE(value, fallback [, ...])',
      documentation: 'Returns the first argument that is not NULL.',
    },
    NULLIF: {
      kind: 'function',
      signature: 'NULLIF(a, b)',
      documentation: 'Returns NULL when a equals b; otherwise returns a.',
    },
    CAST: {
      kind: 'function',
      signature: 'CAST(expr AS type) | expr::type',
      documentation: 'Converts a value to another data type.',
    },
    EXTRACT: {
      kind: 'function',
      signature: 'EXTRACT(field FROM timestamp)',
      documentation: 'Pulls part of a date/time (YEAR, MONTH, DAY, HOUR, etc.).',
    },
    NOW: {
      kind: 'function',
      signature: 'NOW()',
      documentation: 'Current transaction timestamp (PostgreSQL). Same as CURRENT_TIMESTAMP.',
    },
    CURRENT_TIMESTAMP: {
      kind: 'function',
      signature: 'CURRENT_TIMESTAMP',
      documentation: 'Current date and time at the start of the current transaction.',
    },
    CURRENT_DATE: {
      kind: 'function',
      signature: 'CURRENT_DATE',
      documentation: 'Current date (no time component).',
    },
    VARCHAR: {
      kind: 'type',
      signature: 'VARCHAR(n) | CHARACTER VARYING(n)',
      documentation: 'Variable-length character string with optional max length.',
    },
    TEXT: {
      kind: 'type',
      signature: 'TEXT',
      documentation: 'Unlimited-length character string (PostgreSQL).',
    },
    INTEGER: {
      kind: 'type',
      signature: 'INTEGER | INT',
      documentation: '4-byte signed integer.',
    },
    BIGINT: {
      kind: 'type',
      signature: 'BIGINT',
      documentation: '8-byte signed integer.',
    },
    BOOLEAN: {
      kind: 'type',
      signature: 'BOOLEAN | BOOL',
      documentation: 'True/false logical type.',
    },
    TIMESTAMP: {
      kind: 'type',
      signature: 'TIMESTAMP [ WITH TIME ZONE ]',
      documentation: 'Date and time without (or with) time zone.',
    },
    UUID: {
      kind: 'type',
      signature: 'UUID',
      documentation: '128-bit universally unique identifier (PostgreSQL).',
    },
    SERIAL: {
      kind: 'type',
      signature: 'SERIAL | BIGSERIAL',
      documentation: 'Auto-incrementing integer column (PostgreSQL shorthand for INTEGER + sequence).',
    },
    JSONB: {
      kind: 'type',
      signature: 'JSONB',
      documentation: 'Binary JSON storage with indexing support (PostgreSQL).',
    },
    ARRAY: {
      kind: 'type',
      signature: 'type[] | ARRAY[ ... ]',
      documentation: 'PostgreSQL array type or array literal constructor.',
    },
    ASC: {
      kind: 'keyword',
      signature: 'ORDER BY column ASC',
      documentation: 'Ascending sort order (default).',
    },
    DESC: {
      kind: 'keyword',
      signature: 'ORDER BY column DESC',
      documentation: 'Descending sort order.',
    },
  };

  function sqlDocForWord(word) {
    if (!word) return null;
    return SQL_DOCS[word.toUpperCase()] || null;
  }

  const C_DOCS = {
    if: { kind: 'keyword', signature: 'if (condition) statement', documentation: 'Executes statement when condition is non-zero (true).' },
    else: { kind: 'keyword', signature: 'else statement', documentation: 'Alternative branch when the preceding if condition is false.' },
    for: { kind: 'keyword', signature: 'for (init; condition; step) statement', documentation: 'Counted loop — init once, test condition each iteration, run step after body.' },
    while: { kind: 'keyword', signature: 'while (condition) statement', documentation: 'Repeats statement while condition is non-zero.' },
    do: { kind: 'keyword', signature: 'do statement while (condition);', documentation: 'Executes statement at least once, then repeats while condition holds.' },
    switch: { kind: 'keyword', signature: 'switch (expr) { case ...: ... }', documentation: 'Multi-way branch on integer expression value.' },
    case: { kind: 'keyword', signature: 'case constant: statements', documentation: 'Labels a branch inside switch; falls through unless break is used.' },
    break: { kind: 'keyword', signature: 'break;', documentation: 'Exits the innermost loop or switch.' },
    continue: { kind: 'keyword', signature: 'continue;', documentation: 'Skips to the next iteration of the innermost loop.' },
    return: { kind: 'keyword', signature: 'return [expr];', documentation: 'Exits the current function, optionally with a value.' },
    struct: { kind: 'keyword', signature: 'struct name { members... };', documentation: 'Defines a composite type grouping named members.' },
    union: { kind: 'keyword', signature: 'union name { members... };', documentation: 'Defines a type whose members share the same storage.' },
    enum: { kind: 'keyword', signature: 'enum name { A, B, ... };', documentation: 'Defines a set of named integer constants.' },
    typedef: { kind: 'keyword', signature: 'typedef existing type alias;', documentation: 'Creates a synonym for an existing type name.' },
    static: { kind: 'keyword', signature: 'static ...', documentation: 'File-local linkage for globals/functions, or persistent storage for locals.' },
    extern: { kind: 'keyword', signature: 'extern type name;', documentation: 'Declares a symbol defined in another translation unit.' },
    const: { kind: 'keyword', signature: 'const type name = value;', documentation: 'Read-only object — value must not be modified through this name.' },
    volatile: { kind: 'keyword', signature: 'volatile type name;', documentation: 'Inhibits certain optimizations; required for hardware-mapped or signal-handled memory.' },
    inline: { kind: 'keyword', signature: 'inline type fn(...);', documentation: 'Hint to embed function body at call sites (C99+).' },
    sizeof: { kind: 'operator', signature: 'sizeof(type) | sizeof expr', documentation: 'Yields the size in bytes of a type or expression.' },
    void: { kind: 'type', signature: 'void', documentation: 'Absence of value — used for functions that return nothing.' },
    int: { kind: 'type', signature: 'int', documentation: 'Signed integer type, typically 32 bits.' },
    char: { kind: 'type', signature: 'char', documentation: 'Smallest addressable unit; often used for bytes and narrow characters.' },
    short: { kind: 'type', signature: 'short', documentation: 'Signed integer, at least 16 bits.' },
    long: { kind: 'type', signature: 'long', documentation: 'Integer at least as wide as int; long long is wider still.' },
    float: { kind: 'type', signature: 'float', documentation: 'Single-precision IEEE floating point.' },
    double: { kind: 'type', signature: 'double', documentation: 'Double-precision IEEE floating point.' },
    signed: { kind: 'type', signature: 'signed int', documentation: 'Explicitly signed integer (default for char/int).' },
    unsigned: { kind: 'type', signature: 'unsigned int', documentation: 'Non-negative integer type.' },
    bool: { kind: 'type', signature: '_Bool / bool (C99+)', documentation: 'Boolean type — 0 is false, non-zero is true.' },
    true: { kind: 'constant', signature: 'true / 1', documentation: 'Boolean true (C99 stdbool.h).' },
    false: { kind: 'constant', signature: 'false / 0', documentation: 'Boolean false (C99 stdbool.h).' },
    NULL: { kind: 'constant', signature: '#define NULL ((void*)0)', documentation: 'Null pointer constant (stddef.h).' },
    printf: { kind: 'function', signature: 'int printf(const char *fmt, ...);', documentation: 'stdio.h — formatted output to stdout. Returns characters written or negative on error.' },
    fprintf: { kind: 'function', signature: 'int fprintf(FILE *stream, const char *fmt, ...);', documentation: 'stdio.h — formatted output to a FILE stream.' },
    sprintf: { kind: 'function', signature: 'int sprintf(char *buf, const char *fmt, ...);', documentation: 'stdio.h — formatted output into a char buffer (unsafe; prefer snprintf).' },
    snprintf: { kind: 'function', signature: 'int snprintf(char *buf, size_t n, const char *fmt, ...);', documentation: 'stdio.h — bounded formatted output into a buffer (C99).' },
    scanf: { kind: 'function', signature: 'int scanf(const char *fmt, ...);', documentation: 'stdio.h — formatted input from stdin.' },
    fgets: { kind: 'function', signature: 'char *fgets(char *s, int n, FILE *stream);', documentation: 'stdio.h — reads a line into buffer s, at most n-1 chars.' },
    fputs: { kind: 'function', signature: 'int fputs(const char *s, FILE *stream);', documentation: 'stdio.h — writes string s to stream (no automatic newline).' },
    puts: { kind: 'function', signature: 'int puts(const char *s);', documentation: 'stdio.h — writes string and newline to stdout.' },
    fopen: { kind: 'function', signature: 'FILE *fopen(const char *path, const char *mode);', documentation: 'stdio.h — opens a file; mode e.g. "r", "w", "a", "rb". Returns NULL on failure.' },
    fclose: { kind: 'function', signature: 'int fclose(FILE *stream);', documentation: 'stdio.h — closes an open FILE stream.' },
    malloc: { kind: 'function', signature: 'void *malloc(size_t size);', documentation: 'stdlib.h — allocates size bytes; contents are indeterminate. Returns NULL on failure.' },
    calloc: { kind: 'function', signature: 'void *calloc(size_t count, size_t size);', documentation: 'stdlib.h — allocates and zero-initializes count * size bytes.' },
    realloc: { kind: 'function', signature: 'void *realloc(void *ptr, size_t size);', documentation: 'stdlib.h — resizes an existing allocation; may move memory.' },
    free: { kind: 'function', signature: 'void free(void *ptr);', documentation: 'stdlib.h — releases memory from malloc/calloc/realloc. Passing NULL is a no-op.' },
    exit: { kind: 'function', signature: 'void exit(int status);', documentation: 'stdlib.h — terminates process; status 0 means success.' },
    abort: { kind: 'function', signature: 'void abort(void);', documentation: 'stdlib.h — abnormal termination, raising SIGABRT.' },
    atoi: { kind: 'function', signature: 'int atoi(const char *s);', documentation: 'stdlib.h — parses decimal int from string; no error reporting.' },
    strlen: { kind: 'function', signature: 'size_t strlen(const char *s);', documentation: 'string.h — length of null-terminated string, excluding the terminator.' },
    strcpy: { kind: 'function', signature: 'char *strcpy(char *dest, const char *src);', documentation: 'string.h — copies src into dest including NUL (buffer must be large enough).' },
    strncpy: { kind: 'function', signature: 'char *strncpy(char *dest, const char *src, size_t n);', documentation: 'string.h — copies at most n bytes; may not NUL-terminate if src longer than n.' },
    strcmp: { kind: 'function', signature: 'int strcmp(const char *a, const char *b);', documentation: 'string.h — lexicographic compare; returns <0, 0, or >0.' },
    strncmp: { kind: 'function', signature: 'int strncmp(const char *a, const char *b, size_t n);', documentation: 'string.h — compare at most n characters.' },
    strcat: { kind: 'function', signature: 'char *strcat(char *dest, const char *src);', documentation: 'string.h — appends src to dest; dest must have room.' },
    strchr: { kind: 'function', signature: 'char *strchr(const char *s, int c);', documentation: 'string.h — finds first occurrence of byte c in s.' },
    strstr: { kind: 'function', signature: 'char *strstr(const char *haystack, const char *needle);', documentation: 'string.h — finds first occurrence of needle in haystack.' },
    memcpy: { kind: 'function', signature: 'void *memcpy(void *dest, const void *src, size_t n);', documentation: 'string.h — copies n bytes; regions must not overlap (use memmove).' },
    memmove: { kind: 'function', signature: 'void *memmove(void *dest, const void *src, size_t n);', documentation: 'string.h — copies n bytes; safe when regions overlap.' },
    memset: { kind: 'function', signature: 'void *memset(void *s, int c, size_t n);', documentation: 'string.h — fills n bytes of s with byte c.' },
    memcmp: { kind: 'function', signature: 'int memcmp(const void *a, const void *b, size_t n);', documentation: 'string.h — compares first n bytes of two blocks.' },
    assert: { kind: 'macro', signature: 'assert(expr);', documentation: 'assert.h — aborts if expr is false when NDEBUG is not defined.' },
  };

  const CPP_DOCS = {
    class: { kind: 'keyword', signature: 'class Name { ... };', documentation: 'Defines a user type with members, access control, and optional inheritance.' },
    namespace: { kind: 'keyword', signature: 'namespace name { ... }', documentation: 'Groups declarations under a named scope; use name::symbol to refer to members.' },
    using: { kind: 'keyword', signature: 'using alias = type; | using namespace ns;', documentation: 'Creates a type alias or imports names from another namespace.' },
    template: { kind: 'keyword', signature: 'template<typename T> ...', documentation: 'Defines a generic function, class, or alias parameterized by types or values.' },
    typename: { kind: 'keyword', signature: 'template<typename T>', documentation: 'Declares a template type parameter or dependent type name.' },
    virtual: { kind: 'keyword', signature: 'virtual return_type fn();', documentation: 'Enables dynamic dispatch — derived overrides are called through base pointers/references.' },
    override: { kind: 'keyword', signature: 'return_type fn() override;', documentation: 'Marks a member function as overriding a virtual base function (C++11).' },
    final: { kind: 'keyword', signature: 'class C final { ... };', documentation: 'Prevents further derivation from a class, or further overriding of a virtual function.' },
    public: { kind: 'keyword', signature: 'public: members...', documentation: 'Access specifier — members are accessible everywhere.' },
    private: { kind: 'keyword', signature: 'private: members...', documentation: 'Access specifier — members accessible only within the class and friends.' },
    protected: { kind: 'keyword', signature: 'protected: members...', documentation: 'Access specifier — members accessible in the class, friends, and derived classes.' },
    friend: { kind: 'keyword', signature: 'friend class Other;', documentation: 'Grants another class or function access to private/protected members.' },
    explicit: { kind: 'keyword', signature: 'explicit Type(args);', documentation: 'Prevents implicit conversions from constructor arguments.' },
    noexcept: { kind: 'keyword', signature: 'void fn() noexcept;', documentation: 'Declares that a function does not throw exceptions (C++11).' },
    constexpr: { kind: 'keyword', signature: 'constexpr int fn();', documentation: 'Expression or function evaluable at compile time (C++11/14/17).' },
    decltype: { kind: 'keyword', signature: 'decltype(expr)', documentation: 'Deduces the type of an expression at compile time.' },
    auto: { kind: 'keyword', signature: 'auto x = expr;', documentation: 'Deduces variable type from its initializer (C++11).' },
    nullptr: { kind: 'constant', signature: 'nullptr', documentation: 'Null pointer constant with its own type (C++11); prefer over NULL.' },
    new: { kind: 'operator', signature: 'new Type(args) | new Type[n]', documentation: 'Allocates dynamic storage and constructs object(s); throws std::bad_alloc on failure.' },
    delete: { kind: 'operator', signature: 'delete ptr; | delete[] ptr;', documentation: 'Destroys and deallocates memory allocated by new / new[].' },
    this: { kind: 'keyword', signature: 'this', documentation: 'Pointer to the current object inside a non-static member function.' },
    operator: { kind: 'keyword', signature: 'return_type operator op(...);', documentation: 'Defines or overloads an operator for a user-defined type.' },
    try: { kind: 'keyword', signature: 'try { ... } catch (...) { ... }', documentation: 'Begins an exception-handling block.' },
    catch: { kind: 'keyword', signature: 'catch (const E& e) { ... }', documentation: 'Handles exceptions thrown in the matching try block.' },
    throw: { kind: 'keyword', signature: 'throw expr;', documentation: 'Raises an exception; use throw; to rethrow the current exception.' },
    static_cast: { kind: 'operator', signature: 'static_cast<T>(expr)', documentation: 'Well-defined compile-time cast between related types (e.g. base ↔ derived, numeric).' },
    dynamic_cast: { kind: 'operator', signature: 'dynamic_cast<T>(expr)', documentation: 'Safe downcast for polymorphic types; returns nullptr/reference failure at runtime.' },
    reinterpret_cast: { kind: 'operator', signature: 'reinterpret_cast<T>(expr)', documentation: 'Low-level bitwise reinterpretation between unrelated pointer types.' },
    const_cast: { kind: 'operator', signature: 'const_cast<T>(expr)', documentation: 'Adds or removes const/volatile qualifiers (only safe when object was not originally const).' },
    cout: { kind: 'object', signature: 'std::ostream std::cout', documentation: 'iostream — standard character output stream (stdout). Use with << operator.' },
    cin: { kind: 'object', signature: 'std::istream std::cin', documentation: 'iostream — standard character input stream (stdin). Use with >> operator.' },
    cerr: { kind: 'object', signature: 'std::ostream std::cerr', documentation: 'iostream — unbuffered standard error stream.' },
    endl: { kind: 'function', signature: 'std::endl', documentation: 'iostream — inserts newline and flushes the output stream.' },
    string: { kind: 'type', signature: 'std::string', documentation: 'string — dynamic growable sequence of char; prefer over C strings in C++.' },
    vector: { kind: 'type', signature: 'std::vector<T>', documentation: 'vector — dynamic contiguous array; O(1) amortized push_back, random access.' },
    map: { kind: 'type', signature: 'std::map<Key, T>', documentation: 'map — ordered associative container (red-black tree); keys sorted, unique.' },
    set: { kind: 'type', signature: 'std::set<T>', documentation: 'set — ordered unique elements (red-black tree).' },
    unordered_map: { kind: 'type', signature: 'std::unordered_map<Key, T>', documentation: 'unordered_map — hash table map; average O(1) lookup, no ordering.' },
    unordered_set: { kind: 'type', signature: 'std::unordered_set<T>', documentation: 'unordered_set — hash set of unique elements.' },
    pair: { kind: 'type', signature: 'std::pair<T1, T2>', documentation: 'utility — heterogeneous two-element tuple; created with std::make_pair or {a, b}.' },
    optional: { kind: 'type', signature: 'std::optional<T>', documentation: 'optional — value that may or may not be present (C++17).' },
    variant: { kind: 'type', signature: 'std::variant<Ts...>', documentation: 'variant — type-safe union holding one of several types (C++17).' },
    unique_ptr: { kind: 'type', signature: 'std::unique_ptr<T>', documentation: 'memory — exclusive-ownership smart pointer; non-copyable, movable.' },
    shared_ptr: { kind: 'type', signature: 'std::shared_ptr<T>', documentation: 'memory — reference-counted shared ownership smart pointer.' },
    weak_ptr: { kind: 'type', signature: 'std::weak_ptr<T>', documentation: 'memory — non-owning reference to shared_ptr-managed object; breaks cycles.' },
    make_unique: { kind: 'function', signature: 'std::make_unique<T>(args...)', documentation: 'memory — creates a std::unique_ptr<T> (C++14).' },
    make_shared: { kind: 'function', signature: 'std::make_shared<T>(args...)', documentation: 'memory — creates a std::shared_ptr<T> with single allocation.' },
    move: { kind: 'function', signature: 'std::move(expr)', documentation: 'utility — casts to rvalue reference to enable move semantics (does not move by itself).' },
    forward: { kind: 'function', signature: 'std::forward<T>(expr)', documentation: 'utility — perfect-forwards an argument preserving value category.' },
    sort: { kind: 'function', signature: 'std::sort(first, last)', documentation: 'algorithm — sorts range in ascending order; O(n log n).' },
    find: { kind: 'function', signature: 'std::find(first, last, value)', documentation: 'algorithm — linear search; returns iterator to first match or end.' },
    size: { kind: 'method', signature: 'size_type container.size() const', documentation: 'Returns the number of elements in a sequence container.' },
    push_back: { kind: 'method', signature: 'void vector.push_back(const T& val)', documentation: 'Appends element to end of vector/string; may reallocate.' },
    emplace_back: { kind: 'method', signature: 'void vector.emplace_back(args...)', documentation: 'Constructs element in place at end of container (C++11).' },
    begin: { kind: 'method', signature: 'iterator container.begin()', documentation: 'Returns iterator to the first element.' },
    end: { kind: 'method', signature: 'iterator container.end()', documentation: 'Returns past-the-end iterator (not dereferenceable).' },
    std: { kind: 'namespace', signature: 'namespace std { ... }', documentation: 'Standard C++ library namespace — vector, string, cout, etc. live here.' },
  };

  function isCLikePath(path) {
    const base = (path.split('/').pop() || '').toLowerCase();
    return /\.(c|h|cpp|cc|cxx|hpp|hh|hxx)$/.test(base);
  }

  function isCppPath(path) {
    const base = (path.split('/').pop() || '').toLowerCase();
    return /\.(cpp|cc|cxx|hpp|hh|hxx)$/.test(base);
  }

  function cDocForWord(word) {
    if (!word) return null;
    return C_DOCS[word] || C_DOCS[word.toLowerCase()] || null;
  }

  function cppDocForWord(word) {
    if (!word) return null;
    return CPP_DOCS[word] || CPP_DOCS[word.toLowerCase()] || null;
  }

  function clikeDocForWord(word, path) {
    if (isCppPath(path)) {
      return cppDocForWord(word) || cDocForWord(word);
    }
    return cDocForWord(word) || cppDocForWord(word);
  }

  function clikeHoverWord(model, position) {
    let wordObj = model.getWordAtPosition(position);
    const line = model.getLineContent(position.lineNumber);
    if (!wordObj?.word) {
      const col = Math.max(0, position.column - 1);
      if (col < line.length && /[\w$]/.test(line[col])) {
        let start = col;
        let end = col + 1;
        while (start > 0 && /[\w$]/.test(line[start - 1])) start -= 1;
        while (end < line.length && /[\w$]/.test(line[end])) end += 1;
        wordObj = { word: line.slice(start, end), startColumn: start + 1, endColumn: end + 1 };
      }
    }
    const word = wordObj?.word || '';
    if (!word) return null;
    const before = line.slice(0, (wordObj?.startColumn || position.column) - 1);
    const stdMatch = before.match(/(?:^|[^:\w])std::(\w*)$/);
    if (stdMatch && word) {
      return {
        word,
        qualified: `std::${word}`,
        range: new monaco.Range(position.lineNumber, wordObj.startColumn, position.lineNumber, wordObj.endColumn),
      };
    }
    return {
      word,
      qualified: word,
      range: new monaco.Range(position.lineNumber, wordObj.startColumn, position.lineNumber, wordObj.endColumn),
    };
  }

  function resolveEditorPath(helpers, model) {
    const tab = helpers.getActivePath?.() || '';
    if (tab) return tab;
    const uriPath = decodeURIComponent(model?.uri?.path || '').replace(/^\//, '');
    return uriPath || '';
  }

  function isCLikeContext(path, model) {
    if (isCLikePath(path)) return true;
    const lang = model?.getLanguageId?.() || '';
    return lang === 'cpp' || lang === 'c';
  }

  function clikeHoverMarkdown(info, markers) {
    if (!info?.name) return '';
    const parts = [];
    if (info.signature) {
      parts.push(`\`\`\`c\n${info.signature}\n\`\`\``);
    } else {
      parts.push(`**${info.name}** · ${info.kind || 'symbol'}`);
    }
    if (info.documentation) {
      parts.push(String(info.documentation).trim());
    } else if (['function', 'method', 'macro', 'typedef', 'struct', 'class'].includes(info.kind)) {
      parts.push('*No documentation available.*');
    }
    if (markers?.length) {
      parts.push('---');
      for (const m of markers) {
        const tone = (typeof monaco !== 'undefined'
          && m.severity === monaco.MarkerSeverity.Error) ? 'Error' : 'Warning';
        parts.push(`**${tone}:** ${String(m.message || '').trim()}`);
      }
    }
    return parts.join('\n\n');
  }

  function clikeHoverResult(info, markers, range) {
    if (!info?.name) return null;
    const md = clikeHoverMarkdown(info, markers);
    if (!md) return null;
    return {
      range: info.range || range,
      contents: [{ value: md, isTrusted: true }],
    };
  }

  function extractCCommentBefore(lines, lineIdx) {
    const out = [];
    for (let i = lineIdx - 1; i >= 0 && i >= lineIdx - 12; i -= 1) {
      const t = lines[i].trim();
      if (!t) {
        if (out.length) break;
        continue;
      }
      if (t.startsWith('*/')) {
        let block = t;
        for (let j = i - 1; j >= 0; j -= 1) {
          block = `${lines[j]}\n${block}`;
          if (lines[j].trim().startsWith('/*')) break;
        }
        out.unshift(block.replace(/^\/\*+\s*|\s*\*+\/$/g, '').replace(/^\s*\*\s?/gm, '').trim());
        break;
      }
      if (t.startsWith('//')) {
        out.unshift(t.replace(/^\/\/+\s?/, '').trim());
        continue;
      }
      break;
    }
    return out.filter(Boolean).join('\n').trim();
  }

  function lookupCLikeLocalSymbol(model, position, path) {
    const hit = clikeHoverWord(model, position);
    if (!hit?.word) return null;
    if (clikeDocForWord(hit.word, path)) return null;
    const word = hit.word;
    const content = model.getValue();
    const lines = content.split('\n');
    const wordRe = word.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    const defRes = [
      new RegExp(`\\b${wordRe}\\s*\\([^)]*\\)\\s*\\{`),
      new RegExp(`\\b(?:struct|union|enum|class)\\s+${wordRe}\\b`),
      new RegExp(`\\btypedef\\b[^;]*\\b${wordRe}\\s*;`),
      new RegExp(`#\\s*define\\s+${wordRe}\\b`),
    ];
    for (let i = 0; i < lines.length; i += 1) {
      const line = lines[i];
      if (!defRes.some((re) => re.test(line))) continue;
      if (line.trim().startsWith('//')) continue;
      let kind = 'function';
      if (/\bstruct\b/.test(line)) kind = 'struct';
      else if (/\bclass\b/.test(line)) kind = 'class';
      else if (/\benum\b/.test(line)) kind = 'enum';
      else if (/\btypedef\b/.test(line)) kind = 'typedef';
      else if (/^\s*#\s*define/.test(line)) kind = 'macro';
      const sig = line.trim().split('{')[0].split(';')[0].trim();
      const doc = extractCCommentBefore(lines, i);
      return {
        name: word,
        kind,
        signature: sig,
        documentation: doc,
        range: hit.range,
      };
    }
    return null;
  }

  function lookupCLikeHover(helpers, model, position) {
    const path = resolveEditorPath(helpers, model);
    if (!isCLikeContext(path, model)) return null;
    const hit = clikeHoverWord(model, position);
    if (!hit) return null;
    const doc = clikeDocForWord(hit.word, path);
    if (!doc) return null;
    const displayName = hit.qualified !== hit.word ? hit.qualified : hit.word;
    return {
      name: displayName,
      kind: doc.kind || 'keyword',
      signature: doc.signature,
      documentation: doc.documentation,
      range: hit.range,
    };
  }

  function lookupSchemaSqlHover(helpers, word) {
    const schema = helpers.getDbSchema?.();
    if (!schema?.tables?.length || !word) return null;
    const lower = word.toLowerCase();
    for (const table of schema.tables) {
      const label = table.schema && table.schema !== 'main'
        ? `${table.schema}.${table.name}`
        : table.name;
      if (table.name.toLowerCase() === lower || label.toLowerCase() === lower) {
        const cols = (table.columns || [])
          .map((c) => `${c.name} ${c.type_name}${c.nullable ? '' : ' NOT NULL'}`)
          .join('\n');
        return {
          name: label,
          kind: 'table',
          signature: `TABLE ${label}`,
          documentation: cols
            ? `Columns:\n${cols}`
            : 'Table with no columns in schema cache.',
        };
      }
      for (const col of table.columns || []) {
        if (col.name.toLowerCase() === lower) {
          return {
            name: col.name,
            kind: 'column',
            signature: `${col.name} ${col.type_name}${col.nullable ? '' : ' NOT NULL'}`,
            documentation: `Column on table ${label}.`,
          };
        }
      }
    }
    return null;
  }

  function lookupSqlHover(helpers, model, position) {
    const wordObj = model.getWordAtPosition(position);
    const word = wordObj?.word || '';
    if (!word) return null;
    const range = wordRangeAt(model, position);
    const schemaHit = lookupSchemaSqlHover(helpers, word);
    if (schemaHit) return { ...schemaHit, range };
    const doc = sqlDocForWord(word);
    if (!doc) return null;
    return {
      name: word.toUpperCase(),
      kind: doc.kind || 'keyword',
      signature: doc.signature,
      documentation: doc.documentation,
      range,
    };
    if (item.documentation) {
      const doc = String(item.documentation);
      suggestion.documentation = doc;
      suggestion.documentationHtml = `<div class="reaper-java-hover-doc">${documentationHtmlFromText(doc)}</div>`;
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

  function hoverDisplayName(info) {
    if (info?.name) return info.name;
    const sig = info?.signature || '';
    if (isJdtlsSourceLine(sig)) return '';
    const typeMatch = sig.match(/\b(?:class|interface|enum|record|@interface)\s+(\w+)/);
    if (typeMatch) return typeMatch[1];
    const methodMatch = sig.match(/(\w+)\s*\(/);
    return methodMatch ? methodMatch[1] : '';
  }

  function hoverInfoHasContent(info) {
    return !!(info && (info.name || info.signature || info.documentation));
  }

  function isJdtlsSourceLine(text) {
    if (window.ReaperAgentMarkdown?.isJdtlsSourceLine) {
      return window.ReaperAgentMarkdown.isJdtlsSourceLine(text);
    }
    const t = String(text || '').trim();
    if (!t) return false;
    if (/^source:/i.test(t) && (/jdt\.ls/i.test(t) || /file:\/\//i.test(t) || /jdt:\/\//i.test(t))) {
      return true;
    }
    return /jdt\.ls-java-project/i.test(t);
  }

  function documentationHtmlFromText(text) {
    const raw = window.ReaperAgentMarkdown?.stripJdtlsSourceAttribution?.(text) ?? String(text || '');
    const doc = raw.trim();
    if (!doc) return '';
    const md = window.ReaperAgentMarkdown;
    if (md?.looksLikeJavadocHtml?.(doc)) {
      return md.renderJavadocHtml(doc);
    }
    if (md?.looksLikeLspMarkdown?.(doc)) {
      return md.renderLspDocumentation(doc);
    }
    if (doc.includes('Parameters:')) {
      return doc.split('\n').map((line) => `<div class="reaper-javadoc-line">${escapeHtml(line)}</div>`).join('');
    }
    return javadocHtmlFromText(doc);
  }

  function lspMarkupContent(text) {
    const value = String(text || '').trim();
    if (!value) return undefined;
    const out = { value };
    if (window.ReaperAgentMarkdown?.looksLikeLspMarkdown?.(value)) {
      out.kind = (typeof monaco !== 'undefined' && monaco.MarkupKind)
        ? monaco.MarkupKind.Markdown
        : 'markdown';
    }
    return out;
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
    if (!hoverInfoHasContent(info)) return '';
    const displayName = hoverDisplayName(info);
    const kindRaw = info.kind && info.kind !== 'symbol' ? info.kind : '';
    const kind = kindRaw ? escapeHtml(kindRaw) : '';
    let html = '<div class="reaper-java-hover">';
    const signature = info.signature && !isJdtlsSourceLine(info.signature) ? info.signature : '';
    if (signature) {
      html += '<div class="reaper-java-hover-head">';
      if (kind) {
        html += `<span class="reaper-java-hover-kind-pill">${kind}</span>`;
      }
      html += `<div class="reaper-java-hover-sig">${escapeHtml(signature)}</div>`;
      html += '</div>';
    } else if (displayName) {
      html += '<div class="reaper-java-hover-title">';
      html += `<span class="reaper-java-hover-name">${escapeHtml(displayName)}</span>`;
      if (kind) {
        html += `<span class="reaper-java-hover-kind-pill">${kind}</span>`;
      }
      html += '</div>';
    }
    if (info.documentation) {
      html += `<div class="reaper-java-hover-doc">${documentationHtmlFromText(info.documentation)}</div>`;
    } else if (info.kind === 'method' || info.kind === 'field') {
      html += '<div class="reaper-java-hover-empty">No documentation available.</div>';
    }
    html += '</div>';
    return html;
  }

  function buildEditorHoverHtml(symInfo, markers) {
    const parts = [];
    if (hoverInfoHasContent(symInfo)) {
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
    const rootCls = markers?.length && !hoverInfoHasContent(symInfo)
      ? 'reaper-editor-hover reaper-editor-hover--problems-only'
      : 'reaper-editor-hover';
    return `<div class="${rootCls}">${parts.join('')}</div>`;
  }

  function itemDocumentationParts(item) {
    if (item?.documentationHtml) {
      return { text: '', html: String(item.documentationHtml).trim() };
    }
    if (!item?.documentation) return { text: '', html: '' };
    const doc = item.documentation;
    if (typeof doc === 'string') {
      const trimmed = doc.trim();
      if (window.ReaperAgentMarkdown?.looksLikeLspMarkdown?.(trimmed)) {
        return { text: '', html: window.ReaperAgentMarkdown.renderLspDocumentation(trimmed) };
      }
      return { text: trimmed, html: '' };
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
    if (!hoverInfoHasContent(info)) return '';
    const displayName = hoverDisplayName(info);
    const parts = [];
    if (info.signature) {
      parts.push(`\`\`\`java\n${info.signature}\n\`\`\``);
    } else if (displayName) {
      parts.push(`**${displayName}**${info.kind && info.kind !== 'symbol' ? ` · ${info.kind}` : ''}`);
    }
    if (info.documentation) {
      const doc = window.ReaperAgentMarkdown?.stripJdtlsSourceAttribution?.(info.documentation)
        ?? info.documentation;
      if (doc) parts.push(formatJavadocMarkdown(doc));
    } else if (info.kind === 'method' || info.kind === 'field') {
      parts.push('*No documentation.*');
    }
    return parts.join('\n\n');
  }

  function renderJavadocBody(el, text) {
    const doc = String(text || '').trim();
    if (!doc) {
      el.innerHTML = '';
      return;
    }
    const md = window.ReaperAgentMarkdown;
    if (md?.looksLikeLspMarkdown?.(doc)) {
      el.innerHTML = md.renderLspDocumentation(doc);
      return;
    }
    el.innerHTML = '';
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
    // java.util / java.util.concurrent
    List: 'java.util.List',
    ArrayList: 'java.util.ArrayList',
    LinkedList: 'java.util.LinkedList',
    Map: 'java.util.Map',
    HashMap: 'java.util.HashMap',
    LinkedHashMap: 'java.util.LinkedHashMap',
    TreeMap: 'java.util.TreeMap',
    Set: 'java.util.Set',
    HashSet: 'java.util.HashSet',
    LinkedHashSet: 'java.util.LinkedHashSet',
    TreeSet: 'java.util.TreeSet',
    Deque: 'java.util.Deque',
    ArrayDeque: 'java.util.ArrayDeque',
    Queue: 'java.util.Queue',
    Optional: 'java.util.Optional',
    Arrays: 'java.util.Arrays',
    Collections: 'java.util.Collections',
    Objects: 'java.util.Objects',
    UUID: 'java.util.UUID',
    Properties: 'java.util.Properties',
    Comparator: 'java.util.Comparator',
    Iterator: 'java.util.Iterator',
    Enumeration: 'java.util.Enumeration',
    Locale: 'java.util.Locale',
    Random: 'java.util.Random',
    Date: 'java.util.Date',
    Calendar: 'java.util.Calendar',
    TimeZone: 'java.util.TimeZone',
    Scanner: 'java.util.Scanner',
    Pattern: 'java.util.regex.Pattern',
    Matcher: 'java.util.regex.Matcher',
    Stream: 'java.util.stream.Stream',
    IntStream: 'java.util.stream.IntStream',
    LongStream: 'java.util.stream.LongStream',
    DoubleStream: 'java.util.stream.DoubleStream',
    Collectors: 'java.util.stream.Collectors',
    CompletableFuture: 'java.util.concurrent.CompletableFuture',
    Future: 'java.util.concurrent.Future',
    ExecutorService: 'java.util.concurrent.ExecutorService',
    Executors: 'java.util.concurrent.Executors',
    ConcurrentHashMap: 'java.util.concurrent.ConcurrentHashMap',
    CountDownLatch: 'java.util.concurrent.CountDownLatch',
    Semaphore: 'java.util.concurrent.Semaphore',
    AtomicInteger: 'java.util.concurrent.atomic.AtomicInteger',
    AtomicLong: 'java.util.concurrent.atomic.AtomicLong',
    AtomicBoolean: 'java.util.concurrent.atomic.AtomicBoolean',
    // java.io / java.nio
    File: 'java.io.File',
    FileReader: 'java.io.FileReader',
    FileWriter: 'java.io.FileWriter',
    FileInputStream: 'java.io.FileInputStream',
    FileOutputStream: 'java.io.FileOutputStream',
    InputStream: 'java.io.InputStream',
    OutputStream: 'java.io.OutputStream',
    Reader: 'java.io.Reader',
    Writer: 'java.io.Writer',
    BufferedReader: 'java.io.BufferedReader',
    BufferedWriter: 'java.io.BufferedWriter',
    PrintWriter: 'java.io.PrintWriter',
    PrintStream: 'java.io.PrintStream',
    IOException: 'java.io.IOException',
    Serializable: 'java.io.Serializable',
    Path: 'java.nio.file.Path',
    Paths: 'java.nio.file.Paths',
    Files: 'java.nio.file.Files',
    StandardCharsets: 'java.nio.charset.StandardCharsets',
    Charset: 'java.nio.charset.Charset',
    ByteBuffer: 'java.nio.ByteBuffer',
    // java.time
    LocalDate: 'java.time.LocalDate',
    LocalTime: 'java.time.LocalTime',
    LocalDateTime: 'java.time.LocalDateTime',
    Instant: 'java.time.Instant',
    Duration: 'java.time.Duration',
    Period: 'java.time.Period',
    ZonedDateTime: 'java.time.ZonedDateTime',
    ZoneId: 'java.time.ZoneId',
    DateTimeFormatter: 'java.time.format.DateTimeFormatter',
    // java.math / java.net
    BigDecimal: 'java.math.BigDecimal',
    BigInteger: 'java.math.BigInteger',
    URI: 'java.net.URI',
    URL: 'java.net.URL',
    HttpURLConnection: 'java.net.HttpURLConnection',
    // java.lang.reflect / functional
    Function: 'java.util.function.Function',
    Consumer: 'java.util.function.Consumer',
    Supplier: 'java.util.function.Supplier',
    Predicate: 'java.util.function.Predicate',
    BiFunction: 'java.util.function.BiFunction',
    UnaryOperator: 'java.util.function.UnaryOperator',
    BinaryOperator: 'java.util.function.BinaryOperator',
    // logging
    Logger: 'org.slf4j.Logger',
    LoggerFactory: 'org.slf4j.LoggerFactory',
    // lombok (common quick-fix targets)
    Slf4j: 'lombok.extern.slf4j.Slf4j',
    Data: 'lombok.Data',
    Getter: 'lombok.Getter',
    Setter: 'lombok.Setter',
    Builder: 'lombok.Builder',
    AllArgsConstructor: 'lombok.AllArgsConstructor',
    NoArgsConstructor: 'lombok.NoArgsConstructor',
    RequiredArgsConstructor: 'lombok.RequiredArgsConstructor',
    EqualsAndHashCode: 'lombok.EqualsAndHashCode',
    ToString: 'lombok.ToString',
    NonNull: 'lombok.NonNull',
    With: 'lombok.With',
    Value: 'lombok.Value',
    // JUnit 5
    Test: 'org.junit.jupiter.api.Test',
    BeforeEach: 'org.junit.jupiter.api.BeforeEach',
    AfterEach: 'org.junit.jupiter.api.AfterEach',
    BeforeAll: 'org.junit.jupiter.api.BeforeAll',
    AfterAll: 'org.junit.jupiter.api.AfterAll',
    DisplayName: 'org.junit.jupiter.api.DisplayName',
    Disabled: 'org.junit.jupiter.api.Disabled',
    ExtendWith: 'org.junit.jupiter.api.extension.ExtendWith',
    ParameterizedTest: 'org.junit.jupiter.params.ParameterizedTest',
    SpringExtension: 'org.springframework.test.context.junit.jupiter.SpringExtension',
    MockitoExtension: 'org.mockito.junit.jupiter.MockitoExtension',
    Assertions: 'org.junit.jupiter.api.Assertions',
    // JUnit 4
    RunWith: 'org.junit.runner.RunWith',
    Before: 'org.junit.Before',
    After: 'org.junit.After',
    BeforeClass: 'org.junit.BeforeClass',
    AfterClass: 'org.junit.AfterClass',
    Assert: 'org.junit.Assert',
    // Mockito
    Mock: 'org.mockito.Mock',
    Spy: 'org.mockito.Spy',
    InjectMocks: 'org.mockito.InjectMocks',
    Captor: 'org.mockito.Captor',
    Mockito: 'org.mockito.Mockito',
    ArgumentMatchers: 'org.mockito.ArgumentMatchers',
    // Spring Boot test
    SpringBootTest: 'org.springframework.boot.test.context.SpringBootTest',
    WebMvcTest: 'org.springframework.boot.test.autoconfigure.web.servlet.WebMvcTest',
    DataJpaTest: 'org.springframework.boot.test.autoconfigure.orm.jpa.DataJpaTest',
    MockBean: 'org.springframework.boot.test.mock.mockito.MockBean',
    SpyBean: 'org.springframework.boot.test.mock.mockito.SpyBean',
    MockMvc: 'org.springframework.test.web.servlet.MockMvc',
    ActiveProfiles: 'org.springframework.test.context.ActiveProfiles',
    // Spring
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
    Page: 'org.springframework.data.domain.Page',
    Pageable: 'org.springframework.data.domain.Pageable',
    PageRequest: 'org.springframework.data.domain.PageRequest',
    Sort: 'org.springframework.data.domain.Sort',
    Bean: 'org.springframework.context.annotation.Bean',
    Configuration: 'org.springframework.context.annotation.Configuration',
    SpringApplication: 'org.springframework.boot.SpringApplication',
    HttpServletRequest: 'jakarta.servlet.http.HttpServletRequest',
    HttpServletResponse: 'jakarta.servlet.http.HttpServletResponse',
    ApplicationContext: 'org.springframework.context.ApplicationContext',
    Environment: 'org.springframework.core.env.Environment',
    Model: 'org.springframework.ui.Model',
    ModelAttribute: 'org.springframework.web.bind.annotation.ModelAttribute',
    RequestHeader: 'org.springframework.web.bind.annotation.RequestHeader',
    CrossOrigin: 'org.springframework.web.bind.annotation.CrossOrigin',
    ExceptionHandler: 'org.springframework.web.bind.annotation.ExceptionHandler',
    ResponseStatus: 'org.springframework.web.bind.annotation.ResponseStatus',
    Valid: 'jakarta.validation.Valid',
    Validated: 'org.springframework.validation.annotation.Validated',
    Transactional: 'org.springframework.transaction.annotation.Transactional',
    Qualifier: 'org.springframework.beans.factory.annotation.Qualifier',
    Primary: 'org.springframework.context.annotation.Primary',
    Lazy: 'org.springframework.context.annotation.Lazy',
    ConfigurationProperties: 'org.springframework.boot.context.properties.ConfigurationProperties',
    EnableScheduling: 'org.springframework.scheduling.annotation.EnableScheduling',
    Scheduled: 'org.springframework.scheduling.annotation.Scheduled',
    MediaType: 'org.springframework.http.MediaType',
    HttpHeaders: 'org.springframework.http.HttpHeaders',
    RestTemplate: 'org.springframework.web.client.RestTemplate',
    WebClient: 'org.springframework.web.reactive.function.client.WebClient',
    JdbcTemplate: 'org.springframework.jdbc.core.JdbcTemplate',
    Entity: 'jakarta.persistence.Entity',
    Table: 'jakarta.persistence.Table',
    Id: 'jakarta.persistence.Id',
    GeneratedValue: 'jakarta.persistence.GeneratedValue',
    JpaRepository: 'org.springframework.data.jpa.repository.JpaRepository',
    CrudRepository: 'org.springframework.data.repository.CrudRepository',
  };

  const JAVA_STATIC_IMPORT_MEMBERS = {
    'org.junit.jupiter.api.Assertions': [
      'assertEquals', 'assertNotEquals', 'assertNull', 'assertNotNull', 'assertTrue', 'assertFalse',
      'assertSame', 'assertNotSame', 'assertThrows', 'assertDoesNotThrow', 'assertAll', 'fail',
    ],
    'org.junit.Assert': [
      'assertEquals', 'assertNotEquals', 'assertNull', 'assertNotNull', 'assertTrue', 'assertFalse', 'fail',
    ],
    'org.mockito.Mockito': [
      'mock', 'spy', 'verify', 'when', 'doReturn', 'doThrow', 'times', 'never', 'atLeast', 'reset',
    ],
    'org.mockito.ArgumentMatchers': [
      'any', 'anyString', 'anyInt', 'anyLong', 'anyBoolean', 'eq', 'isA', 'isNull', 'notNull',
    ],
    'org.apache.commons.lang3.StringUtils': [
      'isBlank', 'isEmpty', 'isNotBlank', 'isNotEmpty', 'trim', 'join', 'split', 'equals',
    ],
  };

  function parseJavaImports(content) {
    const explicit = new Map();
    const wildcards = [];
    for (const line of String(content || '').split(/\r?\n/)) {
      const trimmed = line.trim();
      if (!trimmed.startsWith('import ')) continue;
      const imp = trimmed.slice('import '.length).replace(/;$/, '').trim();
      if (imp.startsWith('static ')) {
        const rest = imp.slice('static '.length).trim();
        if (rest.endsWith('.*')) wildcards.push(rest.slice(0, -2));
        else {
          const dot = rest.lastIndexOf('.');
          if (dot >= 0) explicit.set(rest.slice(dot + 1), rest);
        }
        continue;
      }
      if (imp.endsWith('.*')) wildcards.push(imp.slice(0, -2));
      else {
        const dot = imp.lastIndexOf('.');
        if (dot >= 0) explicit.set(imp.slice(dot + 1), imp);
      }
    }
    return { explicit, wildcards };
  }

  function staticImportCompletionItems(content, prefix) {
    const { explicit, wildcards } = parseJavaImports(content);
    const prefixLower = String(prefix || '').toLowerCase();
    const seen = new Set();
    const items = [];
    const push = (label, detail) => {
      if (!label || seen.has(label)) return;
      if (prefix && !label.toLowerCase().startsWith(prefixLower)) return;
      seen.add(label);
      items.push({ label, kind: 'method', detail });
    };
    for (const [simple, fqcn] of explicit) {
      if (simple && simple[0] === simple[0].toLowerCase()) push(simple, `static ${fqcn}`);
    }
    for (const pkg of wildcards) {
      const members = JAVA_STATIC_IMPORT_MEMBERS[pkg];
      if (!members) continue;
      for (const name of members) push(name, `static ${pkg}.${name}`);
    }
    return items;
  }

  function lombokSyntheticScopeVariables(content, throughLine) {
    const lines = String(content || '').split(/\r?\n/);
    let classLine = -1;
    const limit = Math.min(throughLine, lines.length);
    for (let i = limit - 1; i >= 0; i -= 1) {
      const t = lines[i].trim();
      if (/\b(class|interface|enum|record)\b/.test(t) && !t.includes('(')) {
        classLine = i;
        break;
      }
    }
    if (classLine < 0) return [];
    const classAnns = [];
    for (let i = classLine - 1; i >= Math.max(0, classLine - 12); i -= 1) {
      const t = lines[i].trim();
      if (!t) continue;
      if (t.startsWith('@')) {
        classAnns.push(t.replace(/^@/, '').split('(')[0].split('.').pop());
        continue;
      }
      if (!t.startsWith('//')) break;
    }
    const out = [];
    const seen = new Set();
    const add = (name, typeHint) => {
      if (!name || seen.has(name)) return;
      seen.add(name);
      out.push({ name, typeHint });
    };
    const logging = [
      ['Slf4j', 'log', 'Logger'],
      ['Log', 'log', 'Logger'],
      ['Log4j', 'log', 'Logger'],
      ['Log4j2', 'log', 'Logger'],
      ['CommonsLog', 'log', 'Log'],
    ];
    for (const [ann, field, ty] of logging) {
      if (classAnns.includes(ann)) add(field, ty);
    }
    for (let i = classLine; i < limit; i += 1) {
      const t = lines[i].trim();
      if (!t || t.startsWith('//') || t.startsWith('@')) continue;
      const m = t.match(/^(?:public|private|protected|static|final|\s)*([\w<>,\[\].?]+)\s+(\w+)\s*[;=]/);
      if (!m) continue;
      const ty = m[1];
      const name = m[2];
      if (/@Mock\b/.test(t) || /@Spy\b/.test(t) || /@InjectMocks\b/.test(t) || /@Captor\b/.test(t)) {
        add(name, ty);
      }
    }
    return out;
  }

  function lombokSyntheticMembers(content, qualifier, memberPrefix) {
    if (qualifier !== 'this' && qualifier !== 'super') return [];
    const lines = String(content || '').split(/\r?\n/);
    let classLine = -1;
    for (let i = 0; i < lines.length; i += 1) {
      const t = lines[i].trim();
      if (/\b(class|interface|enum|record)\b/.test(t) && !t.includes('(')) {
        classLine = i;
        break;
      }
    }
    if (classLine < 0) return [];
    const classAnns = [];
    for (let i = classLine - 1; i >= Math.max(0, classLine - 12); i -= 1) {
      const t = lines[i].trim();
      if (!t) continue;
      if (t.startsWith('@')) {
        classAnns.push(t.replace(/^@/, '').split('(')[0].split('.').pop());
        continue;
      }
      if (!t.startsWith('//')) break;
    }
    const hasData = classAnns.includes('Data');
    const hasGetter = hasData || classAnns.includes('Getter');
    const hasSetter = hasData || classAnns.includes('Setter');
    const prefixLower = String(memberPrefix || '').toLowerCase();
    const items = [];
    const seen = new Set();
    const cap = (s) => s ? s[0].toUpperCase() + s.slice(1) : s;
    for (let i = classLine; i < lines.length; i += 1) {
      const t = lines[i].trim();
      if (!t || t.startsWith('//') || t.startsWith('@') || t.includes('(')) continue;
      const m = t.match(/^(?:public|private|protected|static|final|\s)*([\w<>,\[\].?]+)\s+(\w+)\s*;/);
      if (!m) continue;
      const ty = m[1];
      const field = m[2];
      if (field.startsWith('$')) continue;
      if (hasGetter) {
        const getter = (ty === 'boolean' || ty === 'Boolean') ? `is${cap(field)}` : `get${cap(field)}`;
        if ((!memberPrefix || getter.toLowerCase().startsWith(prefixLower)) && seen.add(getter)) {
          items.push({ label: getter, kind: 'method', detail: `${ty} (lombok)` });
        }
      }
      if (hasSetter) {
        const setter = `set${cap(field)}`;
        if ((!memberPrefix || setter.toLowerCase().startsWith(prefixLower)) && seen.add(setter)) {
          items.push({ label: setter, kind: 'method', detail: 'void (lombok)' });
        }
      }
    }
    for (const { name, typeHint } of lombokSyntheticScopeVariables(content, lines.length)) {
      if ((!memberPrefix || name.toLowerCase().startsWith(prefixLower)) && seen.add(name)) {
        items.push({ label: name, kind: 'field', detail: `${typeHint} (lombok)` });
      }
    }
    return items;
  }

  const JAVA_LANG_NO_IMPORT = new Set([
    'String', 'Object', 'Integer', 'Long', 'Boolean', 'Double', 'Float', 'Byte', 'Short',
    'Character', 'Void', 'Class', 'System', 'Math', 'Enum', 'RuntimeException', 'Exception',
    'Error', 'Throwable', 'Override', 'Deprecated', 'SuppressWarnings', 'FunctionalInterface',
    'AutoCloseable', 'Cloneable', 'Iterable', 'Comparable', 'Runnable', 'Thread',
    'StringBuilder', 'StringBuffer', 'Process', 'ProcessBuilder', 'Record',
  ]);

  function localJavaImportFqcn(symbol) {
    const name = String(symbol || '').trim();
    if (!name || JAVA_LANG_NO_IMPORT.has(name)) return null;
    return COMMON_JAVA_IMPORTS[name] || null;
  }

  function extractJavacClassSymbol(message) {
    const msg = String(message || '');
    const idx = msg.indexOf('symbol:');
    if (idx < 0) return '';
    const rest = msg.slice(idx + 'symbol:'.length).trim();
    const parts = rest.split(/\s+/);
    if (!parts.length) return '';
    if (parts[0] === 'class' || parts[0] === 'interface' || parts[0] === 'variable' || parts[0] === 'method') {
      return parts[1] || '';
    }
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
    const nextTrimmed = insertLine <= lineCount ? model.getLineContent(insertLine).trim() : '';
    const blankBeforeBody = nextTrimmed.startsWith('@')
      || /^(public |private |protected |class |interface |enum |record )/.test(nextTrimmed);
    const suffix = blankBeforeBody ? '\n\n' : '\n';
    const text = insertLine > 1 ? `\n${importLine}${suffix}` : `${importLine}${suffix}`;
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
    if (/\.(c|h)$/i.test(base)) return 'cpp';
    if (/\.(cpp|cc|cxx|hpp|hh|hxx)$/i.test(base)) return 'cpp';
    if (/\.(md|mdx|markdown)$/i.test(base)) return 'markdown';
    if (/^(makefile|gnumakefile)$/i.test(base) || base.endsWith('.mk') || base.startsWith('makefile.')) {
      return 'makefile';
    }
    return fromPath || 'plaintext';
  }

  function ensureModelLanguageForPath(path, model) {
    applyEditorLanguage(path, model);
    return langForPath(path || '');
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
    List: ['add', 'get', 'size', 'isEmpty', 'clear', 'remove', 'iterator', 'contains', 'addAll', 'removeAll', 'stream', 'parallelStream', 'spliterator'],
    ArrayList: ['add', 'get', 'size', 'isEmpty', 'clear', 'remove', 'iterator', 'contains', 'stream', 'parallelStream'],
    Map: ['get', 'put', 'size', 'isEmpty', 'clear', 'remove', 'containsKey', 'containsValue', 'keySet', 'values', 'entrySet'],
    HashMap: ['get', 'put', 'size', 'isEmpty', 'clear', 'remove', 'containsKey'],
    Set: ['add', 'size', 'isEmpty', 'clear', 'remove', 'contains', 'iterator'],
    PrintStream: ['print', 'println', 'printf', 'format', 'write', 'flush', 'close'],
    Scanner: ['next', 'nextLine', 'nextInt', 'hasNext', 'close'],
    Stream: ['map', 'filter', 'flatMap', 'collect', 'forEach', 'reduce', 'findFirst', 'findAny', 'count', 'distinct', 'sorted', 'limit', 'skip', 'toList'],
  };

  function jdkExtraMembersForType(simple, javaLevel) {
    const extras = [];
    const collections = new Set([
      'List', 'ArrayList', 'LinkedList', 'Set', 'HashSet', 'LinkedHashSet',
      'Collection', 'Deque', 'ArrayDeque', 'Iterable',
    ]);
    if (collections.has(simple) && javaLevel >= 8) {
      for (const m of ['stream', 'parallelStream', 'spliterator']) {
        if (!extras.includes(m)) extras.push(m);
      }
    }
    if (javaLevel >= 21 && ['List', 'ArrayList', 'LinkedList', 'Deque'].includes(simple)) {
      for (const m of ['reversed', 'getFirst', 'getLast', 'addFirst', 'addLast', 'removeFirst', 'removeLast']) {
        if (!extras.includes(m)) extras.push(m);
      }
    }
    return extras;
  }

  function jdkBuiltinMembersForType(typeName, memberPrefix, javaLevel = 21) {
    const simple = simplifyJavaTypeName(typeName);
    const baseMembers = JDK_TYPE_MEMBERS[simple] || [];
    const members = [...baseMembers];
    for (const m of jdkExtraMembersForType(simple, javaLevel)) {
      if (!members.includes(m)) members.push(m);
    }
    if (!members.length) return [];
    const prefixLower = (memberPrefix || '').toLowerCase();
    return members
      .filter((m) => !memberPrefix || m.toLowerCase().startsWith(prefixLower))
      .map((m) => ({ label: m, kind: 'method', detail: `${simple}.${m}` }));
  }

  function localMemberCompletions(content, qualifier, memberPrefix, path, model, javaLevel = 21) {
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
      for (const m of lombokSyntheticMembers(content, qualifier, memberPrefix)) {
        if (seen.add(m.label)) items.push(m);
      }
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
        for (const m of jdkBuiltinMembersForType(type, memberPrefix, javaLevel)) {
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

  function lineAfterCursor(content, lineNumber, column) {
    const lines = String(content || '').split(/\r?\n/);
    const line = lines[lineNumber - 1] || '';
    return line.slice(Math.max(0, (column || 1) - 1));
  }

  function hasCloseBraceAfterCursor(content, lineNumber, column) {
    return /^\s*\}/.test(lineAfterCursor(content, lineNumber, column));
  }

  function isRedundantCloseBraceGhost(linePrefix, ghostText, content, lineNumber, column) {
    const ghost = String(ghostText ?? '').trim();
    if (ghost !== '}') return false;
    if (!String(linePrefix || '').trimEnd().endsWith('{')) return false;
    if (content && lineNumber && column) {
      return hasCloseBraceAfterCursor(content, lineNumber, column);
    }
    return true;
  }

  function childIndent(indent) {
    const base = String(indent || '');
    if (base.includes('\t')) return `${base}\t`;
    return `${base}    `;
  }

  function matchExpandableEmptyBraceLine(lineText) {
    const m = String(lineText || '').match(/^(\s*)(.+?\S)\s*\{\s*\}(\s*)$/);
    if (!m) return null;
    const head = m[2].trimEnd();
    if (/^\s*(if|else\b|for|while|switch|catch|try|do|synchronized)\b/.test(head)) return null;
    if (/\b(class|interface|enum|record|struct|trait|namespace)\s+\w+/.test(head)) return m;
    if (/\b(function|func|def|fn)\s+\w+/.test(head)) return m;
    if (/\)\s*$/.test(head)) return m;
    return null;
  }

  function maybeExpandEmptyBraceBlock(ed) {
    if (ed._reaperExpandingBrace) return;
    const model = ed.getModel();
    const pos = ed.getPosition();
    if (!model || !pos) return;
    const lineText = model.getLineContent(pos.lineNumber);
    const m = matchExpandableEmptyBraceLine(lineText);
    if (!m) return;
    const baseIndent = m[1];
    const head = m[2].trimEnd();
    const inner = childIndent(baseIndent);
    const newText = `${baseIndent}${head} {\n${inner}\n${baseIndent}}`;
    ed._reaperExpandingBrace = true;
    try {
      ed.executeEdits('reaper-expand-brace', [{
        range: new monaco.Range(pos.lineNumber, 1, pos.lineNumber, lineText.length + 1),
        text: newText,
      }]);
      ed.setPosition({ lineNumber: pos.lineNumber + 1, column: inner.length + 1 });
    } finally {
      ed._reaperExpandingBrace = false;
    }
  }

  function syntaxInlineSuffix(path, linePrefix, content, lineNumber, column) {
    const trimmed = String(linePrefix || '').trimEnd();
    if (!trimmed) return '';
    const lang = langForPath(path);

    if (/\w\s*\($/.test(trimmed) && !trimmed.endsWith('()')) return ')';
    if (trimmed.endsWith('{') && !trimmed.endsWith('${')) {
      if (content && lineNumber && column && hasCloseBraceAfterCursor(content, lineNumber, column)) {
        return '';
      }
      return '}';
    }
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
    // Do not ghost a space after YAML `key:` — sanitize would strip it, and Space-to-accept
    // would then swallow the key without inserting anything.
    return '';
  }

  /** Prefer AI over local templates for statements and block bodies (all languages). */
  function isInsideControlParen(linePrefix) {
    const trimmed = String(linePrefix || '').trimEnd();
    if (dotQualifierFromLinePrefix(trimmed)) return false;
    if (/\b(for|if|while|switch|catch|foreach|elif|unless|until)\s*\([^)]*$/i.test(trimmed)) {
      return true;
    }
    if (/\b(for|if|while|switch)\s+\w/i.test(trimmed)) return true;
    return false;
  }

  function shouldOfferJavaScopeCompletions(content, lineNumber, linePrefix) {
    if (/^\s*import\b/.test(String(linePrefix || ''))) return false;
    const trimmed = String(linePrefix || '').trimEnd();
    if (/\bnew\s+[A-Z]\w*$/.test(trimmed)) return false;
    if (/\b(?:extends|implements|throws)\s+\w/i.test(trimmed)) return false;
    const lines = String(content || '').split(/\r?\n/);
    let depth = 0;
    for (let i = 0; i < Math.min(lineNumber, lines.length); i += 1) {
      for (const ch of lines[i]) {
        if (ch === '{') depth += 1;
        else if (ch === '}') depth = Math.max(0, depth - 1);
      }
    }
    return depth > 0;
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
    for (const { name, typeHint } of lombokSyntheticScopeVariables(content, throughLine)) {
      add(name, typeHint);
    }
    return out;
  }

  function shouldPreferAiStatementInline(path, linePrefix, content, lineNumber) {
    if (dotQualifierFromLinePrefix(linePrefix)) return false;
    if (isReturnContext(linePrefix)) return false;
    if (isOperandContext(linePrefix)) return false;
    if (isCallArgContext(linePrefix, path)) return false;
    const trimmed = linePrefix.trimEnd();
    if (hasCompleteControlKeyword(trimmed)) return true;
    if (isInsideControlParen(linePrefix)) return true;
    if (isWhitespaceOnlyLine(linePrefix)) return true;
    const token = extractInlinePartialToken(linePrefix);
    if (token.length >= 2 && isControlKeywordPrefix(token)) return true;
    return false;
  }

  /** AI inline when local vars/types/keywords are not handling the cursor (all languages). */
  function shouldRouteInlineToAi(path, linePrefix, content, lineNumber, localGhost = '', aiEnabled = true) {
    if (!aiEnabled) return false;
    if (dotQualifierFromLinePrefix(linePrefix)) return false;
    if (isImportTypingLine(path, linePrefix)) return false;
    if (shouldPauseInlineAtCursor(path, linePrefix, content, lineNumber)) return false;
    // Finished modifiers — never AI-hijack the next word. Declarations may still show AI;
    // Space/Tab policy keeps typing free.
    {
      const modToken = extractInlinePartialToken(linePrefix);
      if (isModifierLeadInFreeTyping(path, linePrefix, modToken)) return false;
      if (shouldPreferModifierKeywordInline(path, linePrefix, modToken)
        && !isModifierLeadInFreeTyping(path, linePrefix, modToken)) {
        // Partial modifier (`priv`) — keep local keyword ghost, skip AI.
        return false;
      }
    }
    if (localGhost) return false;
    if (shouldPreferAiStatementInline(path, linePrefix, content, lineNumber)) return true;

    if (isOperandContext(linePrefix)) {
      if (operandInlineSuffix(content, path, lineNumber, linePrefix)) return false;
      if (supportsWorkspaceIndexInline(path)) return false;
    }
    if (isReturnContext(linePrefix)) {
      if (returnInlineSuffix(content, path, lineNumber, linePrefix)) return false;
      if (supportsWorkspaceIndexInline(path)) return false;
    }
    if (isCallArgContext(linePrefix, path)) {
      if (callArgInlineSuffix(content, path, lineNumber, linePrefix)) return false;
      if (supportsWorkspaceIndexInline(path)) return false;
    }

    const token = extractInlinePartialToken(linePrefix);
    if (token) {
      if (supportsWorkspaceIndexInline(path) && shouldFetchIndexCompletions(linePrefix, token, path)) {
        return false;
      }
      if (shouldPreferJavaTypeInline(path, linePrefix, token)) return false;
      const trimmed = linePrefix.trimEnd();
      if (
        (token.length === 1 && isKeywordPrefixTyping(path, token))
        || (
          shouldPreferControlKeywordInline(path, linePrefix, token)
          && !hasCompleteControlKeyword(trimmed)
        )
      ) {
        return false;
      }
      if (scopeIdentifierInlineSuffix(content, path, lineNumber, token)) return false;
    }
    return true;
  }

  const JAVA_PRIMITIVE_TYPES = new Set([
    'int', 'long', 'boolean', 'char', 'byte', 'short', 'float', 'double', 'void', 'var',
  ]);

  /** Common JDK / framework types for instant inline + suggest (before async index returns). */
  const JAVA_SYNC_TYPE_NAMES = [
    'String', 'System', 'File', 'Files', 'Path', 'Paths', 'List', 'Map', 'Set', 'Optional',
    'Integer', 'Long', 'Boolean', 'Double', 'Float', 'Byte', 'Short', 'Character', 'Object',
    'Class', 'Thread', 'Exception', 'RuntimeException', 'IOException', 'InputStream',
    'OutputStream', 'BufferedReader', 'PrintWriter', 'Scanner', 'Arrays', 'Collections',
    'Objects', 'Math', 'UUID', 'BigDecimal', 'BigInteger', 'ArrayList', 'HashMap', 'HashSet',
    'LinkedList', 'StringBuilder', 'StringBuffer', 'Pattern', 'Matcher', 'Stream',
    'Collectors', 'LocalDate', 'LocalDateTime', 'Instant', 'Duration', 'URI', 'URL',
    'FileInputStream', 'FileOutputStream', 'SpringApplication', 'Autowired', 'Component',
    'Service', 'Repository', 'Controller', 'RestController', 'RequestMapping', 'Bean',
    'Configuration', 'Value', 'Logger', 'LoggerFactory', 'HttpServletRequest', 'ResponseEntity',
    'HttpStatus', 'Test', 'Mock', 'Mockito', 'SpringBootTest', 'MockBean', 'MockMvc',
    'Data', 'Slf4j', 'Builder', 'Entity', 'Table', 'Transactional', 'Valid', 'JdbcTemplate',
    'WebClient', 'CompletableFuture', 'Future', 'Function', 'Consumer', 'Supplier',
    'Predicate', 'Runnable', 'Comparable', 'Iterator', 'Iterable', 'Enum', 'Record',
  ];

  function isJavaLikePath(path) {
    const lang = langForPath(path);
    return lang === 'java' || lang === 'kotlin' || lang === 'kts' || lang === 'groovy';
  }

  function javaSyncTypeInlineSuffix(token) {
    if (!token) return '';
    const lower = token.toLowerCase();
    for (const name of JAVA_SYNC_TYPE_NAMES) {
      if (name.toLowerCase().startsWith(lower) && name.length > token.length) {
        return name.slice(token.length);
      }
    }
    return '';
  }

  /** PascalCase or explicit type position (`new Foo`, `String name`) — not statement keywords. */
  function shouldPreferJavaTypeInline(path, linePrefix, token) {
    if (!token || !isJavaLikePath(path)) return false;
    if (/^[A-Z]/.test(token)) return true;
    const trimmed = String(linePrefix || '').trimEnd();
    if (trimmed.endsWith('new ')
      || trimmed.endsWith('extends ')
      || trimmed.endsWith('implements ')
      || trimmed.endsWith('throws ')
      || trimmed.endsWith('catch (')
      || trimmed.endsWith('<')) {
      return true;
    }
    if (isDeclarationTyping(path, linePrefix)) return true;
    return false;
  }

  /** Lowercase f/i/w at statement start → for/if/while, not File/Integer/WeakReference. */
  function shouldPreferControlKeywordInline(path, linePrefix, token) {
    if (!token || !isCodeStatementLanguage(path)) return false;
    if (shouldPreferJavaTypeInline(path, linePrefix, token)) return false;
    return isControlKeywordPrefix(token);
  }

  /**
   * Access / member modifiers by language — never complete into types that share a
   * prefix (PrivateKeyEntry, InternalError, …) while typing or right after Space.
   */
  const JVM_MODIFIER_KEYWORDS = [
    'public', 'private', 'protected', 'static', 'final', 'abstract',
    'synchronized', 'volatile', 'transient', 'native', 'default', 'sealed',
  ];
  const MODIFIER_KEYWORDS_BY_LANG = {
    java: JVM_MODIFIER_KEYWORDS,
    kotlin: [
      ...JVM_MODIFIER_KEYWORDS,
      'internal', 'open', 'override', 'suspend', 'lateinit', 'crossinline', 'noinline',
    ],
    kts: [
      ...JVM_MODIFIER_KEYWORDS,
      'internal', 'open', 'override', 'suspend', 'lateinit', 'crossinline', 'noinline',
    ],
    groovy: JVM_MODIFIER_KEYWORDS,
    csharp: [
      'public', 'private', 'protected', 'internal', 'static', 'readonly', 'volatile',
      'const', 'abstract', 'sealed', 'override', 'virtual', 'async', 'partial', 'extern',
      'unsafe', 'required',
    ],
    cpp: [
      'public', 'private', 'protected', 'static', 'const', 'constexpr', 'mutable',
      'virtual', 'inline', 'explicit', 'friend', 'volatile', 'extern', 'override', 'final',
    ],
    c: [
      'static', 'const', 'volatile', 'extern', 'inline', 'register', 'signed', 'unsigned',
      'restrict',
    ],
    swift: [
      'public', 'private', 'internal', 'fileprivate', 'open', 'static', 'final', 'lazy',
      'weak', 'unowned', 'override', 'mutating', 'nonmutating', 'dynamic', 'optional',
    ],
    php: [
      'public', 'private', 'protected', 'static', 'final', 'abstract', 'readonly',
    ],
    typescript: [
      'public', 'private', 'protected', 'static', 'readonly', 'abstract', 'async',
      'override', 'declare',
    ],
    javascript: ['static', 'async'],
    rust: ['pub', 'const', 'static', 'async', 'unsafe', 'mut', 'extern'],
    dart: ['static', 'final', 'const', 'abstract', 'async', 'covariant', 'late'],
  };

  const modifierRegexCache = new Map();

  function modifierKeywordsForPath(path) {
    const lang = langForPath(path || '');
    return MODIFIER_KEYWORDS_BY_LANG[lang] || null;
  }

  function modifierRegexesForPath(path) {
    const keywords = modifierKeywordsForPath(path);
    if (!keywords?.length) return null;
    const lang = langForPath(path || '');
    let cached = modifierRegexCache.get(lang);
    if (cached) return cached;
    const alt = keywords
      .map((kw) => kw.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'))
      .join('|');
    cached = {
      keywords,
      lead: new RegExp(`^(?:(?:${alt})(?:\\s+|$))+$`),
      partial: new RegExp(`^(?:(?:${alt})\\s+)*[a-z][a-z]*$`),
      afterFinished: new RegExp(`^(?:(?:${alt})\\s+)`),
    };
    modifierRegexCache.set(lang, cached);
    return cached;
  }

  function isModifierKeywordPrefix(token, path) {
    const keywords = modifierKeywordsForPath(path);
    if (!keywords || !token || /[A-Z]/.test(token)) return false;
    const lower = token.toLowerCase();
    return keywords.some((kw) => kw.startsWith(lower));
  }

  /** Line is only finished modifiers (`private` / `private ` / `public static `). */
  function isCompleteModifierSequence(linePrefix, path) {
    const rx = modifierRegexesForPath(path);
    if (!rx) return false;
    const trimmed = String(linePrefix || '').trimStart();
    return !!trimmed && rx.lead.test(trimmed);
  }

  function modifierKeywordInlineSuffix(token, path) {
    const keywords = modifierKeywordsForPath(path);
    if (!keywords || !token || /[A-Z]/.test(token)) return '';
    const lower = token.toLowerCase();
    let best = null;
    for (const kw of keywords) {
      if (kw.startsWith(lower) && kw.length > token.length) {
        const suffix = kw.slice(token.length);
        if (!best || suffix.length < best.length) best = suffix;
      }
    }
    return best || '';
  }

  /**
   * Typing `private` / `public static` at a member start — prefer the keyword, not
   * classpath types that share a prefix (PrivateKeyEntry, PublicKey, …).
   * Also true right after a finished modifier so Space/next words are not hijacked.
   */
  function shouldPreferModifierKeywordInline(path, linePrefix, token) {
    const rx = modifierRegexesForPath(path);
    if (!rx) return false;
    if (isCompleteModifierSequence(linePrefix, path)) return true;
    if (!token || /[A-Z]/.test(token)) return false;
    if (!isModifierKeywordPrefix(token, path)) return false;
    const trimmed = String(linePrefix || '').trimStart();
    return rx.partial.test(trimmed);
  }

  /**
   * Finished a modifier (`private` / `private `) — free typing for the next word.
   * No suggest popup / index / AI ghosts (those were swallowing Space and next keys).
   */
  function isModifierLeadInFreeTyping(path, linePrefix, token) {
    if (!shouldPreferModifierKeywordInline(path, linePrefix, token)) return false;
    if (isCompleteModifierSequence(linePrefix, path)) return true;
    if (!token) return true;
    return !modifierKeywordInlineSuffix(token, path);
  }

  function rankIndexItemsForInline(items) {
    const rank = (item) => {
      const k = String(item?.kind || '').toLowerCase();
      if (k === 'class' || k === 'interface' || k === 'type' || k === 'enum') return 0;
      if (k === 'variable' || k === 'field' || k === 'property') return 1;
      if (k === 'method' || k === 'function') return 2;
      if (k === 'keyword') return 4;
      return 3;
    };
    return [...items].sort((a, b) => {
      const ra = rank(a);
      const rb = rank(b);
      if (ra !== rb) return ra - rb;
      const la = String(a?.label || a?.insert || '');
      const lb = String(b?.label || b?.insert || '');
      return la.length - lb.length || la.localeCompare(lb);
    });
  }

  /** True when the user finished a type and is typing the variable name (`String greeting`). */
  function hasDeclarationNameSlot(trimmed, typePart, namePart) {
    if (namePart) return true;
    const idx = trimmed.indexOf(typePart);
    if (idx < 0) return false;
    return trimmed.slice(idx + typePart.length).startsWith(' ');
  }

  function isJavaLikeTypeToken(typePart) {
    return JAVA_PRIMITIVE_TYPES.has(typePart)
      || /^[A-Z]/.test(typePart)
      || typePart.includes('<')
      || typePart.includes('[');
  }

  /**
   * `var` / `int` / `String name` / `String name ` — declaration lead-in where Space must
   * type normally (no index/AI ghost accept turning Space into `.` or eating spaces).
   */
  function isDeclarativeLeadInFreeTyping(path, linePrefix) {
    if (!isJavaLikePath(path)) return false;
    if (isDeclarationTyping(path, linePrefix)) return true;
    const raw = String(linePrefix || '');
    const trimmed = raw.trimStart();
    if (!trimmed || trimmed.includes('=') || trimmed.includes(';')) return false;
    if (/\.\w/.test(trimmed)) return false;
    const mod = '(?:(?:public|private|protected|static|final|volatile|transient|abstract|native|synchronized|default|sealed)\\s+)*';
    // Complete primitive/`var` with no trailing chars yet — next key is Space or name.
    const bareType = trimmed.match(new RegExp(`^${mod}([A-Za-z_][\\w]*)$`));
    if (bareType) {
      const typePart = bareType[1];
      if (/^(if|for|while|return|new|throw|catch|switch|try|else|class|interface|enum|record)\b/.test(typePart)) {
        return false;
      }
      return JAVA_PRIMITIVE_TYPES.has(typePart);
    }
    return false;
  }

  /** Suppress inline ghost only while typing `Type varName`, not partial statement identifiers (`S`). */
  function isDeclarationTyping(path, linePrefix) {
    const lang = langForPath(path);
    const trimmed = linePrefix.trimStart();
    if (!trimmed || trimmed.includes('=') || trimmed.includes(';')) return false;
    if (/\.\w/.test(trimmed)) return false;

    if (lang === 'java' || lang === 'kotlin' || lang === 'kts' || lang === 'groovy') {
      const mod = '(?:(?:public|private|protected|static|final|volatile|transient)\\s+)*';
      // `Type name` or `Type name   ` (space before `=` must stay free).
      let m = trimmed.match(new RegExp(`^${mod}([\\w][\\w.<>\\[\\]]*)\\s+([\\w$]+)\\s*$`));
      if (m) {
        const typePart = m[1];
        if (/^(if|for|while|return|new|throw|catch|switch|try)\b/.test(typePart)) return false;
        return isJavaLikeTypeToken(typePart);
      }
      // `Type ` / `var ` — name slot open.
      m = trimmed.match(new RegExp(`^${mod}([\\w][\\w.<>\\[\\]]*)\\s+$`));
      if (m) {
        const typePart = m[1];
        if (/^(if|for|while|return|new|throw|catch|switch|try)\b/.test(typePart)) return false;
        return isJavaLikeTypeToken(typePart);
      }
      return false;
    }

    if (lang === 'typescript' || lang === 'javascript') {
      const typedVar = trimmed.match(
        /^(?:export\s+)?(?:const|let|var)\s+\w+\s*:\s*([\w$][\w$.<>\[\]|&]*)\s*([\w$]*)$/,
      );
      if (typedVar) {
        return hasDeclarationNameSlot(trimmed, typedVar[1], typedVar[2] || '');
      }
      const typeDecl = trimmed.match(/^(?:export\s+)?(?:type|interface)\s+([\w$]+)\s+([\w$]*)$/);
      if (typeDecl) {
        return hasDeclarationNameSlot(trimmed, typeDecl[1], typeDecl[2] || '');
      }
      return false;
    }

    if (lang === 'rust') {
      const typed = trimmed.match(/^(?:pub\s+)?(?:let\s+mut\s+|let\s+|const\s+)\w+\s*:\s*([\w:]+)\s*([\w$]*)$/);
      if (typed) {
        return hasDeclarationNameSlot(trimmed, typed[1], typed[2] || '');
      }
      return false;
    }

    if (lang === 'csharp') {
      const declRe = /^(?:(?:public|private|protected|internal|static|readonly|volatile|const)\s+)*([\w][\w.<>\[\]]*)\s+([\w$]*)$/;
      const m = trimmed.match(declRe);
      if (!m) return false;
      const typePart = m[1];
      const namePart = m[2] || '';
      if (!isJavaLikeTypeToken(typePart)) return false;
      return hasDeclarationNameSlot(trimmed, typePart, namePart);
    }

    if (lang === 'cpp' || lang === 'c') {
      const declRe = /^(?:(?:static|const|volatile|unsigned|signed|long|short)\s+)*([\w][\w:<>\[\]*&]*)\s+([\w$]*)$/;
      const m = trimmed.match(declRe);
      if (!m) return false;
      const typePart = m[1];
      const namePart = m[2] || '';
      if (/^(if|for|while|return|switch|case|struct|enum|union|typedef)\b/.test(typePart)) return false;
      return hasDeclarationNameSlot(trimmed, typePart, namePart);
    }

    if (lang === 'swift') {
      const declRe = /^(?:(?:public|private|internal|fileprivate|open|static|final|lazy|weak|unowned)\s+)*([\w][\w.<>\[\]]*)\s+([\w$]*)$/;
      const m = trimmed.match(declRe);
      if (!m) return false;
      return hasDeclarationNameSlot(trimmed, m[1], m[2] || '');
    }

    if (lang === 'go') {
      const typed = trimmed.match(/^var\s+\w+\s+([\w[\].*]+)\s*([\w$]*)$/);
      if (typed) {
        return hasDeclarationNameSlot(trimmed, typed[1], typed[2] || '');
      }
      return false;
    }

    return false;
  }

  /** @deprecated use isDeclarationTyping */
  function isJavaDeclarationTyping(path, linePrefix) {
    return isDeclarationTyping(path, linePrefix);
  }

  /** Ghost must not start with an identifier char when the line already ends on one — unless completing that token. */
  function inlineGhostSafeAfter(linePrefix, ghostText) {
    const ghost = String(ghostText ?? '');
    if (!ghost) return false;
    const trimmed = String(linePrefix || '').trimEnd();
    if (!trimmed) return true;
    const last = trimmed[trimmed.length - 1];
    if (!/[A-Za-z0-9_$]/.test(last)) return true;
    const token = extractInlinePartialToken(linePrefix);
    if (token && /[A-Za-z0-9_$]/.test(ghost[0])) return true;
    return !/[A-Za-z0-9_$]/.test(ghost[0]);
  }

  function shouldSuppressInlineGhost(path, linePrefix, ghostText, content, lineNumber, column) {
    if (shouldPauseInlineAtCursor(path, linePrefix, content, lineNumber, column)) return true;
    // Show ghosts during declarations — Space never auto-accepts them.
    {
      const tok = extractInlinePartialToken(linePrefix) || '';
      if (isModifierLeadInFreeTyping(path, linePrefix, tok)) return true;
    }
    // Punctuation ghosts (`.` etc.) must never appear — Space used to accept them.
    {
      const ghost = String(ghostText ?? '').split('\n')[0];
      if (ghost && (ghost.startsWith('.') || /^[^\w$\s({\["']/.test(ghost))) return true;
    }
    if (!inlineGhostSafeAfter(linePrefix, ghostText)) return true;
    if (isRedundantCloseBraceGhost(linePrefix, ghostText, content, lineNumber, column)) return true;
    if (content && lineNumber && column && ghostText) {
      const after = lineAfterCursor(content, lineNumber, column);
      const ghost = String(ghostText).split('\n')[0];
      if (ghost && after.startsWith(ghost)) return true;
    }
    if (content && lineNumber && isWhitespaceOnlyLine(linePrefix) && ghostText) {
      const neighbor = emptyLineNeighborBelow(content, lineNumber, linePrefix);
      if (neighbor) {
        const proposed = `${lineIndent(linePrefix)}${String(ghostText).split('\n')[0]}`.trim();
        if (proposed === neighbor.text.trim()) return true;
      }
    }
    return false;
  }

  function buildInlineCacheKey(repo, path, lineNumber, column, linePrefix) {
    if (isWhitespaceOnlyLine(linePrefix)) {
      return `${repo}:${path}:${lineNumber}:${linePrefix}`;
    }
    return `${repo}:${path}:${lineNumber}:${column}:${linePrefix}`;
  }

  /** Keep ghost text while typed chars match the pending suggestion. */
  function inlineGhostFromCache(cache, repo, path, lineNumber, column, linePrefix, content) {
    if (!cache?.text) return '';
    const key = buildInlineCacheKey(repo, path, lineNumber, column, linePrefix);
    let ghost = '';
    if (cache.key === key) {
      ghost = cache.text;
    } else if (
      isWhitespaceOnlyLine(linePrefix)
      && cache.repo === repo
      && cache.path === path
      && cache.lineNumber === lineNumber
      && (cache.linePrefix ?? '') === linePrefix
    ) {
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
    if (shouldSuppressInlineGhost(path, linePrefix, ghost, content, lineNumber, column)) return '';
    return ghost;
  }

  function shouldFetchEmptyLineInline(path, linePrefix, content, lineNumber) {
    if (!isWhitespaceOnlyLine(linePrefix)) return false;
    if (!content || !Number.isFinite(Number(lineNumber))) {
      return supportsWorkspaceIndexInline(path) || isSpringConfigFile(path);
    }
    if (findEnclosingBlock(content, lineNumber)) return true;
    if (emptyLineNeighborBelow(content, lineNumber, linePrefix)) return true;
    if (emptyLineNeighborAbove(content, lineNumber, linePrefix)) return true;
    if (isSpringConfigFile(path)) return true;
    return !!String(content || '').trim();
  }

  function emptyLineNeighborBelow(content, lineNumber, linePrefix, radius = INLINE_NEARBY_LINES) {
    const lines = String(content || '').split(/\r?\n/);
    const cur = Number(lineNumber) - 1;
    if (!Number.isFinite(cur) || cur < 0 || cur >= lines.length) return null;
    const indent = lineIndent(linePrefix);
    const maxRow = Math.min(lines.length - 1, cur + radius);
    for (let i = cur + 1; i <= maxRow; i += 1) {
      const raw = lines[i];
      if (!raw.trim()) continue;
      const ind = lineIndent(raw);
      if (ind.length < indent.length) break;
      return { text: raw.trim(), indent: ind, lineNumber: i + 1 };
    }
    return null;
  }

  function emptyLineNeighborAbove(content, lineNumber, linePrefix, radius = INLINE_NEARBY_LINES) {
    const lines = String(content || '').split(/\r?\n/);
    const cur = Number(lineNumber) - 1;
    if (!Number.isFinite(cur) || cur < 0 || cur >= lines.length) return null;
    const indent = lineIndent(linePrefix);
    const minRow = Math.max(0, cur - radius);
    for (let i = cur - 1; i >= minRow; i -= 1) {
      const raw = lines[i];
      if (!raw.trim()) continue;
      const ind = lineIndent(raw);
      if (ind.length < indent.length) break;
      return { text: raw.trim(), indent: ind, lineNumber: i + 1 };
    }
    return null;
  }

  function usesNeighborLineInline(path) {
    const lang = langForPath(path);
    return lang === 'markdown' || lang === 'yaml' || lang === 'xml' || lang === 'html'
      || lang === 'json' || lang === 'ini' || lang === 'toml' || isSpringConfigFile(path)
      || lang === 'dockerfile' || lang === 'makefile' || lang === 'cmake' || lang === 'shell'
      || lang === 'sql' || lang === 'css' || lang === 'scss' || lang === 'less'
      || lang === 'graphql' || lang === 'protobuf';
  }

  /** First statement starter for an empty block body — never duplicate the line below. */
  function emptyBlockBodyStarter(path, indent) {
    if (isGradleBuildFile(path)) return '';
    const lang = langForPath(path);
    if (lang === 'java' || lang === 'groovy') return `${indent}System.out.println();`;
    if (lang === 'kotlin') return `${indent}println()`;
    if (lang === 'python') return `${indent}pass`;
    if (lang === 'javascript' || lang === 'typescript') return `${indent}// TODO`;
    if (lang === 'rust' || lang === 'go' || lang === 'csharp' || lang === 'swift'
      || lang === 'cpp' || lang === 'dart') {
      return `${indent}// TODO`;
    }
    if (lang === 'ruby') return `${indent}# TODO`;
    if (lang === 'php') return `${indent}// TODO`;
    if (lang === 'shell' || lang === 'lua') return `${indent}# TODO`;
    if (lang === 'r') return `${indent}# TODO`;
    return '';
  }

  function inferEmptyLineFromBelowPath(path, indent, belowText) {
    const text = String(belowText || '').trim();
    if (!text) return '';
    const lang = langForPath(path);

    if (lang === 'markdown') {
      const list = text.match(/^([-+*]|\d+\.)\s+/);
      if (list) return `${indent}${list[0]}`;
      if (text.startsWith('>')) return `${indent}> `;
      if (text.startsWith('#')) return `${indent}- `;
      return '';
    }
    if (lang === 'xml' || lang === 'html') {
      const tag = text.match(/^<\/?([A-Za-z][\w:-]*)/);
      if (tag) return `${indent}<${tag[1]}>`;
    }
    if (lang === 'yaml' || isSpringConfigFile(path)) {
      if (text.startsWith('- ')) return `${indent}- `;
      const keyOnly = text.match(/^([A-Za-z0-9_.-]+):\s*$/);
      if (keyOnly) return `${indent}${keyOnly[1]}: `;
      const keyVal = text.match(/^([A-Za-z0-9_.-]+):\s*/);
      if (keyVal) return `${indent}${keyVal[1]}: `;
    }
    if (lang === 'groovy' || isGradleBuildFile(path)) {
      const dep = text.match(/^(implementation|api|compileOnly|testImplementation|runtimeOnly|classpath)\b/);
      if (dep) return `${indent}${dep[1]} `;
    }
    if (lang === 'json') {
      if (text.startsWith('"')) return `${indent}"`;
    }
    if (lang === 'ini' || lang === 'toml') {
      if (text.includes('=')) {
        const key = text.split('=')[0].trim();
        if (key) return `${indent}${key}=`;
      }
    }
    if (lang === 'css' || lang === 'scss' || lang === 'less') {
      const prop = text.match(/^([a-zA-Z-]+)\s*:/);
      if (prop) return `${indent}${prop[1]}: `;
    }
    if (lang === 'graphql') {
      const field = text.match(/^([A-Za-z_]\w*)\s*:/);
      if (field) return `${indent}${field[1]}: `;
    }
    if (lang === 'protobuf') {
      if (text.startsWith('message ')) return `${indent}message `;
    }
    if (lang === 'sql') {
      const kw = text.match(/^([A-Za-z]+)/);
      if (kw) return `${indent}${kw[1]} `;
    }
    if (lang === 'dockerfile' || lang === 'makefile' || lang === 'cmake' || lang === 'shell') {
      const kw = text.match(/^([A-Za-z_./@$%-]+)/);
      if (kw && !kw[1].startsWith('}')) return `${indent}${kw[1]} `;
    }
    if ((lang === 'java' || lang === 'kotlin' || lang === 'groovy') && text.startsWith('return ')) {
      return `${indent}// ...`;
    }
    return '';
  }

  function preferredMemberInlineGhost(members, memberPrefix) {
    if (!members.length) return '';
    let best;
    if (!memberPrefix) {
      best = members.find((m) => m.label === 'println')
        || members.find((m) => m.label === 'out')
        || members[0];
      if (!best) return '';
      return best.kind === 'method' ? `${best.label}()` : best.label;
    }
    best = members.find((m) =>
      m.label.toLowerCase().startsWith(memberPrefix.toLowerCase())
      && m.label.length > memberPrefix.length,
    );
    if (!best) return '';
    let rest = best.label.slice(memberPrefix.length);
    if (best.kind === 'method' && !rest.endsWith('(')) rest += '()';
    return rest;
  }

  function memberInlineSuffix(content, linePrefix, path, javaLevel = 21) {
    const dot = dotQualifierFromLinePrefix(linePrefix);
    if (!dot) return '';
    const members = localMemberCompletions(content, dot.qualifier, dot.memberPrefix, path, null, javaLevel);
    return preferredMemberInlineGhost(members, dot.memberPrefix);
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
      if (!suggestionMatchesEditorLanguage(path, label)) return;
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
      for (const m of localMemberCompletions(text, dot.qualifier, dot.memberPrefix, path, model, javaLevel)) {
        const insert = m.kind === 'method' ? `${m.label}()` : m.label;
        const filterText = `${dot.memberPrefix || ''}${m.label}`;
        push(m.label, m.kind, m.detail || `${dot.qualifier}.${m.label}`, insert, '0', filterText);
      }
      return { suggestions, linePrefix, prefix, range, seen };
    }

    if (isJavaLikePath(path) && shouldOfferJavaScopeCompletions(text, position.lineNumber, linePrefix)) {
      for (const { name, typeHint } of collectJavaScopeVariables(text, position.lineNumber)) {
        if (!prefix || name.toLowerCase().startsWith(prefix.toLowerCase())) {
          push(name, 'variable', typeHint || 'variable', name, '0');
        }
      }
      for (const m of staticImportCompletionItems(text, prefix)) {
        push(m.label, m.kind, m.detail, `${m.label}()`, '0');
      }
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
      if (!prefix) continue;
      if (sym.name.toLowerCase().startsWith(prefix.toLowerCase())) {
        push(sym.name, sym.kind, sym.kind, sym.name, '1');
      }
    }

    const completionPrefix = prefix || '';
    if (
      helpers?.repoApi
      && helpers.getRepo?.()
      && !shouldPreferModifierKeywordInline(path, linePrefix, completionPrefix)
    ) {
      const cached = readCachedIndexCompletions(helpers, model, position, completionPrefix);
      if (cached?.length) {
        mergeIndexItemsIntoSuggestions(cached, range, seen, null, suggestions, 40, path);
      }
    }

    if (completionPrefix && shouldPreferControlKeywordInline(path, linePrefix, completionPrefix)) {
      const lower = completionPrefix.toLowerCase();
      for (const kw of statementKeywordsForPath(path)) {
        if (kw.toLowerCase().startsWith(lower)) {
          push(kw, 'keyword', 'keyword', kw, '0');
        }
      }
    } else if (isJavaLikePath(path) && completionPrefix
      && shouldPreferJavaTypeInline(path, linePrefix, completionPrefix)) {
      const lower = completionPrefix.toLowerCase();
      for (const name of JAVA_SYNC_TYPE_NAMES) {
        if (name.toLowerCase().startsWith(lower)) {
          push(name, 'class', 'type', name, '0');
        }
      }
    }

    for (const kw of statementKeywordsForPath(path)) {
      if (
        !shouldPreferAiStatementInline(path, linePrefix, text, position.lineNumber)
        && prefix.length >= minKwChars
        && kw.toLowerCase().startsWith(prefix.toLowerCase())
        && !shouldPreferControlKeywordInline(path, linePrefix, prefix)
        && !(shouldPreferJavaTypeInline(path, linePrefix, prefix) && javaSyncTypeInlineSuffix(prefix))
      ) {
        push(kw, 'keyword', 'keyword', kw, '3');
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
    const lang = langForPath(path);
    const singleCharLoop = { f: 'for', i: 'if', w: 'while' };
    if (token.length === 1 && singleCharLoop[lower] && lang !== 'rust') {
      const preferred = singleCharLoop[lower];
      if (statementKeywordsForPath(path).some((kw) => kw.toLowerCase() === preferred)) {
        return preferred.slice(1);
      }
    }
    let best = null;
    for (const kw of statementKeywordsForPath(path)) {
      const kl = kw.toLowerCase();
      if (kl.startsWith(lower) && kw.length > token.length) {
        if (token.length === 1 && kl === 'fi') continue;
        const suffix = kw.slice(token.length);
        if (!best || suffix.length < best.length) best = suffix;
      }
    }
    return best || '';
  }

  function collectContentIdentifierCandidates(content, path, lineNumber = null) {
    const keywords = new Set(clientKeywordsForPath(path).map((kw) => kw.toLowerCase()));
    const seen = new Map();
    const identRe = /\b([A-Za-z_][\w$]*)\b/g;
    const lines = String(content || '').split(/\r?\n/);
    for (let i = 0; i < lines.length; i += 1) {
      const lineNo = i + 1;
      if (lineNumber != null && !isNearbyLine(lineNo, lineNumber)) continue;
      const line = lines[i];
      const trimmed = line.trim();
      if (!trimmed || trimmed.startsWith('//') || trimmed.startsWith('*')) continue;
      identRe.lastIndex = 0;
      let m;
      for (;;) {
        m = identRe.exec(line);
        if (!m) break;
        const name = m[1];
        if (keywords.has(name.toLowerCase())) continue;
        const prev = seen.get(name);
        if (prev) {
          prev.count += 1;
          prev.minLine = Math.min(prev.minLine, lineNo);
          prev.maxLine = Math.max(prev.maxLine, lineNo);
        } else {
          seen.set(name, {
            name,
            count: 1,
            minLine: lineNo,
            maxLine: lineNo,
            isType: /^[A-Z]/.test(name),
          });
        }
      }
    }
    for (const sym of extractSymbols(content, path)) {
      if (lineNumber != null && !isNearbyLine(sym.line, lineNumber)) continue;
      const prev = seen.get(sym.name);
      if (prev) {
        prev.count += 1;
        prev.minLine = Math.min(prev.minLine, sym.line);
        prev.maxLine = Math.max(prev.maxLine, sym.line);
      } else {
        seen.set(sym.name, {
          name: sym.name,
          count: 1,
          minLine: sym.line,
          maxLine: sym.line,
          isType: /^[A-Z]/.test(sym.name),
        });
      }
    }
    return [...seen.values()];
  }

  /** Sync inline suffix from identifiers already present in the buffer (and open-tab context via content). */
  function scopeIdentifierInlineSuffix(content, path, lineNumber, token) {
    if (!token || token.length < MIN_INDEX_INLINE_CHARS) return '';
    const lower = token.toLowerCase();
    const candidates = collectContentIdentifierCandidates(content, path, lineNumber)
      .filter((c) => c.name.toLowerCase().startsWith(lower) && c.name.length > token.length);
    if (!candidates.length) return '';
    const ranked = rankScopeCandidates(candidates, lineNumber, { token, path });
    return ranked[0]?.name.slice(token.length) || '';
  }

  function extractControlParenContent(line, keyword) {
    const re = new RegExp(`\\b${keyword}\\s*\\(`);
    const m = String(line || '').match(re);
    if (!m) return null;
    const start = m.index + m[0].length;
    let depth = 1;
    for (let i = start; i < line.length; i += 1) {
      const ch = line[i];
      if (ch === '(') depth += 1;
      else if (ch === ')') {
        depth -= 1;
        if (depth === 0) return line.slice(start, i).trim();
      }
    }
    return null;
  }

  function findEnclosingBlock(content, lineNumber) {
    const lines = String(content || '').split(/\r?\n/);
    const cur = Number(lineNumber) - 1;
    if (!Number.isFinite(cur) || cur < 0 || cur >= lines.length) return null;
    const curIndent = lineIndent(lines[cur]);

    for (let i = cur - 1; i >= 0; i--) {
      const line = lines[i];
      const ind = lineIndent(line);
      const trimmed = line.trimEnd();
      if (ind.length < curIndent.length) {
        if (/^\s*def\s+\w/.test(trimmed)) {
          return { type: 'block', indent: curIndent };
        }
        if (/^\s*(local\s+)?function\b/.test(trimmed)) {
          return { type: 'block', indent: curIndent };
        }
        if (!(trimmed.endsWith('{') || trimmed.endsWith(':'))) {
          continue;
        }
        if (trimmed.endsWith(':')) {
          if (/^\s*if\b/.test(trimmed)) {
            return { type: 'if', cond: trimmed, indent: curIndent };
          }
          if (/^\s*(elif|else)\b/.test(trimmed)) {
            return { type: 'else', indent: curIndent };
          }
          if (/^\s*for\b/.test(trimmed)) {
            return { type: 'for', init: trimmed, indent: curIndent };
          }
          if (/^\s*while\b/.test(trimmed)) {
            return { type: 'while', cond: trimmed, indent: curIndent };
          }
          return { type: 'block', indent: curIndent };
        }
        const whileInside = extractControlParenContent(trimmed, 'while');
        if (whileInside != null) {
          return { type: 'while', cond: whileInside, indent: curIndent };
        }
        const forInside = extractControlParenContent(trimmed, 'for');
        if (forInside != null) {
          const type = forInside.includes(':') && !forInside.includes(';') ? 'for-each' : 'for';
          return { type, init: forInside, indent: curIndent };
        }
        const ifInside = extractControlParenContent(trimmed, 'if');
        if (ifInside != null) {
          return { type: 'if', cond: ifInside, indent: curIndent };
        }
        if (/\belse\s*\{?\s*$/.test(trimmed)) {
          return { type: 'else', indent: curIndent };
        }
        if (/\b(class|interface|enum|record|struct|trait|namespace)\s+\w+/.test(trimmed)) {
          return { type: 'type-body', indent: curIndent };
        }
        return { type: 'block', indent: curIndent };
      }
      if (trimmed === '{' && i > 0) {
        const prev = lines[i - 1].trimEnd();
        if (/\b(class|interface|enum|record|struct|trait|namespace)\s+\w+/.test(prev)) {
          return { type: 'type-body', indent: curIndent };
        }
        const whileInside = extractControlParenContent(prev, 'while');
        if (whileInside != null) {
          return { type: 'while', cond: whileInside, indent: curIndent };
        }
        const forInside = extractControlParenContent(prev, 'for');
        if (forInside != null) {
          const type = forInside.includes(':') && !forInside.includes(';') ? 'for-each' : 'for';
          return { type, init: forInside, indent: curIndent };
        }
        const ifInside = extractControlParenContent(prev, 'if');
        if (ifInside != null) {
          return { type: 'if', cond: ifInside, indent: curIndent };
        }
        return { type: 'block', indent: curIndent };
      }
    }
    return null;
  }

  function sameIndentLinesInBlock(content, lineNumber, indent, radius = INLINE_NEARBY_LINES) {
    const lines = String(content || '').split(/\r?\n/);
    const cur = lineNumber - 1;
    const above = [];
    const below = [];
    for (let i = 0; i < lines.length; i++) {
      if (i === cur || !lines[i].trim()) continue;
      if (!isNearbyLine(i + 1, lineNumber, radius)) continue;
      if (lineIndent(lines[i]) !== indent) continue;
      const entry = { i, text: lines[i].trim() };
      if (i < cur) above.push(entry);
      else below.push(entry);
    }
    return { above, below };
  }

  function inferEmptyLineContinuationFullLine(path, content, lineNumber, linePrefix, javaLevel = 17) {
    const indent = lineIndent(linePrefix);
    const block = findEnclosingBlock(content, lineNumber);
    if (!block) {
      if (usesNeighborLineInline(path)) {
        const neighbor = emptyLineNeighborBelow(content, lineNumber, linePrefix);
        if (neighbor) {
          return inferEmptyLineFromBelowPath(path, indent || neighbor.indent, neighbor.text);
        }
        const aboveNeighbor = emptyLineNeighborAbove(content, lineNumber, linePrefix);
        if (aboveNeighbor) {
          return inferEmptyLineFromBelowPath(path, indent || aboveNeighbor.indent, aboveNeighbor.text);
        }
      }
      return '';
    }
    const blockIndent = indent || block.indent;
    const lang = langForPath(path);
    const { above, below } = sameIndentLinesInBlock(content, lineNumber, blockIndent);
    const effectiveIndent = blockIndent;

    if (lang === 'java' || lang === 'kotlin' || lang === 'groovy') {
      if (block.type === 'while') {
        const cond = block.cond;
        let m;
        if ((m = cond.match(/i\s*<\s*(\w+)\.size\s*\(\s*\)/))) {
          const list = m[1];
          const hasGet = above.some((x) => x.text.includes('.get(i)'));
          if (!hasGet) {
            return javaLevel >= 10
              ? `${blockIndent}var item = ${list}.get(i);`
              : `${blockIndent}${list}.get(i);`;
          }
          if (!above.some((x) => x.text === 'i++;' || /\bi\s*\+\+\s*;/.test(x.text))) {
            return `${blockIndent}i++;`;
          }
        }
        if ((m = cond.match(/(\w+)\.hasNext\s*\(\s*\)/))) {
          const iter = m[1];
          if (!above.some((x) => x.text.includes('.next()'))) {
            return javaLevel >= 10
              ? `${effectiveIndent}var item = ${iter}.next();`
              : `${effectiveIndent}Object item = ${iter}.next();`;
          }
        }
      }
      if (block.type === 'for-each' || (block.type === 'for' && block.init.includes(':') && !block.init.includes(';'))) {
        const varName = forEachVarName(block.init);
        if (varName && !above.some((x) => x.text.includes(varName))) {
          if (block.init.includes('.entrySet()')) {
            return `${effectiveIndent}var key = ${varName}.getKey();`;
          }
          return `${effectiveIndent}System.out.println(${varName});`;
        }
      }
      if (block.type === 'for') {
        const init = block.init;
        let m;
        if ((m = init.match(/(?:var\s+)?(\w+)\s*:\s*(\w+)\s*$/))) {
          const varName = m[1];
          if (!above.length) return `${effectiveIndent}System.out.println(${varName});`;
        }
        const indexed = parseJavaIndexedForLoop(init);
        if (indexed) {
          const access = javaIndexedAccessExpr(indexed);
          const hasBodyLine = access && above.some((x) => x.text.includes(access));
          if (!hasBodyLine) {
            return javaIndexedLoopBodyLine(indexed, effectiveIndent, javaLevel, lang);
          }
          if (!above.some((x) => x.text === 'i++;' || /\bi\s*\+\+\s*;/.test(x.text))) {
            return `${effectiveIndent}i++;`;
          }
        }
      }
      if ((block.type === 'if' || block.type === 'else') && !above.length) {
        return `${effectiveIndent}// TODO`;
      }
    }

    if (lang === 'python') {
      if ((block.type === 'while' || block.type === 'for' || block.type === 'if' || block.type === 'else')
        && !above.length) {
        return `${effectiveIndent}pass`;
      }
    }

    if ((block.type === 'block' || block.type === 'type-body') && !above.length) {
      const starter = emptyBlockBodyStarter(path, effectiveIndent);
      if (starter) return starter;
    }

    if (!above.length && below.length) {
      const hint = inferEmptyLineFromBelowPath(path, effectiveIndent, below[0].text);
      if (hint) return hint;
    }

    if (usesNeighborLineInline(path)) {
      const neighbor = emptyLineNeighborBelow(content, lineNumber, linePrefix);
      if (neighbor) {
        const hint = inferEmptyLineFromBelowPath(path, effectiveIndent || neighbor.indent, neighbor.text);
        if (hint) return hint;
      }
      const aboveNeighbor = emptyLineNeighborAbove(content, lineNumber, linePrefix);
      if (aboveNeighbor) {
        const hint = inferEmptyLineFromBelowPath(path, effectiveIndent || aboveNeighbor.indent, aboveNeighbor.text);
        if (hint) return hint;
      }
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

  /** Synchronous local ghost — identifiers, types, members, keywords only (no loops/blocks). */
  function localInlineSuggestion(
    path, linePrefix, content, lineNumber, javaLevel = 17, column = 0, indexCtx = null,
    { fast = false } = {},
  ) {
    if (!fast) {
      const springInline = springConfigLocalInline(path, linePrefix, content, lineNumber, column);
      if (springInline) return springInline;
      const gradlePropsInline = gradlePropertiesLocalInline(path, linePrefix, content, lineNumber);
      if (gradlePropsInline) return gradlePropsInline;
    }
    const member = memberInlineSuffix(content, linePrefix, path, javaLevel);
    if (member) return member;
    if (content && isOperandContext(linePrefix)) {
      const operand = operandInlineSuffix(content, path, lineNumber, linePrefix);
      if (operand) return operand;
    }
    if (content && isReturnContext(linePrefix)) {
      const ret = returnInlineSuffix(content, path, lineNumber, linePrefix);
      if (ret) return ret;
    }
    if (content && isCallArgContext(linePrefix, path)) {
      const arg = callArgInlineSuffix(content, path, lineNumber, linePrefix);
      if (arg) return arg;
    }
    const syntax = syntaxInlineSuffix(path, linePrefix, content, lineNumber, column);
    if (syntax) return syntax;
    const trimmed = linePrefix.trimEnd();
    const token = extractInlinePartialToken(linePrefix);
    if (token) {
      // Access modifiers before scope identifiers — otherwise `private` + nearby
      // `privateField` / `stat` + `status` ghosts steal Space and feel like autocorrect.
      // At bare statement start, single-letter f/i/w still prefer for/if/while over final.
      const modRx = modifierRegexesForPath(path);
      const leadTrimmed = String(linePrefix || '').trimStart();
      const afterFinishedModifiers = !!(modRx
        && modRx.partial.test(leadTrimmed)
        && modRx.afterFinished.test(leadTrimmed));
      if (shouldPreferModifierKeywordInline(path, linePrefix, token)) {
        if (!token) return '';
        const preferControlOverFinal = !afterFinishedModifiers
          && token.length <= 1
          && shouldPreferControlKeywordInline(path, linePrefix, token)
          && !hasCompleteControlKeyword(trimmed);
        if (!preferControlOverFinal) {
          const modSuffix = modifierKeywordInlineSuffix(token, path);
          if (modSuffix) return modSuffix;
          return '';
        }
      }
      if (
        (!fast || token.length <= 2)
        && shouldPreferControlKeywordInline(path, linePrefix, token)
        && !hasCompleteControlKeyword(trimmed)
      ) {
        const kwSuffix = keywordInlineSuffix(path, token);
        if (kwSuffix) return kwSuffix;
      }
      if (fast && token.length >= 1) {
        const fastScope = fastLineWindowScopeSuffix(content, path, lineNumber, token);
        if (fastScope) return fastScope;
      }
      if (!fast && token.length >= MIN_INDEX_INLINE_CHARS) {
        const scopeSuffix = scopeIdentifierInlineSuffix(content, path, lineNumber, token);
        if (scopeSuffix) return scopeSuffix;
      }
      if (indexCtx?.helpers && indexCtx?.model && indexCtx?.position) {
        const indexSuffix = inlineSuffixFromCachedIndex(
          indexCtx.helpers, indexCtx.model, indexCtx.position, linePrefix, token,
        );
        if (indexSuffix) return indexSuffix;
      }
      if (shouldPreferJavaTypeInline(path, linePrefix, token)) {
        const typeSuffix = javaSyncTypeInlineSuffix(token);
        if (typeSuffix) return typeSuffix;
      }
    }
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

  function buildInlineItems(model, position, linePrefix, text, path = '') {
    const suffix = capInlineText(sanitizeInlineGhostText(text));
    if (!suffix) return { items: [] };
    const token = extractInlinePartialToken(linePrefix);
    const ghostLine = suffix.includes('\n') ? suffix.split('\n')[0] : suffix;
    let filterText = token && !ghostLine.startsWith(token) ? token + ghostLine : ghostLine;
    if (inlineFilterUsesFullPrefix(linePrefix, path)) {
      filterText = `${linePrefix}${ghostLine}`;
    }
    return {
      items: [{
        insertText: ghostLine,
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

  function indexCompleteLookupKey(helpers, repo, path, line, column, prefix, contentLen) {
    const overlayHash = overlayFingerprint(helpers, path);
    return `${indexCompleteCacheKey(repo, path, line, column, prefix || '', contentLen)}:${overlayHash}`;
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
    const cacheKey = indexCompleteLookupKey(
      helpers, repo, path, position.lineNumber, position.column, prefix || '', contentLen,
    );
    const cached = indexCompleteCache.get(cacheKey);
    if (cached && Date.now() - cached.time < INDEX_COMPLETE_CACHE_MS) {
      return cached.items;
    }
    return null;
  }

  function inlineSuffixFromIndexItems(items, linePrefix, prefix, path = '') {
    if (!Array.isArray(items) || !items.length) return '';
    const member = dotQualifierFromLinePrefix(linePrefix);
    const tokenPrefix = member
      ? (member.memberPrefix || '')
      : (prefix || extractInlinePartialToken(linePrefix) || '');

    // Never turn `private` / `private ` into PrivateKeyEntry (or similar) via index ghosts.
    if (
      !member
      && (
        shouldPreferModifierKeywordInline(path, linePrefix, tokenPrefix)
        || isCompleteModifierSequence(linePrefix, path)
      )
    ) {
      return '';
    }

    const useServerOrder = member || supportsWorkspaceIndexInline(path);
    const ordered = useServerOrder ? items : rankIndexItemsForInline(items);

    // Empty line: top jdtls/index completion is the ghost (LSP behavior).
    if (!member && !tokenPrefix && isWhitespaceOnlyLine(linePrefix)) {
      for (const item of ordered) {
        if (!isCodeLikeCompletion(item.label, item.kind)) continue;
        let insert = stripSnippetMarkers(String(item.insert || item.label || ''));
        if (!insert || insert.includes('\n')) continue;
        const indent = lineIndent(linePrefix);
        if (indent && insert.startsWith(indent)) {
          insert = insert.slice(indent.length);
        }
        const trimmed = insert.trimStart();
        if (trimmed) return trimmed;
      }
      return '';
    }

    if (!member && !tokenPrefix) return '';
    for (const item of ordered) {
      const suffix = inlineSuffixFromLspItem(item, linePrefix, prefix);
      if (suffix) return suffix;
    }
    return '';
  }

  function inlineSuffixFromCachedIndex(helpers, model, position, linePrefix, prefix) {
    const items = readCachedIndexCompletions(helpers, model, position, prefix);
    if (!items?.length) return '';
    const path = helpers.getActivePath?.() || '';
    return inlineSuffixFromIndexItems(items, linePrefix, prefix, path);
  }

  function mergeIndexItemsIntoSuggestions(items, range, seen, memberContext, suggestions, limit = 80, path = '') {
    if (!Array.isArray(items)) return;
    for (const item of items) {
      const mapped = mapIndexItemToSuggestion(item, range, seen, memberContext, path);
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
    const cacheKey = indexCompleteLookupKey(helpers, repo, path, line, column, prefix || '', contentLen);
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

  async function fetchIndexInlineSuffix(helpers, model, position, linePrefix, prefix) {
    const path = helpers.getActivePath?.() || '';
    const cachedSuffix = inlineSuffixFromCachedIndex(helpers, model, position, linePrefix, prefix);
    if (cachedSuffix) return cachedSuffix;
    const member = dotQualifierFromLinePrefix(linePrefix);
    const emptyLine = isWhitespaceOnlyLine(linePrefix) && !member;
    const timeoutMs = member
      ? LSP_INLINE_MEMBER_FETCH_MS
      : (emptyLine ? 450 : LSP_INLINE_FETCH_MS);
    const items = await fetchCompletionsWithTimeout(
      helpers, model, position, prefix || '', timeoutMs,
    );
    if (!items?.length) return '';
    return inlineSuffixFromIndexItems(items, linePrefix, prefix, path);
  }

  const COMPLETION_FETCH_TIMEOUT_MS = 8000;
  const INDEX_COMPLETION_BUDGET_MS = 600;

  function fetchCompletionsWithTimeout(helpers, model, position, prefix, timeoutMs = COMPLETION_FETCH_TIMEOUT_MS) {
    return raceWithTimeout(
      fetchCompletions(helpers, model, position, prefix),
      timeoutMs,
      'Completions timed out',
    );
  }

  const definitionCache = new Map();
  const definitionInflight = new Map();
  const hoverCache = new Map();
  const DEF_CACHE_MAX = 512;
  const DEF_NAV_GUARD_MS = 400;
  let lastDefinitionNavMs = 0;
  let definitionRequestSeq = 0;

  function markDefinitionNavigation() {
    lastDefinitionNavMs = Date.now();
  }

  function resetDefinitionInflight(reason) {
    if (definitionInflight.size > 0) {
      console.warn('[ReaperLang] clearing definition inflight:', reason);
      definitionInflight.clear();
    }
  }

  /** Avoid "no definition" when a parallel lookup already navigated (F12 + provideDefinition). */
  function reportNoDefinition(helpers) {
    if (Date.now() - lastDefinitionNavMs < DEF_NAV_GUARD_MS) return;
    helpers.showNavigationResult?.('No definition found', 'info');
  }

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

  function reaperUriForPath(path) {
    let normalized = String(path || '').replace(/\\/g, '/');
    if (!normalized) return monaco.Uri.parse('reaper://workspace/');
    const isAbs = normalized.startsWith('/') || /^[A-Za-z]:\//.test(normalized);
    if (!isAbs) normalized = normalized.replace(/^\/+/, '');
    return monaco.Uri.parse(`reaper://workspace/${encodeURIComponent(normalized)}`);
  }

  function pathFromReaperUri(uri) {
    if (!uri || uri.scheme !== 'reaper' || uri.authority !== 'workspace') return '';
    const raw = uri.path.replace(/^\//, '');
    try {
      return decodeURIComponent(raw);
    } catch {
      return raw;
    }
  }

  function withNavigationBusy(helpers, label, fn) {
    if (typeof helpers?.runWithNavigationBusy === 'function') {
      return helpers.runWithNavigationBusy(label, fn);
    }
    return fn();
  }

  function definitionLocationFromHit(hit) {
    if (!hit?.path) return null;
    const nameLen = (hit.name || 'symbol').length;
    const line = Math.max(1, hit.line || 1);
    const col = Math.max(1, hit.column || 1);
    return {
      uri: reaperUriForPath(hit.path),
      range: new monaco.Range(line, col, line, col + nameLen),
    };
  }

  async function lookupDefinition(helpers, model, position) {
    if (!helpers.repoApi || !helpers.getRepo) return null;
    const repo = helpers.getRepo();
    if (!repo) return null;
    const path = resolveEditorPath(helpers, model);
    if (!path) return null;

    const line = position.lineNumber;
    const column = position.column;
    const content = model.getValue();
    const cacheKey = definitionCacheKey(repo, path, line, column, content);
    const cached = definitionCache.get(cacheKey);
    if (cached) {
      return definitionLocationFromHit(cached);
    }

    const inflight = definitionInflight.get(cacheKey);
    if (inflight) return inflight;

    const requestSeq = ++definitionRequestSeq;
    const promise = withNavigationBusy(helpers, 'Go to definition…', async () => {
      try {
        const url = helpers.repoApi(repo, '/workspace/definition');
        const hit = await raceWithTimeout(
          helpers.api(url, {
            method: 'POST',
            body: JSON.stringify({ path, line, column, content }),
            allowDuringSave: true,
          }),
          DEFINITION_FETCH_TIMEOUT_MS,
          'Definition lookup timed out',
        );
        if (requestSeq !== definitionRequestSeq) return null;
        if (!hit?.path) return null;
        if (definitionCache.size >= DEF_CACHE_MAX) definitionCache.clear();
        definitionCache.set(cacheKey, hit);
        return definitionLocationFromHit(hit);
      } catch (err) {
        if (requestSeq === definitionRequestSeq) {
          resetDefinitionInflight(err?.message || 'definition failed');
        }
        console.warn('[ReaperLang] lookupDefinition failed:', err);
        return null;
      }
    }).finally(() => {
      definitionInflight.delete(cacheKey);
    });

    definitionInflight.set(cacheKey, promise);
    return promise;
  }

  async function lookupHoverInfo(helpers, model, position, { member, symbol } = {}) {
    if (!helpers.repoApi || !helpers.getRepo) return null;
    const repo = helpers.getRepo();
    if (!repo) return null;
    const path = resolveEditorPath(helpers, model);
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
      const info = hit && (hit.name || hit.signature || hit.documentation) ? hit : null;
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
    return hoverInfoHasContent(info) ? info : null;
  }

  const RENAME_FETCH_TIMEOUT_MS = 15000;
  const DEFINITION_FETCH_TIMEOUT_MS = 12000;
  const REFERENCES_FETCH_TIMEOUT_MS = 12000;

  function raceWithTimeout(promise, timeoutMs, message = 'Request timed out') {
    if (!timeoutMs || timeoutMs <= 0) return promise;
    const schedule = (typeof globalThis !== 'undefined' && typeof globalThis.setTimeout === 'function')
      ? globalThis.setTimeout
      : (typeof setTimeout === 'function' ? setTimeout : null);
    if (!schedule) return promise;
    return Promise.race([
      promise,
      new Promise((_, reject) => {
        schedule(() => reject(new Error(message)), timeoutMs);
      }),
    ]);
  }

  async function postWorkspaceLsp(helpers, model, position, endpoint, extra = {}, timeoutMs = 0) {
    if (!helpers.repoApi || !helpers.getRepo) return null;
    const repo = helpers.getRepo();
    if (!repo) return null;
    const path = resolveEditorPath(helpers, model);
    if (!path) return null;
    const line = position.lineNumber;
    const column = position.column;
    const content = model.getValue();
    const request = helpers.api(helpers.repoApi(repo, endpoint), {
      method: 'POST',
      body: JSON.stringify({ path, line, column, content, ...extra }),
    });
    if (!timeoutMs) return request;
    return raceWithTimeout(request, timeoutMs);
  }

  const REFERENCES_CACHE_MAX = 64;
  const referencesCache = new Map();
  const referencesInflight = new Map();

  function referencesCacheKey(repo, path, line, column, content) {
    return `${repo}|${path}|${line}|${column}|${content.length}|${content.slice(0, 240)}`;
  }

  async function lookupReferences(helpers, model, position) {
    if (!helpers.repoApi || !helpers.getRepo) return [];
    const repo = helpers.getRepo();
    if (!repo) return [];
    const path = resolveEditorPath(helpers, model);
    if (!path) return [];
    const line = position.lineNumber;
    const column = position.column;
    const content = model.getValue();
    const cacheKey = referencesCacheKey(repo, path, line, column, content);
    const cached = referencesCache.get(cacheKey);
    if (cached) return cached;

    const inflight = referencesInflight.get(cacheKey);
    if (inflight) return inflight;

    const promise = withNavigationBusy(helpers, 'Finding usages…', async () => {
      try {
        const refs = await postWorkspaceLsp(
          helpers, model, position, '/workspace/references', {}, REFERENCES_FETCH_TIMEOUT_MS,
        ) || [];
        if (referencesCache.size >= REFERENCES_CACHE_MAX) referencesCache.clear();
        referencesCache.set(cacheKey, refs);
        return refs;
      } catch (err) {
        console.warn('[ReaperLang] references failed:', err);
        return [];
      } finally {
        referencesInflight.delete(cacheKey);
      }
    });
    referencesInflight.set(cacheKey, promise);
    return promise;
  }

  async function runJavaOrganizeImports(editor, helpers) {
    const model = editor.getModel();
    const pos = editor.getPosition();
    if (!model || !pos) return;
    try {
      const actions = await postWorkspaceLsp(helpers, model, pos, '/workspace/java/code-actions', {
        only: ['source.organizeImports'],
      });
      const action = (actions || []).find((a) => a?.edits?.length);
      if (!action) {
        helpers.toast?.('No import changes', 'info');
        return;
      }
      const changed = await helpers.applyJavaWorkspaceEdits?.(action.edits);
      if (changed) helpers.toast?.('Organized imports', 'success');
      else helpers.toast?.('Imports already organized', 'info');
    } catch (err) {
      helpers.toast?.(err?.message || 'Organize imports failed', 'error');
    }
  }

  const JAVA_REFACTOR_KINDS = [
    'refactor',
    'refactor.extract',
    'refactor.inline',
    'refactor.rewrite',
    'source',
  ];

  function isJavaEditorPath(helpers, model) {
    const path = resolveEditorPath(helpers, model) || '';
    return path.toLowerCase().endsWith('.java');
  }

  async function runJavaRefactor(editor, helpers) {
    const model = editor.getModel();
    if (!model || !isJavaEditorPath(helpers, model)) return;
    const sel = editor.getSelection();
    const pos = sel && !sel.isEmpty()
      ? { lineNumber: sel.startLineNumber, column: sel.startColumn }
      : editor.getPosition();
    if (!pos) return;
    const extra = { only: JAVA_REFACTOR_KINDS };
    if (sel && !sel.isEmpty()) {
      extra.end_line = sel.endLineNumber;
      extra.end_column = sel.endColumn;
    }
    const cursorAnchor = (() => {
      try {
        const coords = editor.getScrolledVisiblePosition(pos);
        const dom = editor.getDomNode();
        if (!coords || !dom) return null;
        const rect = dom.getBoundingClientRect();
        const left = rect.left + coords.left;
        const top = rect.top + coords.top;
        return {
          left,
          top,
          bottom: top + (coords.height || 16),
          right: left + 1,
          width: 1,
          height: coords.height || 16,
          getBoundingClientRect() {
            return {
              left: this.left,
              top: this.top,
              bottom: this.bottom,
              right: this.right,
              width: this.width,
              height: this.height,
            };
          },
        };
      } catch {
        return null;
      }
    })();
    try {
      helpers.hideQuickFixMenu?.();
      const actions = await withNavigationBusy(helpers, 'Loading refactorings…', async () => (
        await postWorkspaceLsp(helpers, model, pos, '/workspace/java/code-actions', extra)
      )) || [];
      const actionable = (actions || []).filter((a) => a?.edits?.length);
      if (!actionable.length) {
        if (helpers.isJdtlsReady && !helpers.isJdtlsReady()) {
          helpers.toast?.('Java language server still indexing…', 'info');
        } else {
          helpers.toast?.('No refactorings available here', 'info');
        }
        return;
      }
      helpers.markJdtlsReady?.();
      const menuItems = actionable.map((a) => ({
        title: a.title || 'Refactor',
        provider: 'local',
        edits: a.edits,
        kind: a.kind,
      }));
      const applyPicked = async (picked) => {
        try {
          const changed = await helpers.applyJavaWorkspaceEdits?.(picked.edits);
          helpers.toast?.(
            changed
              ? `Applied: ${picked.title || 'Refactor'} (${changed} file${changed === 1 ? '' : 's'})`
              : 'No changes from refactoring',
            changed ? 'success' : 'info',
          );
          if (changed) {
            (helpers.refreshDiagnosticsAfterFix || helpers.scheduleDiagnostics)?.();
          }
        } catch (err) {
          helpers.toast?.(err?.message || 'Refactor failed', 'error');
        }
      };
      if (menuItems.length === 1) {
        await applyPicked(menuItems[0]);
        return;
      }
      if (helpers.showRefactorStaircaseMenu) {
        helpers.showRefactorStaircaseMenu(menuItems, (picked) => {
          void applyPicked(picked);
        }, cursorAnchor);
      } else {
        helpers.showQuickFixMenu?.(menuItems, (picked) => {
          void applyPicked(picked);
        }, cursorAnchor);
      }
    } catch (err) {
      helpers.toast?.(err?.message || 'Refactor failed', 'error');
    }
  }

  async function runRename(editor, helpers) {
    const model = editor.getModel();
    const pos = editor.getPosition();
    if (!model || !pos) return;
    const path = resolveEditorPath(helpers, model);
    if (!path) return;
    const word = model.getWordAtPosition(pos)?.word || '';
    if (!word) {
      helpers.toast?.('Rename not available here', 'info');
      return;
    }
    const newName = helpers.promptRename
      ? await helpers.promptRename({ title: 'Rename Symbol', subtitle: path, value: word })
      : window.prompt('Rename to:', word);
    if (!newName || newName === word) return;
    return withNavigationBusy(helpers, 'Renaming…', async () => {
      try {
        const result = await postWorkspaceLsp(
          helpers, model, pos, '/workspace/rename', { new_name: newName }, RENAME_FETCH_TIMEOUT_MS,
        );
        const edits = Array.isArray(result) ? result : (result?.edits || []);
        const pathRename = Array.isArray(result) ? null : result?.path_rename;
        if (!edits?.length && !pathRename) {
          helpers.toast?.('Rename produced no edits', 'info');
          return;
        }
        const changed = await helpers.applyJavaWorkspaceEdits?.(edits);
        let pathChanged = false;
        if (pathRename?.from && pathRename?.to) {
          pathChanged = await helpers.renameWorkspacePath?.(pathRename.from, pathRename.to, { skipSymbolEdits: true }) === true;
        }
        const totalChanged = (changed || 0) + (pathChanged ? 1 : 0);
        helpers.toast?.(
          totalChanged
            ? `Renamed to ${newName} (${totalChanged} file${totalChanged === 1 ? '' : 's'})`
            : 'Rename failed',
          totalChanged ? 'success' : 'error',
        );
      } catch (err) {
        helpers.toast?.(err?.message || 'Rename failed', 'error');
      }
    });
  }

  async function runFindUsages(editor, helpers) {
    const model = editor.getModel();
    const pos = editor.getPosition();
    if (!model || !pos) return;
    const word = model.getWordAtPosition(pos)?.word || '';
    if (!word) {
      helpers.toast?.('Find Usages not available here', 'info');
      return;
    }
    helpers.showJavaReferences?.([], `Usages of ${word}`, { loading: true });
    const refs = await lookupReferences(helpers, model, pos);
    if (!refs.length) {
      helpers.hideJavaReferences?.();
      helpers.showNavigationResult?.('No usages found', 'info');
      helpers.toast?.('No usages found', 'info');
      return;
    }
    helpers.showJavaReferences?.(refs, `Usages of ${word} (${refs.length})`);
  }

  async function runPeekDefinition(editor) {
    await editor.getAction('editor.action.peekDefinition')?.run();
  }

  function installHoverViewportFix(ed) {
    const root = document.getElementById('editor-overflow-root');
    const editorBox = document.getElementById('editor-container');
    if (!root || !editorBox || !ed) return;

    let raf = 0;
    let lastMaxHeight = '';
    let lastTop = '';

    const clampHover = () => {
      raf = 0;
      const hover = root.querySelector('.monaco-hover');
      if (!hover) {
        lastMaxHeight = '';
        lastTop = '';
        return;
      }

      const statusBar = document.querySelector('.ij-statusbar');
      const ceiling = Math.min(
        editorBox.getBoundingClientRect().bottom,
        statusBar?.getBoundingClientRect().top ?? window.innerHeight,
      ) - 8;
      const rootRect = root.getBoundingClientRect();
      const hoverRect = hover.getBoundingClientRect();
      const statusH = parseFloat(getComputedStyle(document.documentElement)
        .getPropertyValue('--ij-status-h')) || 22;
      const maxByViewport = Math.min(520, window.innerHeight - statusH - 56);
      const maxBySpace = Math.max(160, ceiling - hoverRect.top);
      const newMaxHeight = `${Math.min(maxByViewport, maxBySpace)}px`;

      let newTop = hover.style.top;
      if (hoverRect.bottom > ceiling) {
        const currentTop = parseFloat(hover.style.top);
        const baseTop = Number.isFinite(currentTop)
          ? currentTop
          : hoverRect.top - rootRect.top;
        newTop = `${Math.max(4, baseTop - (hoverRect.bottom - ceiling))}px`;
      }

      if (newMaxHeight !== lastMaxHeight) {
        hover.style.maxHeight = newMaxHeight;
        lastMaxHeight = newMaxHeight;
      }
      if (hoverRect.bottom > ceiling && newTop !== lastTop) {
        hover.style.top = newTop;
        lastTop = newTop;
      } else if (hoverRect.bottom <= ceiling) {
        lastTop = hover.style.top;
      }
    };

    const schedule = () => {
      if (!root.querySelector('.monaco-hover')) return;
      if (raf) return;
      raf = requestAnimationFrame(clampHover);
    };

    // Only when hover mounts/unmounts — not Monaco's per-frame style mutations (CPU hog).
    new MutationObserver((mutations) => {
      for (const m of mutations) {
        if (m.type !== 'childList') continue;
        for (const node of m.addedNodes) {
          if (node.nodeType === 1 && node.classList?.contains('monaco-hover')) {
            schedule();
            return;
          }
        }
        if (!root.querySelector('.monaco-hover')) {
          lastMaxHeight = '';
          lastTop = '';
        }
      }
    }).observe(root, { childList: true, subtree: true });

    ed.onDidScrollChange(schedule);
    window.addEventListener('resize', schedule, { passive: true });
  }

  function setupEditorFeatures(editor, helpers) {
    const origGetActivePath = helpers.getActivePath;
    helpers = {
      ...helpers,
      getActivePath() {
        const tab = origGetActivePath?.() || '';
        if (tab) return tab;
        const model = (helpers.getEditor?.() || editor)?.getModel?.();
        return model ? resolveEditorPath({ getActivePath: origGetActivePath }, model) : '';
      },
    };
    editor._reaperHelpers = helpers;

    completionDebug(helpers, ['debug active', `build=${document.querySelector('meta[name="reaper-ui-build"]')?.content?.trim() || '?'}`, 'rev=342']);

    installHoverViewportFix(editor);

    try { registerGroovy(); } catch (e) { console.warn('[ReaperLang] registerGroovy failed', e); }
    try { registerMakefile(); } catch (e) { console.warn('[ReaperLang] registerMakefile failed', e); }

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
      const ed = activeEditor();
      if (ed) ed._reaperMemberFallbackNavigated = false;
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

    function memberFallbackTypeLabel(item) {
      const detail = memberItemDetail(item).trim();
      if (!detail) return '';
      const paren = detail.indexOf('(');
      if (paren > 0) {
        const head = detail.slice(0, paren).trim();
        const lastSpace = head.lastIndexOf(' ');
        const typePart = lastSpace >= 0 ? head.slice(0, lastSpace) : head;
        const dot = typePart.lastIndexOf('.');
        return dot >= 0 ? typePart.slice(dot + 1) : typePart;
      }
      const dot = detail.lastIndexOf('.');
      const short = dot >= 0 ? detail.slice(dot + 1) : detail;
      return short.length > 28 ? `${short.slice(0, 26)}…` : short;
    }

    function memberKindLabel(kindKey) {
      if (kindKey === 'method') return 'Method';
      if (kindKey === 'field') return 'Field';
      return 'Member';
    }

    function updateMemberFallbackDocTitle(item, kindKey) {
      if (!memberFallbackEl || !item) return;
      const titleEl = memberFallbackEl.querySelector('.reaper-member-suggest-doc-title');
      const pillEl = memberFallbackEl.querySelector('.reaper-member-suggest-doc-kind');
      const label = memberFallbackLabel(item);
      if (titleEl) titleEl.textContent = label;
      if (pillEl) {
        pillEl.textContent = memberKindLabel(kindKey);
        pillEl.className = `reaper-member-suggest-doc-kind reaper-java-hover-kind-pill ${kindKey}`;
      }
    }

    async function updateMemberFallbackDoc() {
      if (!memberFallbackEl) return;
      const sigEl = memberFallbackEl.querySelector('.reaper-member-suggest-doc-sig');
      const bodyEl = memberFallbackEl.querySelector('.reaper-member-suggest-doc-body');
      const item = memberFallbackItems[memberFallbackIndex];
      if (!item || !bodyEl) return;
      const kindKey = memberKindKey(item);
      const label = memberFallbackLabel(item);
      updateMemberFallbackDocTitle(item, kindKey);
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

      const header = document.createElement('div');
      header.className = 'reaper-member-suggest-header';
      header.innerHTML = `<span class="reaper-member-suggest-header-title">Suggestions</span>`
        + `<span class="reaper-member-suggest-header-count">${visibleItems.length}</span>`
        + `<span class="reaper-member-suggest-header-hint">↑↓ navigate · ↵ accept · Esc dismiss</span>`;

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
        const typeLabel = memberFallbackTypeLabel(item);
        if (typeLabel) {
          const typeEl = document.createElement('span');
          typeEl.className = 'reaper-member-suggest-type';
          typeEl.textContent = typeLabel;
          typeEl.title = memberItemDetail(item);
          row.appendChild(icon);
          row.appendChild(name);
          row.appendChild(typeEl);
        } else {
          row.appendChild(icon);
          row.appendChild(name);
        }
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
      const docTitle = document.createElement('span');
      docTitle.className = 'reaper-member-suggest-doc-title';
      docTitle.textContent = memberFallbackLabel(visibleItems[0]);
      const docKind = document.createElement('span');
      docKind.className = `reaper-member-suggest-doc-kind reaper-java-hover-kind-pill ${memberKindKey(visibleItems[0])}`;
      docKind.textContent = memberKindLabel(memberKindKey(visibleItems[0]));
      docHead.appendChild(docTitle);
      docHead.appendChild(docKind);
      const sigEl = document.createElement('div');
      sigEl.className = 'reaper-member-suggest-doc-sig reaper-java-hover-sig';
      const bodyEl = document.createElement('div');
      bodyEl.className = 'reaper-member-suggest-doc-body reaper-java-hover-doc';
      docPanel.appendChild(docHead);
      docPanel.appendChild(sigEl);
      docPanel.appendChild(bodyEl);

      panel.appendChild(listEl);
      panel.appendChild(docPanel);
      memberFallbackEl.appendChild(header);
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
      const content = editorContent(activeEditor(), model);
      {
        const tok = prefix || '';
        if (isModifierLeadInFreeTyping(path, linePrefix, tok)) return false;
      }
      if (shouldPauseInlineAtCursor(path, linePrefix, content, position.lineNumber, position.column)) {
        return false;
      }
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

    /**
     * Brief Escape cool-down only — popup may auto-open again while typing.
     * Suggestions never force-accept; Tab accepts, Space/letters type normally.
     */
    function completionEscapedRecently(ed) {
      return !!(ed?._reaperSuggestEscapedUntil && Date.now() < ed._reaperSuggestEscapedUntil);
    }

    function clearEditorFreeTyping(ed) {
      if (!ed) return;
      ed._reaperFreeTyping = false;
      ed._reaperFreeTypingLine = null;
      ed._reaperSuggestEscapedUntil = 0;
      ed._reaperMemberFallbackNavigated = false;
    }

    function isEditorFreeTyping() {
      // Kept for call sites: we no longer latch a mode that kills autocomplete.
      // Typing always reaches the editor; only Tab accepts suggestions.
      return false;
    }

    function syncDeclarationFreeTyping() {
      /* no-op: autocomplete may auto-popup; it must not force-accept */
    }

    /** Escape: dismiss suggest/AI ghosts and refocus — keep typing, popup can return later. */
    function escapeEditorCompletionUi(ed) {
      if (!ed) return false;
      clearTimeout(ed._reaperSuggestTimer);
      clearTimeout(ed._reaperMemberSuggestTimer);
      clearTimeout(ed._reaperInlinePauseTimer);
      ed._reaperSuggestTimer = null;
      ed._reaperMemberSuggestTimer = null;
      ed._reaperInlinePauseTimer = null;
      // Brief pause so Escape isn't immediately undone by the same keystroke's pause timer.
      ed._reaperSuggestEscapedUntil = Date.now() + 400;
      cancelAiInlineFetch();
      dismissSuggestUi(ed);
      dismissInlineGhost(ed);
      ed._reaperMemberFallbackNavigated = false;
      try {
        ed.trigger('keyboard', 'closeParameterHints', null);
      } catch {
        /* best-effort */
      }
      ed.focus();
      return true;
    }

    function completionUiBlocksTyping(ed) {
      if (!ed) return false;
      if (memberFallbackEl) return true;
      const ctx = ed._contextKeyService;
      // Only the Monaco suggest widget / custom member popup — not inline ghost text.
      // Treating inlineSuggestionVisible as blocking stole `:`, `(`, etc. and broke
      // auto-closing brackets / YAML colon typing in Java and YAML.
      return !!ctx?.getContextKeyValue('suggestWidgetVisible');
    }

    function editorIsTypingTarget(ed) {
      if (!ed) return false;
      if (ed.hasTextFocus?.()) return true;
      const root = ed.getDomNode?.();
      return !!(root && root.contains(document.activeElement));
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

      // Do not intercept merely because the caret follows an identifier, or because
      // an inline ghost is visible — that stalled the caret after `:` / `()` in Java/YAML.
      if (!completionUiBlocksTyping(ed)) return;

      // Space / punctuation / brackets: dismiss the popup so it cannot swallow the
      // key, but NEVER preventDefault — typing must always reach the editor.
      // (Manual insert via preventDefault+executeEdits was dropping Space after
      // `var name` / `String value` when suggest/fallback was open.)
      if (
        e.key === ' '
        || e.key === ':'
        || e.key === '('
        || e.key === ')'
        || e.key === '['
        || e.key === ']'
        || e.key === '{'
        || e.key === '}'
        || (e.key.length === 1 && /[^a-zA-Z0-9_$]/.test(e.key))
      ) {
        dismissSuggestUi(ed);
        dismissInlineGhost(ed);
        return;
      }
    }

    function onMemberFallbackKeydown(e) {
      if (!memberFallbackEl) return;
      const ed = activeEditor();
      const model = ed?.getModel();
      const position = ed?.getPosition();
      const path = helpers.getActivePath?.() || '';
      const linePrefix = model && position ? editorLinePrefix(model, position) : '';
      const content = model ? editorContent(ed, model) : '';
      if (
        e.key.length === 1
        && !e.ctrlKey
        && !e.metaKey
        && !e.altKey
        && shouldPauseInlineAtCursor(path, linePrefix, content, position?.lineNumber, position?.column)
      ) {
        hideMemberSuggestFallback();
        dismissMonacoSuggestWidget(ed);
        return;
      }
      // Space / punctuation: close fallback so it cannot block the key; let it type.
      if (e.key === ' ' || (e.key.length === 1 && /[^a-zA-Z0-9_$]/.test(e.key))) {
        hideMemberSuggestFallback();
        dismissMonacoSuggestWidget(ed);
        return;
      }
      if (e.key === 'ArrowLeft' || e.key === 'ArrowRight' || e.key === 'Home' || e.key === 'End') {
        hideMemberSuggestFallback();
        return;
      }
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        e.stopImmediatePropagation();
        escapeEditorCompletionUi(ed);
        return;
      }
      if (e.key === 'Tab') {
        hideMemberSuggestFallback();
        return;
      }
      if (e.key === 'ArrowDown') {
        ed._reaperMemberFallbackNavigated = true;
        memberFallbackIndex = (memberFallbackIndex + 1) % memberFallbackItems.length;
        focusMemberFallbackRow();
        e.preventDefault();
        e.stopPropagation();
        return;
      }
      if (e.key === 'ArrowUp') {
        ed._reaperMemberFallbackNavigated = true;
        memberFallbackIndex = (memberFallbackIndex - 1 + memberFallbackItems.length)
          % memberFallbackItems.length;
        focusMemberFallbackRow();
        e.preventDefault();
        e.stopPropagation();
        return;
      }
      // Enter only accepts after the user arrow-navigated — otherwise keep typing
      // (newline). Pre-focused row 0 must not feel like autocorrect.
      if (e.key === 'Enter') {
        if (ed._reaperMemberFallbackNavigated) {
          acceptMemberFallbackItem(ed);
          e.preventDefault();
          e.stopPropagation();
        } else {
          hideMemberSuggestFallback();
          dismissMonacoSuggestWidget(ed);
        }
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

    function cursorMoveFromMouse(e) {
      return e?.source === 'mouse';
    }

    function dismissCompletionOnCursorClick(ed) {
      cancelAiInlineFetch();
      clearTimeout(ed._reaperInlinePauseTimer);
      ed._reaperInlinePauseTimer = null;
      dismissInlineGhost(ed);
      dismissSuggestUi(ed);
    }

    function onEditorEscapeKeydown(e) {
      if (e.key !== 'Escape' || e.ctrlKey || e.metaKey || e.altKey) return;
      const ed = activeEditor();
      if (!ed || !editorIsTypingTarget(ed)) return;
      if (!escapeEditorCompletionUi(ed)) return;
      e.preventDefault();
      e.stopPropagation();
      e.stopImmediatePropagation();
    }

    document.addEventListener('keydown', onCompletionTypeThroughKeydown, true);
    document.addEventListener('keydown', onMemberFallbackKeydown, true);
    document.addEventListener('keydown', onEditorEscapeKeydown, true);
    document.addEventListener('mousedown', (e) => {
      if (!memberFallbackEl) return;
      if (memberFallbackEl.contains(e.target)) return;
      hideMemberSuggestFallback();
    }, true);
    editor.onDidChangeModelContent(() => {
      if (!memberFallbackEl) return;
      refreshSuggestFallbackFilter();
    });
    editor.onDidChangeCursorPosition((e) => {
      if (!memberFallbackEl) return;
      if (cursorMoveFromMouse(e)) {
        hideMemberSuggestFallback();
        return;
      }
      refreshSuggestFallbackFilter();
    });
    editor.onDidBlurEditorWidget(() => {
      hideMemberSuggestFallback();
      cancelAiInlineFetch();
      dismissInlineGhost(editor);
    });

    let lastMemberSuggestKey = '';
    let lastMemberSuggestAt = 0;

    function fireCompletionsSuggest(ed, { force = false } = {}) {
      const model = ed.getModel();
      const position = ed.getPosition();
      if (!model || !position) return;
      const path = helpers.getActivePath?.() || '';
      const { linePrefix, prefix } = completionContext(model, position);
      if (!force && completionEscapedRecently(ed)) return;
      if (!path) {
        if (force) helpers.toast?.('Open a file first', 'info');
        return;
      }
      if (force) {
        ed._reaperSuggestEscapedUntil = 0;
        clearEditorFreeTyping(ed);
      }
      const repo = helpers.getRepo?.();

      const memberContext = dotQualifierFromLinePrefix(linePrefix);
      const completionPrefix = memberContext
        ? (memberContext.memberPrefix || prefix || '')
        : (prefix || '');
      const content = editorContent(ed, model);
      if (
        !force
        && shouldPauseInlineAtCursor(path, linePrefix, content, position.lineNumber, position.column)
      ) {
        return;
      }
      // Finished modifier — never open index popup.
      if (
        !memberContext
        && isModifierLeadInFreeTyping(path, linePrefix, completionPrefix)
      ) {
        dismissSuggestUi(ed);
        dismissInlineGhost(ed);
        return;
      }
      if (
        !force
        && !memberContext
        && !shouldFetchIndexCompletions(linePrefix, completionPrefix, path)
      ) {
        return;
      }
      if (
        !memberContext
        && shouldPreferModifierKeywordInline(path, linePrefix, completionPrefix)
      ) {
        // Partial modifier only — keyword list, never index fetch.
        const mods = [];
        const lower = String(completionPrefix || '').toLowerCase();
        const { range } = completionContext(model, position);
        for (const kw of modifierKeywordsForPath(path) || []) {
          if (kw.startsWith(lower) && kw.length > completionPrefix.length) {
            mods.push({
              label: kw,
              kind: completionKind('keyword'),
              detail: 'keyword',
              insertText: kw,
              range,
              sortText: `0_${kw}`,
            });
          }
        }
        if (mods.length) showMemberSuggestFallback(ed, mods);
        else dismissSuggestUi(ed);
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
          mergeIndexItemsIntoSuggestions(items, range, seen, memberContext, fromIndex, 80, path);
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
      if (!force && completionEscapedRecently(ed)) return;
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
      } else {
        const memberPrefix = memberContext.memberPrefix || '';
        void fetchCompletionsWithTimeout(
          helpers, model, position, memberPrefix, COMPLETION_FETCH_TIMEOUT_MS,
        ).then((items) => {
          if (!items?.length || activeEditor() !== ed) return;
          presentCompletionSuggestions(ed, items, { content, path });
        }).catch(() => {});
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
        return lookupDefinition(helpers, model, position).then((loc) => {
          if (loc) {
            markDefinitionNavigation();
          } else {
            reportNoDefinition(helpers);
          }
          return loc;
        });
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
        .filter((m) => positionInMarker(position, m));
    }

    function enrichSymInfoFromCLikeBuiltin(symInfo, cLikeInfo) {
      if (!symInfo?.name || !cLikeInfo) return symInfo;
      if (symInfo.name !== cLikeInfo.name && symInfo.name !== cLikeInfo.name.split('::').pop()) {
        return symInfo;
      }
      const hasDoc = !!(symInfo.documentation || symInfo.signature);
      if (hasDoc) return symInfo;
      return {
        ...symInfo,
        kind: symInfo.kind || cLikeInfo.kind,
        signature: symInfo.signature || cLikeInfo.signature,
        documentation: symInfo.documentation || cLikeInfo.documentation,
      };
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

    monaco.languages.registerHoverProvider({ language: 'cpp' }, {
      provideHover(model, position) {
        return provideClikeHover(model, position);
      },
    });
    monaco.languages.registerHoverProvider({ language: 'c' }, {
      provideHover(model, position) {
        return provideClikeHover(model, position);
      },
    });

    function provideClikeHover(model, position) {
      const range = wordRangeAt(model, position);
      const path = resolveEditorPath(helpers, model);
      if (!isCLikeContext(path, model)) return null;
      const markers = markersAtEditorPosition(model, position);
      const cLikeInfo = lookupCLikeHover(helpers, model, position);
      if (cLikeInfo) {
        return clikeHoverResult(cLikeInfo, markers, range);
      }
      const cLikeLocal = lookupCLikeLocalSymbol(model, position, path);
      if (cLikeLocal) {
        return clikeHoverResult(cLikeLocal, markers, range);
      }
      if (markers.length) {
        return hoverResultForMarkers(model, position, markers, range);
      }
      return lookupSymbolHover(helpers, model, position).then((symInfo) => {
        if (!symInfo?.name) return null;
        const enriched = enrichSymInfoFromCLikeBuiltin(symInfo, null);
        return clikeHoverResult(enriched, [], enriched?.range || range);
      });
    }

    monaco.languages.registerHoverProvider(REAPER_DOC_SELECTOR, {
      provideHover(model, position) {
        const range = wordRangeAt(model, position);
        const path = resolveEditorPath(helpers, model);
        const lang = model.getLanguageId();
        const markers = markersAtEditorPosition(model, position);

        const attachDebugValue = async (hoverResult) => {
          if (!helpers.isDebugStopped?.() || !helpers.lookupDebugHoverValue) {
            return hoverResult;
          }
          let info = null;
          try {
            info = await helpers.lookupDebugHoverValue(model, position);
          } catch {
            info = null;
          }
          // Back-compat: older helper returned a plain string.
          if (typeof info === 'string') {
            const word = model.getWordAtPosition(position);
            info = word?.word ? { label: word.word, value: info } : null;
          }
          if (!info?.value) return hoverResult;
          const label = String(info.label || '').replace(/`/g, '\\`');
          const safe = String(info.value).replace(/`/g, '\\`');
          const debugLine = {
            value: label ? `**${label}** = \`${safe}\`` : `\`${safe}\``,
          };
          const word = model.getWordAtPosition(position);
          const wordRange = word
            ? new monaco.Range(
              position.lineNumber,
              word.startColumn,
              position.lineNumber,
              word.endColumn,
            )
            : range;
          if (!hoverResult) {
            return { range: wordRange, contents: [debugLine] };
          }
          return {
            ...hoverResult,
            range: hoverResult.range || wordRange,
            contents: [debugLine, ...(hoverResult.contents || [])],
          };
        };

        // C/C++ use dedicated providers for docs; still attach debug values here.
        if (lang === 'cpp' || lang === 'c') {
          return attachDebugValue(null);
        }

        if (langForPath(path) === 'sql' && model.getLanguageId() === 'sql') {
          if (markers.length) {
            return attachDebugValue(hoverResultForMarkers(model, position, markers, range));
          }
          const sqlInfo = lookupSqlHover(helpers, model, position);
          if (!sqlInfo) return attachDebugValue(null);
          const html = hoverHtmlFromInfo(sqlInfo);
          if (!html) return attachDebugValue(null);
          return attachDebugValue({
            range: sqlInfo.range || range,
            contents: [{
              value: html,
              supportHtml: true,
              isTrusted: true,
            }],
          });
        }

        // Merge symbol docs with diagnostics when both apply (Java, etc.).
        if (markers.length) {
          return lookupSymbolHover(helpers, model, position).then((symInfo) => {
            if (hoverInfoHasContent(symInfo)) {
              const html = buildEditorHoverHtml(symInfo, markers);
              if (html) {
                return attachDebugValue({
                  range,
                  contents: [{
                    value: html,
                    supportHtml: true,
                    isTrusted: true,
                  }],
                });
              }
            }
            return attachDebugValue(hoverResultForMarkers(model, position, markers, range));
          });
        }

        return lookupSymbolHover(helpers, model, position).then((symInfo) => {
          if (!hoverInfoHasContent(symInfo)) return attachDebugValue(null);
          const html = buildEditorHoverHtml(symInfo, []);
          if (!html) {
            const md = hoverMarkdownFromInfo(symInfo);
            if (!md) return attachDebugValue(null);
            return attachDebugValue({
              range,
              contents: [{ value: md, isTrusted: true }],
            });
          }
          return attachDebugValue({
            range,
            contents: [{
              value: html,
              supportHtml: true,
              isTrusted: true,
            }],
          });
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
      if (fix?.provider === 'local' || fix?.provider === 'jdtls') return title;
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
          const fqcn = localJavaImportFqcn(symbol);
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
      return fixes.some((f) => f.provider && f.provider !== 'local' && f.provider !== 'loading');
    }

    function aiProvidersConfigured() {
      return !!(
        helpers.getGeminiConfigured?.()
        || helpers.getCursorConfigured?.()
        || helpers.getCursorInlineAvailable?.()
        || helpers.getAnthropicConfigured?.()
        || helpers.getBedrockConfigured?.()
      );
    }

    function toastNoQuickFixes(reason) {
      if (aiProvidersConfigured()) {
        helpers.toast?.(
          reason || 'No quick fixes for this error — Cursor/Gemini returned nothing useful. Try again or check the bridge.',
          'info',
        );
      } else {
        helpers.toast?.(
          'No quick fixes available — configure Cursor or Gemini in Settings → AI',
          'error',
        );
      }
    }

    const QUICK_FIX_LOADING = Object.freeze({
      title: 'Fetching AI fixes…',
      provider: 'loading',
    });

    function presentQuickFixes(ed, fixes, {
      alwaysMenu = false,
      anchorEl = null,
      markers = null,
      line = null,
    } = {}) {
      const model = ed.getModel();
      const scoped = model && markers
        ? sanitizeFixList(model, fixes || [], markers)
        : (fixes || []).filter((f) => f.edits?.length);
      const list = scoped;
      const actionable = list.filter((f) => f.edits?.length);
      const pending = (fixes || []).some((f) => f.provider === 'loading');

      if (!actionable.length && !pending) {
        toastNoQuickFixes();
        return;
      }
      const showMenu = alwaysMenu
        || actionable.length > 1
        || hasAiQuickFixes(actionable)
        || pending;
      if (!showMenu && actionable.length === 1) {
        if (applyQuickFixEdits(ed, actionable[0], markers)) {
          helpers.toast?.(`Applied: ${quickFixActionTitle(actionable[0])}`, 'success');
          (helpers.refreshDiagnosticsAfterFix || helpers.scheduleDiagnostics)?.();
        } else {
          helpers.toast?.('Could not apply fix — try again', 'error');
        }
        return;
      }
      const menuItems = pending
        ? [...actionable, ...(fixes || []).filter((f) => f.provider === 'loading')]
        : actionable;
      const anchorLine = line
        ?? markers?.[0]?.startLineNumber
        ?? ed.getPosition()?.lineNumber
        ?? null;
      helpers.showQuickFixMenu?.(menuItems, (fix) => {
        if (applyQuickFixEdits(ed, fix, markers)) {
          helpers.toast?.(`Applied: ${quickFixActionTitle(fix)}`, 'success');
          (helpers.refreshDiagnosticsAfterFix || helpers.scheduleDiagnostics)?.();
        } else {
          helpers.toast?.('Could not apply fix', 'error');
        }
      }, anchorEl, anchorLine);
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
      const presentOpts = { alwaysMenu, anchorEl, markers, line };

      if (fixes.length) {
        presentQuickFixes(ed, needsAiFetch ? [...fixes, QUICK_FIX_LOADING] : fixes, presentOpts);
      } else if (needsAiFetch) {
        presentQuickFixes(ed, [QUICK_FIX_LOADING], presentOpts);
      } else {
        presentQuickFixes(ed, fixes, presentOpts);
        return;
      }

      if (!needsAiFetch) return;

      const allMarkers = allFileMarkers(model);
      fetchQuickFixes(
        model, allMarkers.length ? allMarkers : markers, {
          silent: true,
          scopeMarkers: markers,
          forceRefresh: true,
        },
      ).then((aiFixes) => {
        const merged = mergeQuickFixes(fixes, aiFixes);
        const menuOpen = helpers.isQuickFixMenuOpen?.();
        const hadInitialFixes = fixes.length > 0;

        if (merged.length && (menuOpen || !hadInitialFixes)) {
          presentQuickFixes(ed, merged, presentOpts);
        } else if (!merged.length && !hadInitialFixes) {
          helpers.hideQuickFixMenu?.();
          toastNoQuickFixes(
            aiProvidersConfigured()
              ? 'Cursor/Gemini are configured, but no fix was returned for this error'
              : null,
          );
        } else if (menuOpen && !merged.length && hadInitialFixes) {
          // Drop the loading row; keep local fixes.
          presentQuickFixes(ed, fixes, presentOpts);
        }
      }).catch((err) => {
        if (!fixes.length) {
          helpers.hideQuickFixMenu?.();
          helpers.toast?.(err?.message || 'AI quick fix failed', 'error');
        } else if (helpers.isQuickFixMenuOpen?.()) {
          presentQuickFixes(ed, fixes, presentOpts);
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
      forceRefresh = false,
    } = {}) {
      const repo = helpers.getRepo();
      const path = helpers.getActivePath?.() || '';
      if (!repo || !path || !helpers.repoApi || !markers.length) return [];
      const content = model.getValue();
      const payload = markersToDiagnosticPayload(markers);
      const key = quickFixCacheKey(repo, path, content.length, diagnosticSignature(payload));
      const anchorMarkers = scopeMarkers || markers;
      if (forceRefresh) {
        quickFixCache.delete(key);
      } else {
        const cached = readCachedQuickFixes(key, model, anchorMarkers);
        // Empty cache from a silent prefetch must not block a user click when AI is configured.
        if (cached !== null) {
          if (cached.length > 0 || silent || !aiProvidersConfigured()) return cached;
          quickFixCache.delete(key);
        }
      }
      const inflight = quickFixInflight.get(key);
      if (inflight && !forceRefresh) return inflight;

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
    const AI_INLINE_DEBOUNCE_MS = 300;
    const AI_INLINE_STATEMENT_DEBOUNCE_MS = 150;
    const INLINE_PAUSE_MS = 200;
    const EMPTY_LINE_PAUSE_MS = 100;
    const EMPTY_LINE_AI_DEBOUNCE_MS = 120;
    const INLINE_LOCAL_FETCH_MS = 180;
    const INLINE_AI_FETCH_MS = 18000;
    let aiInlineSeq = 0;
    let aiInlinePendingKey = '';
    const aiCompleteCache = { key: '', items: [], time: 0 };
    let inlineShowTimer = null;

    function editorAcceptsInlineAi(ed) {
      if (typeof document !== 'undefined' && document.visibilityState === 'hidden') return false;
      return editorIsTypingTarget(ed);
    }

    function cancelAiInlineFetch() {
      clearTimeout(aiInlineTimer);
      aiInlineTimer = null;
      aiInlinePendingKey = '';
      aiInlineSeq += 1;
    }

    function queueInlineSuggestion(ed, { emptyLine = false, soft = false } = {}) {
      clearTimeout(inlineShowTimer);
      inlineShowTimer = setTimeout(() => {
        const paint = () => {
          ed.trigger('reaper', 'editor.action.inlineSuggest.trigger', {});
        };
        if (emptyLine && !soft) {
          requestAnimationFrame(paint);
          if (aiInlineCache.text) {
            clearTimeout(ed._reaperEmptyInlinePaintTimer);
            ed._reaperEmptyInlinePaintTimer = setTimeout(() => {
              const visible = ed._contextKeyService?.getContextKeyValue('inlineSuggestionVisible');
              if (!visible) paint();
            }, 32);
          }
        } else {
          requestAnimationFrame(paint);
        }
      }, 12);
    }

    function aiInlineFetchEnabled() {
      return !!helpers.getAiInlineComplete?.() && !!helpers.getAiInlineProviderAvailable?.();
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

    function setInlineCache(ed, cacheKey, text, controlSnippet, meta, options = {}) {
      const path = meta?.path ?? (helpers.getActivePath?.() || '');
      const linePrefix = meta?.linePrefix ?? '';
      const clean = capInlineText(sanitizeInlineGhostText(text));
      const snippet = controlSnippet || '';
      const content = meta?.content ?? ed?._reaperContent ?? ed?.getModel?.()?.getValue?.() ?? '';
      const lineNumber = meta?.lineNumber ?? 0;
      const column = meta?.column ?? 0;
      if (
        !clean
        || shouldSuppressInlineGhost(path, linePrefix, clean, content, lineNumber, column)
      ) {
        return;
      }
      if (
        aiInlineCache.key === cacheKey
        && aiInlineCache.text === clean
        && aiInlineCache.controlSnippet === snippet
      ) {
        if (options.repaint) {
          queueInlineSuggestion(ed, { emptyLine: isWhitespaceOnlyLine(linePrefix) });
        }
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
      if (options.refresh !== false) {
        queueInlineSuggestion(ed, { emptyLine: isWhitespaceOnlyLine(linePrefix) });
      }
    }

    function acceptInlineOrControlSnippet(ed, { newlineOnRedundantBrace = false } = {}) {
      const model = ed.getModel();
      const position = ed.getPosition();
      if (!model || !position) return;

      const linePrefix = editorLinePrefix(model, position);
      const cacheKey = inlineCacheKey(model, position, linePrefix);
      const content = editorContent(ed, model);
      const pending = ed._reaperPendingControlSnippet;
      const insertRange = new monaco.Range(
        position.lineNumber, position.column, position.lineNumber, position.column,
      );

      const clearInline = () => {
        clearInlineCache(ed);
      };

      const ghost = aiInlineCache.key === cacheKey ? aiInlineCache.text : '';
      if (
        isRedundantCloseBraceGhost(linePrefix, ghost || '}', content, position.lineNumber, position.column)
        && (!ghost || ghost.trim() === '}')
      ) {
        dismissInlineGhost(ed);
        if (newlineOnRedundantBrace) {
          ed.trigger('keyboard', 'type', { text: '\n' });
        } else {
          ed.trigger('reaper', 'tab', null);
        }
        return;
      }

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
      const ctx = ed._contextKeyService;
      if (ctx?.getContextKeyValue('inSnippetMode')) {
        ed.trigger('reaper', 'jumpToNextSnippetPlaceholder', null);
        return;
      }
      // Tab is the explicit accept key for suggest / AI ghosts.
      if (memberFallbackEl) {
        acceptMemberFallbackItem(ed);
        return;
      }
      if (ctx?.getContextKeyValue('suggestWidgetVisible')) {
        try {
          ed.trigger('reaper', 'selectNextSuggestion', null);
        } catch {
          /* older Monaco */
        }
        ed.trigger('reaper', 'acceptSelectedSuggestion', null);
        return;
      }
      if (hasActiveInlineSuggestion(ed)) {
        acceptInlineOrControlSnippet(ed, { newlineOnRedundantBrace: false });
        return;
      }
      // Nothing to accept — normal indent.
      ed.trigger('reaper', 'tab', null);
    }

    function shouldAutoOpenSuggest(model, position, path, content) {
      if (completionEscapedRecently(activeEditor())) return false;
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
      if (p.length === 1 && isKeywordPrefixTyping(path || helpers.getActivePath?.() || '', p)) {
        return true;
      }
      return false;
    }

    // Local + index completions — member access returns instantly (no loading spinner).
    monaco.languages.registerCompletionItemProvider(REAPER_DOC_SELECTOR, {
      triggerCharacters: ['.', '@', ':', '-', '<', '"', "'", '/', '#', '*', '=', '(', '[', '{'],
      provideCompletionItems(model, position, context) {
        const path = helpers.getActivePath?.() || '';
        if (!path || !editorScopeMatches(path, model)) {
          completionDebug(helpers, ['provider', completionTriggerLabel(context), 'skip: language scope']);
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
        const content = editorContent(activeEditor(), model);
        if (
          !manual
          && shouldPauseInlineAtCursor(path, linePrefix, content, position.lineNumber, position.column)
        ) {
          return { suggestions: [], incomplete: false };
        }
        if (!memberContext && !shouldFetchIndexCompletions(linePrefix, completionPrefix, path) && !manual) {
          completionDebug(helpers, [
            'provider', completionTriggerLabel(context), 'skip: fetch gate',
            `line=…${linePrefix.slice(-24)}`,
          ]);
          return { suggestions: [], incomplete: false };
        }

        const ed = activeEditor();
        const modifierTyping = !memberContext
          && shouldPreferModifierKeywordInline(path, linePrefix, completionPrefix);

        // Finished modifier — close popup so the next word types freely.
        // Declarations still show suggest; Space never accepts (see keydown handlers).
        if (
          !manual
          && !memberContext
          && modifierTyping
          && isModifierLeadInFreeTyping(path, linePrefix, completionPrefix)
        ) {
          if (ed) {
            dismissSuggestUi(ed);
            dismissInlineGhost(ed);
          }
          completionDebug(helpers, [
            'provider', completionTriggerLabel(context), 'modifier free typing',
            path.split('/').pop(),
          ]);
          return { suggestions: [], incomplete: false };
        }

        // Partial modifier (`priv`) — only modifier keywords, never index types.
        if (modifierTyping && !manual) {
          const mods = [];
          const lower = String(completionPrefix || '').toLowerCase();
          for (const kw of modifierKeywordsForPath(path) || []) {
            if (kw.startsWith(lower) && kw.length > completionPrefix.length) {
              mods.push({
                label: kw,
                kind: completionKind('keyword'),
                detail: 'keyword',
                insertText: kw,
                range,
                sortText: `0_${kw}`,
              });
            }
          }
          if (mods.length > 0 && ed) {
            presentCompletionSuggestions(ed, mods, { content, path });
          } else if (ed) {
            dismissSuggestUi(ed);
          }
          completionDebug(helpers, [
            'provider', completionTriggerLabel(context), 'modifier keyword only',
            `n=${mods.length}`,
            path.split('/').pop(),
          ]);
          return { suggestions: [], incomplete: false };
        }

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
          mergeIndexItemsIntoSuggestions(cached, range, seen, memberContext, suggestions, 80, path);
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
                  mergeIndexItemsIntoSuggestions(items, range, idxSeen, memberContext, fromIndex, 80, path);
                  if (fromIndex.length > 0) {
                    const merged = suggestions.length > 0
                      ? [...suggestions, ...fromIndex.filter((i) => !suggestions.some((s) => s.label === i.label))]
                      : fromIndex;
                    presentCompletionSuggestions(ed, merged, { content, path });
                  } else if (suggestions.length === 0) {
                    presentCompletionSuggestions(ed, items, { content, path });
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
              mergeIndexItemsIntoSuggestions(items, range, seen, memberContext, suggestions, 80, path);
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
        if (!helpers.getAiInlineComplete?.() || !helpers.getAiInlineProviderAvailable?.()) {
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

    function scheduleInlineOnPause(ed) {
      if (!editorAcceptsInlineAi(ed)) return;
      if (completionEscapedRecently(ed)) return;
      clearTimeout(ed._reaperInlinePauseTimer);
      let delay = INLINE_PAUSE_MS;
      const model = ed.getModel();
      const position = ed.getPosition();
      if (model && position) {
        const linePrefix = editorLinePrefix(model, position);
        if (isWhitespaceOnlyLine(linePrefix)) {
          delay = EMPTY_LINE_PAUSE_MS;
        }
      }
      ed._reaperInlinePauseTimer = setTimeout(() => runInlineOnPause(ed), delay);
    }

    function runInlineOnPause(ed) {
      if (!editorAcceptsInlineAi(ed)) return;
      const model = ed.getModel();
      const position = ed.getPosition();
      const repo = helpers.getRepo();
      const path = helpers.getActivePath?.() || '';
      if (!model || !position || !repo || !path) return;

      ed._reaperContent = model.getValue();
      const linePrefix = editorLinePrefix(model, position);
      const content = ed._reaperContent ?? model.getValue();
      const onEmptyMarkup = isWhitespaceOnlyLine(linePrefix) && isMarkupOrConfigPath(path);
      refreshInlineAfterEdit(ed, { light: onEmptyMarkup });

      if (helpers.repoApi && !onEmptyMarkup && shouldPrefetchInlineOnPause(path, linePrefix, content, position.lineNumber)) {
        prefetchIndexInlineGhost(ed, linePrefix, inlineIndexPrefix(linePrefix));
      }
      if (aiInlineFetchEnabled()) {
        const routeAi = shouldRouteInlineToAi(
          path, linePrefix, content, position.lineNumber, '', true,
        );
        if (
          routeAi
          && !shouldPauseInlineAtCursor(path, linePrefix, content, position.lineNumber, position.column)
        ) {
          scheduleAiInlineFetch();
        }
      }
      if (shouldPauseInlineAtCursor(path, linePrefix, content, position.lineNumber, position.column)) {
        dismissInlineGhost(ed);
        dismissSuggestUi(ed);
      } else {
        queueInlineSuggestion(ed, { emptyLine: isWhitespaceOnlyLine(linePrefix) });
      }
    }

    function scheduleEmptyLineInline(ed) {
      const model = ed.getModel();
      const position = ed.getPosition();
      const repo = helpers.getRepo();
      const path = helpers.getActivePath?.() || '';
      if (!model || !position || !repo || !path) return;
      if (!editorAcceptsInlineAi(ed)) return;
      const linePrefix = editorLinePrefix(model, position);
      if (!isWhitespaceOnlyLine(linePrefix)) return;
      const javaLevel = inlineJavaLevel(helpers);
      const content = editorContent(ed, model);
      const cacheKey = buildInlineCacheKey(repo, path, position.lineNumber, position.column, linePrefix);
      const meta = inlineCacheMeta(repo, path, position, linePrefix);
      const indexCtx = { helpers, model, position };
      const local = inlineGhostSuffix(
        path, linePrefix, content, position.lineNumber, javaLevel, position.column, indexCtx,
      );
      if (
        local
        && !shouldSuppressInlineGhost(
          path, linePrefix, local, content, position.lineNumber, position.column,
        )
      ) {
        setInlineCache(ed, cacheKey, local, '', meta);
        return;
      }
      if (aiInlineFetchEnabled()) scheduleAiInlineFetch();
    }

    /** Keystroke path: cached LSP top completion when available, else cheap local ghost. */
    function refreshInlineLocalFast(ed) {
      const model = ed.getModel();
      const position = ed.getPosition();
      const repo = helpers.getRepo();
      const path = helpers.getActivePath?.() || '';
      if (!model || !position || !repo || !path) {
        clearInlineCache(ed);
        return;
      }
      if (completionEscapedRecently(ed)) {
        clearInlineCache(ed);
        dismissSuggestUi(ed);
        return;
      }
      const linePrefix = editorLinePrefix(model, position);
      const content = editorContent(ed, model);
      const cacheKey = buildInlineCacheKey(
        repo, path, position.lineNumber, position.column, linePrefix,
      );
      {
        const tok = extractInlinePartialToken(linePrefix);
        if (isModifierLeadInFreeTyping(path, linePrefix, tok)) {
          clearInlineCache(ed);
          dismissSuggestUi(ed);
          return;
        }
      }
      if (shouldPauseInlineAtCursor(path, linePrefix, content, position.lineNumber, position.column)) {
        clearInlineCache(ed);
        return;
      }
      if (isWhitespaceOnlyLine(linePrefix)) {
        if (aiInlineCache.key === cacheKey && aiInlineCache.text) {
          queueInlineSuggestion(ed, { emptyLine: true, soft: true });
        }
        return;
      }
      const meta = inlineCacheMeta(repo, path, position, linePrefix);
      const javaLevel = inlineJavaLevel(helpers);
      const indexCtx = { helpers, model, position };
      const indexPrefix = inlineIndexPrefix(linePrefix);

      if (shouldPreferLspInlineGhost(path, linePrefix, content, position.lineNumber)
        && shouldFetchIndexCompletions(linePrefix, indexPrefix, path, {
          content,
          lineNumber: position.lineNumber,
        })) {
        const lspCached = inlineSuffixFromCachedIndex(
          helpers, model, position, linePrefix, indexPrefix,
        );
        if (
          lspCached
          && !shouldSuppressInlineGhost(
            path, linePrefix, lspCached, content, position.lineNumber, position.column,
          )
        ) {
          setInlineCache(ed, cacheKey, lspCached, '', meta, { refresh: false });
          queueInlineSuggestion(ed);
          return;
        }
      }

      const local = inlineGhostSuffix(
        path, linePrefix, content, position.lineNumber, javaLevel, position.column, indexCtx,
        { fast: true },
      );
      if (
        local
        && !shouldSuppressInlineGhost(
          path, linePrefix, local, content, position.lineNumber, position.column,
        )
      ) {
        setInlineCache(ed, cacheKey, local, '', meta, { refresh: false });
        queueInlineSuggestion(ed);
      } else {
        const tok = extractInlinePartialToken(linePrefix);
        if (isModifierLeadInFreeTyping(path, linePrefix, tok)) {
          clearInlineCache(ed);
          dismissSuggestUi(ed);
        } else if (aiInlineCache.key && aiInlineCache.key !== cacheKey) {
          clearInlineCache(ed);
        }
      }
    }

    function refreshInlineAfterEdit(ed, { light = false } = {}) {
      const model = ed.getModel();
      const position = ed.getPosition();
      const repo = helpers.getRepo();
      const path = helpers.getActivePath?.() || '';
      if (!model || !position || !repo || !path) {
        clearInlineCache(ed);
        return;
      }
      const linePrefix = editorLinePrefix(model, position);
      const cacheKey = buildInlineCacheKey(
        repo, path, position.lineNumber, position.column, linePrefix,
      );
      const meta = inlineCacheMeta(repo, path, position, linePrefix);
      const javaLevel = inlineJavaLevel(helpers);
      const content = editorContent(ed, model);
      const aiOn = aiInlineFetchEnabled();

      {
        const tok = extractInlinePartialToken(linePrefix);
        if (isModifierLeadInFreeTyping(path, linePrefix, tok)
          || shouldPauseInlineAtCursor(path, linePrefix, content, position.lineNumber, position.column)) {
          clearInlineCache(ed);
          return;
        }
      }

      if (isWhitespaceOnlyLine(linePrefix)) {
        const indexPrefix = inlineIndexPrefix(linePrefix);
        let ghost = inferEmptyLineContinuationSuffix(
          path, content, position.lineNumber, linePrefix, javaLevel,
        );
        if (!ghost) {
          ghost = inlineSuffixFromCachedIndex(
            helpers, model, position, linePrefix, indexPrefix,
          );
        }
        if (
          ghost
          && !shouldSuppressInlineGhost(
            path, linePrefix, ghost, content, position.lineNumber, position.column,
          )
        ) {
          setInlineCache(ed, cacheKey, ghost, '', meta);
        } else if (aiInlineCache.key === cacheKey && aiInlineCache.text) {
          queueInlineSuggestion(ed, { emptyLine: true, soft: true });
        } else if (aiInlineCache.key && aiInlineCache.key !== cacheKey) {
          clearInlineCache(ed);
        }
        return;
      }

      const indexCtx = { helpers, model, position };
      const preferAi = aiOn && shouldPreferAiStatementInline(
        path, linePrefix, content, position.lineNumber,
      );

      let local = inlineGhostSuffix(
        path, linePrefix, content, position.lineNumber, javaLevel, position.column, indexCtx,
        { fast: preferAi || isMarkupOrConfigPath(path) },
      );
      if (!local && isWhitespaceOnlyLine(linePrefix)) {
        local = inferEmptyLineContinuationSuffix(
          path, content, position.lineNumber, linePrefix, javaLevel,
        );
      }
      const routeAi = shouldRouteInlineToAi(
        path, linePrefix, content, position.lineNumber, local, aiOn,
      );
      if (
        local
        && !shouldSuppressInlineGhost(
          path, linePrefix, local, content, position.lineNumber, position.column,
        )
      ) {
        setInlineCache(ed, cacheKey, local, '', meta);
      } else {
        const stale = inlineGhostFromCache(
          aiInlineCache, repo, path, position.lineNumber, position.column, linePrefix, content,
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
          queueInlineSuggestion(ed, { emptyLine: isWhitespaceOnlyLine(linePrefix) });
        } else if (aiInlineCache.key && aiInlineCache.key !== cacheKey) {
          clearInlineCache(ed);
        }
      }

      if (dotQualifierFromLinePrefix(linePrefix)) {
        scheduleMemberCompletions(ed);
        return;
      }

      if (isSpringConfigFile(path)) {
        scheduleSpringConfigInline(ed);
        scheduleAutocompleteSuggest(ed);
        return;
      }

      if (!preferAi) {
        const tok = extractInlinePartialToken(linePrefix);
        if (!isModifierLeadInFreeTyping(path, linePrefix, tok)) {
          scheduleAutocompleteSuggest(ed);
        }
      }
    }

    function prefetchIndexInlineGhost(ed, linePrefix, prefix) {
      const model = ed.getModel();
      const position = ed.getPosition();
      const repo = helpers.getRepo();
      const path = helpers.getActivePath?.() || '';
      if (!repo || !path || !model || !position || !helpers.repoApi) return;
      const tokenPrefix = prefix ?? inlineIndexPrefix(linePrefix);
      if (
        !shouldFetchIndexCompletions(linePrefix, tokenPrefix, path)
        && !dotQualifierFromLinePrefix(linePrefix)
      ) {
        return;
      }
      const content = ed._reaperContent ?? model.getValue();
      const member = dotQualifierFromLinePrefix(linePrefix);
      const timeoutMs = member ? LSP_INLINE_MEMBER_FETCH_MS : LSP_INLINE_FETCH_MS;
      void fetchCompletionsWithTimeout(
        helpers, model, position, tokenPrefix, timeoutMs,
      )
        .then((items) => {
          const suffix = inlineSuffixFromIndexItems(items, linePrefix, tokenPrefix, path);
          if (
            suffix
            && !shouldSuppressInlineGhost(
              path, linePrefix, suffix, content, position.lineNumber, position.column,
            )
          ) {
            const cacheKey = buildInlineCacheKey(
              repo, path, position.lineNumber, position.column, linePrefix,
            );
            setInlineCache(
              ed,
              cacheKey,
              suffix,
              '',
              inlineCacheMeta(repo, path, position, linePrefix),
            );
            return;
          }
          if (items?.length && tokenPrefix) {
            scheduleAutocompleteSuggest(ed, true);
          }
        })
        .catch(() => {});
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

    async function fetchInlineComplete(model, position, linePrefix, localOnly = false) {
      const repo = helpers.getRepo();
      const path = helpers.getActivePath?.() || '';
      if (!repo || !path) return '';
      const payload = contentForApiPayload(editorContent(editor, model), position.lineNumber);
      const timeoutMs = localOnly ? INLINE_LOCAL_FETCH_MS : INLINE_AI_FETCH_MS;
      const request = helpers.api(helpers.repoApi(repo, '/workspace/inline-complete'), {
        method: 'POST',
        body: JSON.stringify({
          path,
          line: payload.line,
          column: position.column,
          content: payload.content,
          line_prefix: linePrefix,
          local_only: localOnly,
        }),
      }).then((res) => res?.text ?? '');
      return Promise.race([
        request,
        new Promise((resolve) => setTimeout(() => resolve(''), timeoutMs)),
      ]);
    }

    function scheduleAiInlineFetch() {
      if (!aiInlineFetchEnabled()) return;
      if (!editorAcceptsInlineAi(editor)) return;
      const model = editor.getModel();
      const position = editor.getPosition();
      const repo = helpers.getRepo();
      const path = helpers.getActivePath?.() || '';
      if (!model || !position || !repo || !path) return;
      const linePrefix = editorLinePrefix(model, position);
      if (dotQualifierFromLinePrefix(linePrefix)) {
        clearTimeout(aiInlineTimer);
        aiInlinePendingKey = '';
        return;
      }
      const content = editorContent(editor, model);
      if (!shouldRouteInlineToAi(path, linePrefix, content, position.lineNumber, '', aiInlineFetchEnabled())) {
        clearTimeout(aiInlineTimer);
        aiInlinePendingKey = '';
        return;
      }
      const cacheKey = buildInlineCacheKey(
        repo, path, position.lineNumber, position.column, linePrefix,
      );
      if (aiInlineCache.key === cacheKey && aiInlineCache.text) return;
      if (aiInlinePendingKey === cacheKey && aiInlineTimer) return;
      aiInlinePendingKey = cacheKey;
      clearTimeout(aiInlineTimer);
      const seq = ++aiInlineSeq;
      let debounceMs = AI_INLINE_DEBOUNCE_MS;
      if (model && position) {
        const path = helpers.getActivePath?.() || '';
        const linePrefix = editorLinePrefix(model, position);
        const content = editorContent(editor, model);
        if (path && isWhitespaceOnlyLine(linePrefix)) {
          debounceMs = EMPTY_LINE_AI_DEBOUNCE_MS;
        } else if (
          path && shouldPreferAiStatementInline(
            path, linePrefix, content, position.lineNumber,
          )
        ) {
          debounceMs = AI_INLINE_STATEMENT_DEBOUNCE_MS;
        }
      }
      aiInlineTimer = setTimeout(async () => {
        aiInlinePendingKey = '';
        if (seq !== aiInlineSeq) return;
        if (!editorAcceptsInlineAi(editor)) return;
        const model = editor.getModel();
        const position = editor.getPosition();
        if (!model || !position) return;
        const repo = helpers.getRepo();
        const path = helpers.getActivePath?.() || '';
        if (!repo || !path) return;
        const linePrefix = editorLinePrefix(model, position);
        if (dotQualifierFromLinePrefix(linePrefix)) return;
        if (!inlineTypingReady()) return;
        {
          const tok = extractInlinePartialToken(linePrefix);
          if (isModifierLeadInFreeTyping(path, linePrefix, tok)) return;
        }
        if (shouldPauseInlineAtCursor(path, linePrefix, editorContent(editor, model), position.lineNumber, position.column)) {
          return;
        }
        const cacheKey = buildInlineCacheKey(
          repo, path, position.lineNumber, position.column, linePrefix,
        );
        const meta = inlineCacheMeta(repo, path, position, linePrefix);
        if (aiInlineCache.key === cacheKey && aiInlineCache.text) return;
        try {
          const text = await fetchInlineComplete(model, position, linePrefix, false);
          if (!text || seq !== aiInlineSeq) return;
          setInlineCache(editor, cacheKey, text, '', meta);
        } catch {
          // Inline completion is best-effort; network errors (e.g. "Load failed") are not user-actionable.
        }
      }, debounceMs);
    }

    monaco.languages.registerInlineCompletionsProvider(ALL_EDITOR_LANGS, {
      provideInlineCompletions: async (model, position, _context, token) => {
        if (!editorAcceptsInlineAi(editor)) return { items: [] };
        const repo = helpers.getRepo();
        const path = helpers.getActivePath?.() || '';
        if (!repo || !path || !editorScopeMatches(path, model)) return { items: [] };

        const linePrefix = editorLinePrefix(model, position);
        if (!inlineTypingReady()) return { items: [] };
        const content = editorContent(editor, model);
        if (completionEscapedRecently(editor)) {
          clearInlineCache(editor);
          return { items: [] };
        }
        {
          const tok = extractInlinePartialToken(linePrefix);
          if (isModifierLeadInFreeTyping(path, linePrefix, tok)
            || shouldPauseInlineAtCursor(path, linePrefix, content, position.lineNumber, position.column)) {
            clearInlineCache(editor);
            return { items: [] };
          }
        }

        const cacheKey = buildInlineCacheKey(
          repo, path, position.lineNumber, position.column, linePrefix,
        );
        const javaLevel = inlineJavaLevel(helpers);
        const indexCtx = { helpers, model, position };
        const memberContext = dotQualifierFromLinePrefix(linePrefix);
        const aiOn = aiInlineFetchEnabled();
        const routeAi = !memberContext && shouldRouteInlineToAi(
          path, linePrefix, content, position.lineNumber, '', aiOn,
        );

        const indexPrefix = memberContext
          ? (memberContext.memberPrefix || '')
          : (extractInlinePartialToken(linePrefix) || '');

        async function buildLspInlineResult() {
          if (!shouldFetchIndexCompletions(linePrefix, indexPrefix, path, {
            content,
            lineNumber: position.lineNumber,
          })) return null;
          const cachedSuffix = inlineSuffixFromCachedIndex(
            helpers, model, position, linePrefix, indexPrefix,
          );
          if (
            cachedSuffix
            && !shouldSuppressInlineGhost(
              path, linePrefix, cachedSuffix, content, position.lineNumber, position.column,
            )
            && !token.isCancellationRequested
          ) {
            return cachedSuffix;
          }
          const wantsFetch = memberContext || shouldPreferLspInlineGhost(
            path, linePrefix, content, position.lineNumber,
          );
          if (!wantsFetch) return null;
          try {
            const suffix = await fetchIndexInlineSuffix(
              helpers, model, position, linePrefix, indexPrefix,
            );
            if (
              suffix
              && !shouldSuppressInlineGhost(
                path, linePrefix, suffix, content, position.lineNumber, position.column,
              )
              && !token.isCancellationRequested
            ) {
              return suffix;
            }
          } catch {
            // fall through
          }
          return null;
        }

        const cachedGhost = inlineGhostFromCache(
          aiInlineCache, repo, path, position.lineNumber, position.column, linePrefix, content,
        );
        if (cachedGhost) {
          editor._reaperPendingControlSnippet = null;
          if (token.isCancellationRequested) return { items: [] };
          return buildInlineItems(model, position, linePrefix, cachedGhost, path);
        }

        if (isWhitespaceOnlyLine(linePrefix)) {
          const indexPrefix = memberContext
            ? (memberContext.memberPrefix || '')
            : (extractInlinePartialToken(linePrefix) || '');
          const emptyLspCached = inlineSuffixFromCachedIndex(
            helpers, model, position, linePrefix, indexPrefix,
          );
          if (
            emptyLspCached
            && !shouldSuppressInlineGhost(
              path, linePrefix, emptyLspCached, content, position.lineNumber, position.column,
            )
            && !token.isCancellationRequested
          ) {
            const meta = inlineCacheMeta(repo, path, position, linePrefix);
            setInlineCache(editor, cacheKey, emptyLspCached, '', meta, { refresh: false });
            editor._reaperPendingControlSnippet = null;
            return buildInlineItems(model, position, linePrefix, emptyLspCached, path);
          }
          const emptyLocal = inferEmptyLineContinuationSuffix(
            path, content, position.lineNumber, linePrefix, javaLevel,
          );
          if (
            emptyLocal
            && !shouldSuppressInlineGhost(
              path, linePrefix, emptyLocal, content, position.lineNumber, position.column,
            )
            && !token.isCancellationRequested
          ) {
            const meta = inlineCacheMeta(repo, path, position, linePrefix);
            setInlineCache(editor, cacheKey, emptyLocal, '', meta, { refresh: false });
            editor._reaperPendingControlSnippet = null;
            return buildInlineItems(model, position, linePrefix, emptyLocal, path);
          }
          if (aiOn && routeAi) {
            scheduleAiInlineFetch();
          }
          if (helpers.repoApi && shouldFetchEmptyLineInline(path, linePrefix, content, position.lineNumber)) {
            prefetchIndexInlineGhost(editor, linePrefix, indexPrefix);
          }
          return { items: [] };
        }

        if (shouldPreferLspInlineGhost(path, linePrefix, content, position.lineNumber)) {
          const lspSuffix = await buildLspInlineResult();
          if (lspSuffix) {
            const meta = inlineCacheMeta(repo, path, position, linePrefix);
            setInlineCache(editor, cacheKey, lspSuffix, '', meta, { refresh: false });
            editor._reaperPendingControlSnippet = null;
            return buildInlineItems(model, position, linePrefix, lspSuffix, path);
          }
        }

        {
          const local = inlineGhostSuffix(
            path, linePrefix, content, position.lineNumber, javaLevel, position.column, indexCtx,
            { fast: true },
          );
          if (
            local
            && !shouldSuppressInlineGhost(
              path, linePrefix, local, content, position.lineNumber, position.column,
            )
          ) {
            const meta = inlineCacheMeta(repo, path, position, linePrefix);
            setInlineCache(editor, cacheKey, local, '', meta, { refresh: false });
            editor._reaperPendingControlSnippet = null;
            if (token.isCancellationRequested) return { items: [] };
            return buildInlineItems(model, position, linePrefix, local, path);
          }
        }

        if (!shouldPreferLspInlineGhost(path, linePrefix, content, position.lineNumber)) {
          const lspSuffix = await buildLspInlineResult();
          if (lspSuffix) {
            const meta = inlineCacheMeta(repo, path, position, linePrefix);
            setInlineCache(editor, cacheKey, lspSuffix, '', meta, { refresh: false });
            editor._reaperPendingControlSnippet = null;
            return buildInlineItems(model, position, linePrefix, lspSuffix, path);
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
                setInlineCache(editor, cacheKey, suffix, '', meta, { refresh: false });
                return buildInlineItems(model, position, linePrefix, suffix, path);
              }
            } catch {
              // fall through
            }
          }
        }

        if (!routeAi && !isImportTypingLine(path, linePrefix)) {
          try {
            const text = await fetchInlineComplete(model, position, linePrefix, true);
            if (
              text
              && !shouldSuppressInlineGhost(
                path, linePrefix, text, content, position.lineNumber, position.column,
              )
              && !token.isCancellationRequested
            ) {
              const meta = inlineCacheMeta(repo, path, position, linePrefix);
              setInlineCache(editor, cacheKey, text, '', meta, { refresh: false });
              editor._reaperPendingControlSnippet = null;
              return buildInlineItems(model, position, linePrefix, text, path);
            }
          } catch {
            // fall through
          }
        }

        if (routeAi && aiOn) {
          scheduleAiInlineFetch();
        }

        return { items: [] };
      },
      freeInlineCompletions: () => {},
    });

    if (!window.__reaperInlineVisibilityHook) {
      window.__reaperInlineVisibilityHook = true;
      document.addEventListener('visibilitychange', () => {
        if (document.visibilityState === 'hidden') cancelAiInlineFetch();
      });
    }

    editor.onDidChangeModelContent(() => {
      editor._reaperLastContentEditAt = Date.now();
      const model = editor.getModel();
      if (model) editor._reaperContent = model.getValue();
      queueMicrotask(() => maybeExpandEmptyBraceBlock(editor));
      queueMicrotask(() => maybeInsertJavaAssignSpace(editor));
      if (!editorAcceptsInlineAi(editor)) return;
      if (completionEscapedRecently(editor)) return;
      refreshInlineLocalFast(editor);
      scheduleInlineOnPause(editor);
    });

    editor.onKeyDown((e) => {
      const ev = e.browserEvent;
      if (ev?.key === 'Escape' && !ev.ctrlKey && !ev.metaKey && !ev.altKey) {
        escapeEditorCompletionUi(editor);
        e.preventDefault();
        e.stopPropagation();
        return;
      }
      if (ev?.key === ' ' && ev.ctrlKey && !ev.metaKey && !ev.altKey) {
        ev.preventDefault();
        ev.stopPropagation();
        editor._reaperSuggestEscapedUntil = 0;
        setTimeout(() => fireCompletionsSuggest(activeEditor(), { force: true }), 0);
        return;
      }
      // Space never accepts suggest/AI — dismiss and type a real space.
      if (ev?.key === ' ' && !ev.ctrlKey && !ev.metaKey && !ev.altKey && !ev.shiftKey) {
        if (hasActiveInlineSuggestion(editor) || completionUiBlocksTyping(editor)) {
          dismissInlineGhost(editor);
          dismissSuggestUi(editor);
        }
      }
    });

    editor.onDidChangeCursorPosition((e) => {
      if (cursorMoveFromMouse(e)) {
        if (editorAcceptsInlineAi(editor)) dismissCompletionOnCursorClick(editor);
        return;
      }
      if (Date.now() - (editor._reaperLastContentEditAt || 0) < 80) return;
      if (!editorAcceptsInlineAi(editor)) return;
      if (completionEscapedRecently(editor)) return;
      scheduleInlineOnPause(editor);
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
        if (!helpers.openFileAt || resource?.scheme !== 'reaper' || resource?.authority !== 'workspace') {
          return false;
        }
        const path = pathFromReaperUri(resource);
        if (!path) return false;
        markDefinitionNavigation();
        void helpers.openFileAt(path, selection?.startLineNumber || 1, selection?.startColumn || 1);
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
      // Context menu: use Monaco's editor.action.revealDefinition (same provideDefinition).
      run: async () => {
        const pos = editor.getPosition();
        if (!pos) return;
        const model = editor.getModel();
        if (!model) return;
        const loc = await lookupDefinition(helpers, model, pos);
        if (!loc) {
          reportNoDefinition(helpers);
          helpers.toast?.('No definition found — wait for Java index or add import', 'info');
          return;
        }
        markDefinitionNavigation();
        const path = pathFromReaperUri(loc.uri);
        if (path) {
          await helpers.openFileAt(path, loc.range.startLineNumber, loc.range.startColumn);
        }
      },
    });

    editor.addAction({
      id: 'reaper.changeAllOccurrences',
      label: 'Change All Occurrences',
      keybindings: [
        monaco.KeyMod.CtrlCmd | monaco.KeyMod.Alt | monaco.KeyCode.KeyG,
      ],
      // Context menu: use Monaco's editor.action.changeAll (same behavior).
      run: () => editor.getAction('editor.action.changeAll')?.run(),
    });

    editor.addAction({
      id: 'reaper.findUsages',
      label: 'Find Usages',
      keybindings: [monaco.KeyMod.Alt | monaco.KeyCode.F7],
      contextMenuGroupId: 'navigation',
      contextMenuOrder: 2.0,
      run: () => runFindUsages(editor, helpers),
    });

    editor.addAction({
      id: 'reaper.peekDefinition',
      label: 'Peek Definition',
      keybindings: [monaco.KeyMod.Alt | monaco.KeyCode.F12],
      // Context menu: use Monaco's editor.action.peekDefinition.
      run: () => runPeekDefinition(editor),
    });

    editor.addAction({
      id: 'reaper.renameSymbol',
      label: 'Rename Symbol',
      keybindings: [monaco.KeyCode.F6],
      contextMenuGroupId: 'navigation',
      contextMenuOrder: 3.0,
      run: () => runRename(editor, helpers),
    });

    editor.addAction({
      id: 'reaper.javaRefactor',
      label: 'Refactor…',
      keybindings: [
        monaco.KeyMod.Shift | monaco.KeyMod.Alt | monaco.KeyCode.KeyR,
      ],
      precondition: 'editorLangId == java',
      // Use Monaco's numbered modification group so this sits after Navigation,
      // not as the first context-menu section (plain "modification" sorts first).
      contextMenuGroupId: '1_modification',
      contextMenuOrder: 1.5,
      run: () => runJavaRefactor(editor, helpers),
    });

    editor.addAction({
      id: 'reaper.organizeImports',
      label: 'Organize Imports',
      keybindings: [
        monaco.KeyMod.CtrlCmd | monaco.KeyMod.Alt | monaco.KeyCode.KeyO,
      ],
      contextMenuGroupId: '1_modification',
      contextMenuOrder: 2.0,
      run: () => runJavaOrganizeImports(editor, helpers),
    });

    editor.addAction({
      id: 'reaper.parameterInfo',
      label: 'Parameter Info',
      keybindings: [
        monaco.KeyMod.CtrlCmd | monaco.KeyMod.Shift | monaco.KeyCode.Space,
      ],
      contextMenuGroupId: 'navigation',
      contextMenuOrder: 4.0,
      run: () => editor.trigger('keyboard', 'editor.action.triggerParameterHints', {}),
    });

    const signatureHelpLangs = ['java', 'c', 'cpp'];
    try {
      if (typeof monaco.languages.registerSignatureHelpProvider === 'function') {
        monaco.languages.registerSignatureHelpProvider(signatureHelpLangs, {
          signatureHelpTriggerCharacters: ['(', ','],
          provideSignatureHelp: async (model, position) => {
            const help = await postWorkspaceLsp(helpers, model, position, '/workspace/signature-help');
            if (!help?.signatures?.length) {
              return { value: { signatures: [], activeSignature: 0, activeParameter: 0 }, dispose: () => {} };
            }
            const signatures = help.signatures.map((sig) => ({
              label: sig.label,
              documentation: sig.documentation
                ? lspMarkupContent(sig.documentation)
                : undefined,
              parameters: (sig.parameters || []).map((p) => ({
                label: p.label,
                documentation: p.documentation
                  ? lspMarkupContent(p.documentation)
                  : undefined,
              })),
            }));
            return {
              value: {
                signatures,
                activeSignature: help.active_signature || 0,
                activeParameter: help.active_parameter ?? 0,
              },
              dispose: () => {},
            };
          },
        });
      }
    } catch (err) {
      console.warn('[ReaperLang] registerSignatureHelpProvider failed', err);
    }

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
      id: 'reaper.goToLine',
      label: 'Go to Line…',
      keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyG],
      run: () => helpers.goToLine?.(),
    });

    editor.addAction({
      id: 'reaper.inlineSuggest.acceptEnter',
      label: 'Accept Inline Suggestion',
      keybindings: [monaco.KeyCode.Enter],
      precondition: 'inlineSuggestionVisible && !suggestWidgetVisible',
      run: (ed) => {
        // Enter never accepts AI/ghost — only Tab does. Keep a real newline.
        dismissInlineGhost(ed);
        dismissSuggestUi(ed);
        ed.trigger('keyboard', 'type', { text: '\n' });
      },
    });

    editor.onDidChangeModelContent(() => {
      helpers.setLanguageStatus?.();
    });

    if (reaperDotCompletionHandler) {
      helpers.setCompleteDebugStatus?.(`c${REAPER_COMPLETION_REV} · dot handler ready`);
    } else {
      console.warn('[Reaper] dot handler not attached');
    }

    return helpers;
  }

  function isDiagnosablePath(path) {
    if (!path || path.startsWith('.reaper/')) return false;
    const base = (path.split('/').pop() || '').toLowerCase();
    if (!base || base.startsWith('.')) return false;
    if (base === 'dockerfile' || base.startsWith('dockerfile.')) return true;
    if (base === 'makefile' || base === 'gnumakefile' || base === 'cmakelists.txt') return true;
    return langForPath(path) !== 'plaintext';
  }

  function ensureMonacoBasicLanguage(lang, onReady) {
    const done = typeof onReady === 'function' ? onReady : () => {};
    if (typeof require === 'undefined' || !lang || lang === 'plaintext') {
      done();
      return;
    }
    const loadLang = lang === 'c' ? 'cpp' : lang;
    const safe = String(loadLang).replace(/[^a-z0-9_-]/gi, '');
    if (!safe || REAPER_CUSTOM_LANGS.has(safe)) {
      done();
      return;
    }
    try {
      require([`vs/basic-languages/${safe}/${safe}`], () => done(), () => done());
    } catch {
      done();
    }
  }

  function applyEditorLanguage(path, model, onReady) {
    const expected = langForPath(path || '');
    const done = typeof onReady === 'function' ? onReady : () => {};
    if (!model || !expected || expected === 'plaintext') {
      done(expected);
      return;
    }
    ensureReaperCustomLanguage(expected);
    ensureMonacoBasicLanguage(expected, () => {
      const prev = model.getLanguageId?.();
      if (prev !== expected) {
        monaco.editor.setModelLanguage(model, expected);
      } else if (REAPER_CUSTOM_LANGS.has(expected)) {
        monaco.editor.setModelLanguage(model, 'plaintext');
        monaco.editor.setModelLanguage(model, expected);
      }
      done(expected);
    });
  }

  Object.assign(window.ReaperLang || {}, {
    bundleRev: '353',
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
    langLabelForPath,
    compilerToolIdsForPath,
    compilerLabelsForPath,
    isDiagnosablePath,
    registerGroovy,
    registerMakefile,
    setupEditorFeatures,
    extractSymbols,
    ensureMonacoBasicLanguage,
    applyEditorLanguage,
    ensureReaperCustomLanguage,
    handleDotCompletion: (ed) => {
      if (typeof reaperDotCompletionHandler === 'function') {
        return reaperDotCompletionHandler(ed);
      }
      return window.ReaperLang?.handleDotCompletion?.(ed) ?? -1;
    },
    inlineTestHelpers: () => ({
      memberInlineSuffix,
      localInlineSuggestion,
      inlineGhostSuffix,
      skipLocalStatementTemplates,
      shouldPreferAiStatementInline,
      shouldRouteInlineToAi,
      shouldFetchEmptyLineInline,
      shouldFetchIndexCompletions,
      extractInlinePartialToken,
      scopeIdentifierInlineSuffix,
      collectContentIdentifierCandidates,
      isWhitespaceOnlyLine,
      inferEmptyLineContinuationSuffix,
      findEnclosingBlock,
      emptyLineNeighborBelow,
      emptyLineNeighborAbove,
      inferEmptyLineFromBelowPath,
      isNearbyLine,
      isGradleBuildFile,
      isGradlePropertiesFile,
      gradlePropertiesLocalInline,
      INLINE_NEARBY_LINES,
      editorScopeMatches,
      suggestionMatchesEditorLanguage,
      maybeInsertJavaAssignSpace,
      isCodeStatementLanguage,
      isDeclarationTyping,
      shouldSuppressInlineGhost,
      supportsWorkspaceIndexInline,
      isProseLanguage,
      isMarkupOrConfigPath,
      inlineSuffixFromIndexItems,
      shouldPreferLspInlineGhost,
      inlineSuffixFromLspItem,
      javaSyncTypeInlineSuffix,
      localJavaImportFqcn,
      extractJavacClassSymbol,
      keywordInlineSuffix,
      statementKeywordsForPath,
      shouldPreferJavaTypeInline,
      shouldPreferControlKeywordInline,
      shouldPreferModifierKeywordInline,
      isCompleteModifierSequence,
      isModifierLeadInFreeTyping,
      isDeclarativeLeadInFreeTyping,
      modifierKeywordInlineSuffix,
      modifierKeywordsForPath,
      isModifierKeywordPrefix,
      controlStructureInlineSuffix,
      rankIndexItemsForInline,
      buildInlineItems,
      buildInlineCacheKey,
      inlineGhostFromCache,
      shouldPrefetchInlineOnPause,
      inlineIndexPrefix,
      isImportTypingLine,
      isControlKeywordPrefix,
      parseJavaIndexedForLoop,
      javaIndexedAccessExpr,
      javaIndexedLoopBodyLine,
      inlineJavaLevel,
      isLineInlineComplete,
      shouldPauseInlineAtCursor,
      isOperandContext,
      isAfterOperatorWithSpace,
      fastLineWindowScopeSuffix,
      operandInlineSuffix,
      operatorTailMatch,
      rankScopeCandidates,
      isReturnContext,
      isCallArgContext,
      returnInlineSuffix,
      callArgInlineSuffix,
      inlineFilterUsesFullPrefix,
    }),
  });
  window.__reaperLangBundleLoaded = true;
})();
