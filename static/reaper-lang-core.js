/* Minimal ReaperLang bootstrap — loads before monaco-languages.js so member
   completion works even if the larger bundle fails to parse in WKWebView. */
(function () {
  'use strict';

  const REAPER_COMPLETION_REV = '246';

  function langForPath(path) {
    const base = (path.split('/').pop() || '').toLowerCase();
    if (!base) return 'plaintext';
    if (base === 'dockerfile' || base.startsWith('dockerfile.')) return 'dockerfile';
    if (base === 'makefile' || base === 'gnumakefile') return 'makefile';
    if (base.startsWith('makefile.') || base.endsWith('.mk')) return 'makefile';
    if (base === 'cmakelists.txt') return 'cmake';
    if (base.endsWith('.gradle.kts')) return 'kotlin';
    if (base.endsWith('.gradle')) return 'groovy';
    if (base.endsWith('.properties') || base.endsWith('.gradle.properties')) return 'ini';
    const ext = base.includes('.') ? base.slice(base.lastIndexOf('.') + 1) : '';
    const map = {
      java: 'java', kt: 'kotlin', kts: 'kotlin', groovy: 'groovy', gradle: 'groovy',
      js: 'javascript', mjs: 'javascript', cjs: 'javascript',
      ts: 'typescript', jsx: 'javascript', tsx: 'typescript',
      py: 'python', go: 'go', rs: 'rust', rb: 'ruby', php: 'php',
      c: 'cpp', h: 'cpp', cpp: 'cpp', cc: 'cpp', cxx: 'cpp', hpp: 'cpp', hh: 'cpp',
      md: 'markdown', mdx: 'markdown',
      json: 'json', jsonc: 'json',
      yml: 'yaml', yaml: 'yaml',
      html: 'html', htm: 'html',
      css: 'css', scss: 'scss', less: 'less',
      sql: 'sql', xml: 'xml', toml: 'toml',
      sh: 'shell', bash: 'shell', zsh: 'shell',
    };
    return map[ext] || 'plaintext';
  }

  function langLabel(lang) {
    const labels = {
      java: 'Java',
      groovy: 'Groovy',
      kotlin: 'Kotlin',
      javascript: 'JavaScript',
      typescript: 'TypeScript',
      plaintext: 'Plain Text',
      ini: 'Properties',
      cpp: 'C/C++',
    };
    return labels[lang] || (lang ? lang.charAt(0).toUpperCase() + lang.slice(1) : 'Plain Text');
  }

  function langLabelForPath(path) {
    const base = (path.split('/').pop() || '').toLowerCase();
    if (base.endsWith('.c')) return 'C';
    if (base.endsWith('.h')) return 'C header';
    if (/\.(cpp|cc|cxx|hpp|hh|hxx)$/.test(base)) return 'C++';
    if (base === 'makefile' || base === 'gnumakefile') return 'Makefile';
    return langLabel(langForPath(path));
  }

  const REAPER_CUSTOM_LANGS = new Set(['groovy', 'makefile']);

  function compilerToolIdsForPath(path) {
    const lang = langForPath(path);
    const map = {
      java: ['java'],
      kotlin: ['kotlin'],
      groovy: ['groovy'],
      python: ['python'],
      ruby: ['ruby'],
      rust: ['cargo'],
      go: ['go'],
      javascript: ['node'],
      typescript: ['tsc', 'node'],
      cpp: ['clangd', 'clang', 'gcc'],
    };
    return map[lang] || [];
  }

  function compilerLabelsForPath(path) {
    const ids = compilerToolIdsForPath(path);
    const labels = {
      java: 'Java', kotlin: 'Kotlin', groovy: 'Groovy', python: 'Python',
      ruby: 'Ruby', cargo: 'cargo', go: 'Go', node: 'Node', tsc: 'tsc',
      clangd: 'clangd', clang: 'clang', gcc: 'gcc',
    };
    return ids.map((id) => labels[id] || id).join(', ');
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
    window.ReaperLang?.ensureReaperCustomLanguage?.(expected);
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

  function simplifyJavaTypeName(typeName) {
    let s = String(typeName || '').trim();
    if (!s) return '';
    s = s.replace(/\[\]/g, '');
    const lt = s.indexOf('<');
    if (lt > 0) s = s.slice(0, lt);
    return s.split('.').pop() || s;
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

  const JDK_STATIC_FIELD_TYPES = {
    System: { out: 'PrintStream', in: 'InputStream', err: 'PrintStream' },
  };

  const JDK_CLASS_STATIC_MEMBERS = {
    System: ['out', 'in', 'err'],
    Math: ['PI', 'E', 'abs', 'max', 'min', 'random', 'sqrt'],
    Arrays: ['asList', 'sort', 'copyOf', 'equals', 'stream'],
  };

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

  function jdkBuiltinMembersForType(typeName, memberPrefix) {
    const simple = simplifyJavaTypeName(typeName);
    const members = JDK_TYPE_MEMBERS[simple];
    if (!members) return [];
    const prefixLower = (memberPrefix || '').toLowerCase();
    return members
      .filter((m) => !memberPrefix || m.toLowerCase().startsWith(prefixLower))
      .map((m) => ({ label: m, kind: 'method', detail: `${simple}.${m}` }));
  }

  function jdkStaticMemberItems(qualifier, memberPrefix, content) {
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

  function memberPreviewItems(content, qualifier, memberPrefix) {
    const items = [];
    const seen = new Set();
    for (const m of jdkStaticMemberItems(qualifier, memberPrefix, content)) {
      if (seen.add(m.label)) items.push(m);
    }
    const type = resolveJavaQualifierType(content, qualifier);
    if (type) {
      for (const m of jdkBuiltinMembersForType(type, memberPrefix)) {
        if (seen.add(m.label)) items.push(m);
      }
    }
    return items.slice(0, 40);
  }

  function editorLinePrefix(model, position) {
    if (!model || !position) return '';
    try {
      if (typeof monaco !== 'undefined' && monaco.Range && model.getValueInRange) {
        return model.getValueInRange(
          new monaco.Range(position.lineNumber, 1, position.lineNumber, position.column),
        );
      }
    } catch {
      /* monaco not ready */
    }
    const line = model.getLineContent(position.lineNumber);
    return line.slice(0, Math.max(0, position.column - 1));
  }

  function memberDotContext(model, position) {
    const linePrefix = editorLinePrefix(model, position);
    return { linePrefix, member: dotQualifierFromLinePrefix(linePrefix) };
  }

  let corePopupEl = null;

  function hideCoreMemberPopup() {
    if (corePopupEl) {
      corePopupEl.remove();
      corePopupEl = null;
    }
  }

  function memberInsertText(item) {
    const label = String(item.label || '');
    return item.kind === 'method' ? `${label}()` : label;
  }

  function showCoreMemberPopup(ed, items) {
    hideCoreMemberPopup();
    if (!ed || !items.length) return;
    const model = ed.getModel?.();
    const position = ed.getPosition?.();
    const root = document.getElementById('editor-overflow-root');
    if (!model || !position || !root) return;
    let pt = null;
    try {
      const coords = ed.getScrolledVisiblePosition(position);
      if (coords) pt = { left: coords.left, top: coords.top + coords.height + 2 };
    } catch {
      /* editor not ready */
    }
    if (!pt) return;

    corePopupEl = document.createElement('div');
    corePopupEl.className = 'reaper-member-suggest visible';
    corePopupEl.style.left = `${pt.left}px`;
    corePopupEl.style.top = `${pt.top}px`;

    items.forEach((item, i) => {
      const row = document.createElement('div');
      row.className = `reaper-member-suggest-row${i === 0 ? ' focused' : ''}`;
      row.textContent = item.label;
      row.addEventListener('mousedown', (e) => {
        e.preventDefault();
        const member = dotQualifierFromLinePrefix(editorLinePrefix(model, position));
        const prefix = member?.memberPrefix || '';
        const startCol = Math.max(1, position.column - prefix.length);
        const range = typeof monaco !== 'undefined' && monaco.Range
          ? new monaco.Range(position.lineNumber, startCol, position.lineNumber, position.column)
          : null;
        const text = memberInsertText(item);
        if (range && ed.executeEdits) {
          ed.executeEdits('reaper-member-core', [{ range, text }]);
        } else {
          ed.trigger('keyboard', 'type', { text });
        }
        hideCoreMemberPopup();
        ed.focus?.();
      });
      corePopupEl.appendChild(row);
    });
    root.appendChild(corePopupEl);
  }

  function handleDotCompletionCore(ed) {
    const model = ed?.getModel?.();
    const position = ed?.getPosition?.();
    if (!model || !position) return 0;
    const ctx = memberDotContext(model, position);
    if (!ctx.member) return 0;
    const content = model.getValue();
    const items = memberPreviewItems(
      content, ctx.member.qualifier, ctx.member.memberPrefix,
    );
    if (items.length) showCoreMemberPopup(ed, items);
    return items.length;
  }

  const core = {
    completionRev: () => REAPER_COMPLETION_REV,
    coreOnly: true,
    langForPath,
    langLabel,
    langLabelForPath,
    compilerToolIdsForPath,
    compilerLabelsForPath,
    isDiagnosablePath,
    ensureMonacoBasicLanguage,
    applyEditorLanguage,
    dotQualifierFromLinePrefix,
    memberDotContext,
    editorLinePrefix,
    memberPreview: memberPreviewItems,
    jdkMemberPreview(qualifier, memberPrefix, content) {
      return memberPreviewItems(content, qualifier, memberPrefix);
    },
    handleDotCompletion: handleDotCompletionCore,
    hideCoreMemberPopup,
  };

  window.ReaperLang = Object.assign(window.ReaperLang || {}, core);
})();
