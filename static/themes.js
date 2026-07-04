(function () {
  const THEME_KEY = 'reaper-theme';
  const DEFAULT_THEME = 'navy';

  const THEMES = [
    { id: 'darcula', label: 'Darcula (Classic)', dark: true, monaco: 'reaper-darcula' },
    { id: 'charcoal', label: 'Charcoal Dark', dark: true, monaco: 'reaper-charcoal' },
    { id: 'navy', label: 'Deep Navy', dark: true, monaco: 'reaper-navy' },
    { id: 'blueblack', label: 'Blue & Black', dark: true, monaco: 'reaper-blueblack' },
    { id: 'offwhite', label: 'Off-White Light', dark: false, monaco: 'reaper-offwhite' },
    { id: 'mono', label: 'Black & White', dark: true, monaco: 'reaper-mono' },
  ];

  const LEGACY_THEME_IDS = {
    softgray: 'offwhite',
    solarized: 'offwhite',
    paper: 'offwhite',
    highcontrast: 'offwhite',
  };

  // Diagnostic squiggles: visible underline colors; no gutter wash-out or scrollbar stripes.
  const DIAG_EDITOR_COLORS = {
    'editorError.foreground': '#FF5555',
    'editorError.border': '#FF5555',
    'editorError.background': '#00000000',
    'editorWarning.foreground': '#FBBF24',
    'editorWarning.border': '#FBBF24',
    'editorWarning.background': '#00000000',
    'editorInfo.foreground': '#60A5FA',
    'editorInfo.border': '#60A5FA',
    'editorInfo.background': '#00000000',
    'minimap.errorHighlight': '#00000000',
    'minimap.warningHighlight': '#00000000',
    'overviewRuler.errorForeground': '#FF5555',
    'overviewRuler.warningForeground': '#FBBF24',
    'overviewRuler.infoForeground': '#60A5FA',
  };

  const SUGGEST_INLINE_COLORS = {
    'editorGhostText.foreground': '#808080',
    'editorSuggestWidget.background': '#3C3F41',
    'editorSuggestWidget.border': '#515658',
    'editorSuggestWidget.foreground': '#BBBBBB',
    'editorSuggestWidget.selectedBackground': '#214283',
    'editorSuggestWidget.selectedForeground': '#FFFFFF',
    'editorSuggestWidget.highlightForeground': '#FFFFFF',
    'editorSuggestWidget.focusHighlightForeground': '#FFFFFF',
    'editorHoverWidget.background': '#3C3F41',
    'editorHoverWidget.foreground': '#BBBBBB',
    'editorHoverWidget.border': '#515658',
    'editorHoverWidget.highlightForeground': '#6BA6F7',
    'editorHoverWidget.statusBarBackground': '#313335',
    'editorLightBulb.foreground': '#FFCC33',
    'editorLightBulbAi.foreground': '#FFCC33',
    'editorLightBulbAutoFix.foreground': '#FFCC33',
  };

  const MENU_WIDGET_COLORS_DARK = {
    'menu.background': '#3C3F41',
    'menu.foreground': '#BBBBBB',
    'menu.selectionBackground': '#214283',
    'menu.selectionForeground': '#FFFFFF',
    'menu.border': '#515658',
    'menu.separatorBackground': '#515658',
  };

  const MENU_WIDGET_COLORS_LIGHT = {
    'menu.background': '#FFFFFF',
    'menu.foreground': '#333333',
    'menu.selectionBackground': '#0060C0',
    'menu.selectionForeground': '#FFFFFF',
    'menu.border': '#E2E8F0',
    'menu.separatorBackground': '#E2E8F0',
  };

  function monacoWidgetColors(base) {
    if (base === 'vs') {
      return { ...SUGGEST_INLINE_COLORS, ...MENU_WIDGET_COLORS_LIGHT };
    }
    return { ...SUGGEST_INLINE_COLORS, ...MENU_WIDGET_COLORS_DARK };
  }

  const MONACO_THEMES = {
    'reaper-darcula': {
      base: 'vs-dark',
      inherit: true,
      rules: [
        { token: 'comment', foreground: '808080', fontStyle: 'italic' },
        { token: 'keyword', foreground: 'CC7832' },
        { token: 'string', foreground: '6A8759' },
        { token: 'number', foreground: '6897BB' },
        { token: 'type', foreground: 'A9B7C6' },
        { token: 'type.identifier', foreground: 'FFB86C' },
        { token: 'identifier', foreground: 'A9B7C6' },
        { token: 'delimiter', foreground: 'A9B7C6' },
        { token: 'operator', foreground: 'A9B7C6' },
        { token: 'variable', foreground: '9876AA' },
        { token: 'variable.name', foreground: '9876AA' },
        { token: 'metatag', foreground: '6A8759' },
      ],
      colors: {
        'editor.background': '#2B2B2B',
        'editor.foreground': '#A9B7C6',
        'editorLineNumber.foreground': '#606366',
        'editorLineNumber.activeForeground': '#A9B7C6',
        'editor.lineHighlightBackground': '#323232',
        'editor.selectionBackground': '#214283',
        'editor.inactiveSelectionBackground': '#21428380',
        'editorCursor.foreground': '#A9B7C6',
        'editorWhitespace.foreground': '#3C3F41',
        'editorIndentGuide.background': '#373737',
        'editorIndentGuide.activeBackground': '#606366',
        'editorGutter.background': '#313335',
        'minimap.background': '#2B2B2B',
        'editorWidget.background': '#3C3F41',
        'editorWidget.border': '#515658',
      },
    },
    'reaper-charcoal': {
      base: 'vs-dark',
      inherit: true,
      rules: [
        { token: 'comment', foreground: '6A9955', fontStyle: 'italic' },
        { token: 'keyword', foreground: 'FF00FF' },
        { token: 'string', foreground: '39FF14' },
        { token: 'number', foreground: '00FFFF' },
        { token: 'type', foreground: 'FFFFFF' },
        { token: 'type.identifier', foreground: '00FFFF' },
        { token: 'identifier', foreground: 'FFFFFF' },
        { token: 'delimiter', foreground: 'D4D4D4' },
        { token: 'operator', foreground: 'FF00FF' },
        { token: 'variable', foreground: '9876AA' },
        { token: 'variable.name', foreground: '9876AA' },
        { token: 'annotation', foreground: '39FF14' },
      ],
      colors: {
        'editor.background': '#1E1E1E',
        'editor.foreground': '#FFFFFF',
        'editorLineNumber.foreground': '#858585',
        'editorLineNumber.activeForeground': '#FFFFFF',
        'editor.lineHighlightBackground': '#2A2A2A',
        'editor.selectionBackground': '#264F78',
        'editor.inactiveSelectionBackground': '#264F7880',
        'editorCursor.foreground': '#FFFFFF',
        'editorWhitespace.foreground': '#3E3E42',
        'editorIndentGuide.background': '#404040',
        'editorIndentGuide.activeBackground': '#707070',
        'editorGutter.background': '#1E1E1E',
        'minimap.background': '#1E1E1E',
        'editorWidget.background': '#252526',
        'editorWidget.border': '#3E3E42',
      },
    },
    'reaper-navy': {
      base: 'vs-dark',
      inherit: true,
      rules: [
        { token: 'comment', foreground: '64748B', fontStyle: 'italic' },
        { token: 'keyword', foreground: 'E879F9' },
        { token: 'string', foreground: '4ADE80' },
        { token: 'number', foreground: '22D3EE' },
        { token: 'type', foreground: 'F8FAFC' },
        { token: 'type.identifier', foreground: '22D3EE' },
        { token: 'identifier', foreground: 'F8FAFC' },
        { token: 'delimiter', foreground: 'CBD5E1' },
        { token: 'operator', foreground: 'E879F9' },
        { token: 'variable', foreground: 'A78BFA' },
        { token: 'variable.name', foreground: 'A78BFA' },
        { token: 'annotation', foreground: '4ADE80' },
      ],
      colors: {
        'editor.background': '#0F172A',
        'editor.foreground': '#FFFFFF',
        'editorLineNumber.foreground': '#64748B',
        'editorLineNumber.activeForeground': '#F8FAFC',
        'editor.lineHighlightBackground': '#1E293B',
        'editor.selectionBackground': '#1D4ED866',
        'editor.inactiveSelectionBackground': '#1D4ED844',
        'editorCursor.foreground': '#22D3EE',
        'editorWhitespace.foreground': '#334155',
        'editorIndentGuide.background': '#1E293B',
        'editorIndentGuide.activeBackground': '#475569',
        'editorGutter.background': '#0F172A',
        'minimap.background': '#0F172A',
        'editorWidget.background': '#1E293B',
        'editorWidget.border': '#334155',
      },
    },
    'reaper-offwhite': {
      base: 'vs',
      inherit: true,
      rules: [
        { token: 'comment', foreground: '64748B', fontStyle: 'italic' },
        { token: 'keyword', foreground: '2563EB' },
        { token: 'string', foreground: '0F172A' },
        { token: 'number', foreground: '475569' },
        { token: 'type', foreground: '334155' },
        { token: 'type.identifier', foreground: '2563EB' },
        { token: 'identifier', foreground: '334155' },
        { token: 'delimiter', foreground: '475569' },
        { token: 'operator', foreground: '2563EB' },
        { token: 'variable', foreground: '7C3AED' },
        { token: 'variable.name', foreground: '7C3AED' },
        { token: 'annotation', foreground: 'DC2626' },
      ],
      colors: {
        'editor.background': '#F8FAFC',
        'editor.foreground': '#334155',
        'editorLineNumber.foreground': '#94A3B8',
        'editorLineNumber.activeForeground': '#0F172A',
        'editor.lineHighlightBackground': '#F1F5F9',
        'editor.selectionBackground': '#BFDBFE',
        'editor.inactiveSelectionBackground': '#DBEAFE',
        'editorCursor.foreground': '#2563EB',
        'editorWhitespace.foreground': '#E2E8F0',
        'editorIndentGuide.background': '#E2E8F0',
        'editorIndentGuide.activeBackground': '#CBD5E1',
        'editorGutter.background': '#F8FAFC',
        'minimap.background': '#F8FAFC',
        'editorWidget.background': '#FFFFFF',
        'editorWidget.border': '#E2E8F0',
      },
    },
    'reaper-blueblack': {
      base: 'vs-dark',
      inherit: true,
      rules: [
        { token: 'comment', foreground: '6B7280', fontStyle: 'italic' },
        { token: 'keyword', foreground: '60A5FA' },
        { token: 'string', foreground: 'E5E7EB' },
        { token: 'number', foreground: '93C5FD' },
        { token: 'type', foreground: 'F9FAFB' },
        { token: 'type.identifier', foreground: '60A5FA' },
        { token: 'identifier', foreground: 'F3F4F6' },
        { token: 'delimiter', foreground: 'D1D5DB' },
        { token: 'operator', foreground: '60A5FA' },
        { token: 'annotation', foreground: '9CA3AF' },
      ],
      colors: {
        'editor.background': '#000000',
        'editor.foreground': '#F3F4F6',
        'editorLineNumber.foreground': '#4B5563',
        'editorLineNumber.activeForeground': '#E5E7EB',
        'editor.lineHighlightBackground': '#0A0A0A',
        'editor.selectionBackground': '#1D4ED866',
        'editor.inactiveSelectionBackground': '#1D4ED844',
        'editorCursor.foreground': '#3B82F6',
        'editorWhitespace.foreground': '#1F2937',
        'editorIndentGuide.background': '#111111',
        'editorIndentGuide.activeBackground': '#374151',
        'editorGutter.background': '#000000',
        'minimap.background': '#000000',
        'editorWidget.background': '#0A0A0A',
        'editorWidget.border': '#1F2937',
      },
    },
    'reaper-mono': {
      base: 'vs-dark',
      inherit: true,
      rules: [
        { token: 'comment', foreground: '6E6E6E', fontStyle: 'italic' },
        { token: 'keyword', foreground: 'FFFFFF', fontStyle: 'bold' },
        { token: 'string', foreground: 'B0B0B0' },
        { token: 'number', foreground: 'D4D4D4' },
        { token: 'type', foreground: 'E8E8E8' },
        { token: 'type.identifier', foreground: 'E8E8E8' },
        { token: 'identifier', foreground: 'D4D4D4' },
        { token: 'delimiter', foreground: 'A0A0A0' },
        { token: 'operator', foreground: 'FFFFFF' },
        { token: 'annotation', foreground: '8A8A8A' },
      ],
      colors: {
        'editor.background': '#0A0A0A',
        'editor.foreground': '#D4D4D4',
        'editorLineNumber.foreground': '#525252',
        'editorLineNumber.activeForeground': '#D4D4D4',
        'editor.lineHighlightBackground': '#141414',
        'editor.selectionBackground': '#3A3A3A',
        'editor.inactiveSelectionBackground': '#2A2A2A',
        'editorCursor.foreground': '#FFFFFF',
        'editorWhitespace.foreground': '#1A1A1A',
        'editorIndentGuide.background': '#1A1A1A',
        'editorIndentGuide.activeBackground': '#404040',
        'editorGutter.background': '#0A0A0A',
        'minimap.background': '#0A0A0A',
        'editorWidget.background': '#141414',
        'editorWidget.border': '#2A2A2A',
      },
    },
  };

  let cachedThemeId = DEFAULT_THEME;

  function normalizeThemeId(id) {
    const mapped = LEGACY_THEME_IDS[id] || id;
    return THEMES.some((t) => t.id === mapped) ? mapped : DEFAULT_THEME;
  }

  async function persistThemePref(id) {
    try {
      await fetch('/api/ui-preferences', {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ theme_id: id }),
      });
    } catch (err) {
      console.warn('[Reaper] Failed to save theme to ui-preferences.json:', err);
    }
  }

  async function loadThemePref() {
    let id = DEFAULT_THEME;
    try {
      const res = await fetch('/api/ui-preferences');
      if (res.ok) {
        const prefs = await res.json();
        if (prefs?.theme_id) id = normalizeThemeId(prefs.theme_id);
      }
    } catch {
      /* use default until API is up */
    }

    const legacy = localStorage.getItem(THEME_KEY);
    if (legacy && THEMES.some((t) => t.id === legacy)) {
      if (legacy !== id) {
        id = legacy;
        void persistThemePref(id);
      }
      localStorage.removeItem(THEME_KEY);
    } else if (legacy) {
      localStorage.removeItem(THEME_KEY);
    }

    cachedThemeId = id;
    return id;
  }

  function getStoredTheme() {
    return normalizeThemeId(cachedThemeId);
  }

  function getTheme(id) {
    return THEMES.find((t) => t.id === id) || THEMES[0];
  }

  function syncThemeSelects(id) {
    ['theme-select', 'theme-select-menu'].forEach((selId) => {
      const el = document.getElementById(selId);
      if (el && el.value !== id) el.value = id;
    });
  }

  function applyTheme(id, { persist = true } = {}) {
    const theme = getTheme(normalizeThemeId(id));
    cachedThemeId = theme.id;
    const root = document.documentElement;
    root.dataset.theme = theme.id;
    root.classList.toggle('dark', theme.dark);
    root.classList.toggle('theme-light', !theme.dark);
    if (persist) void persistThemePref(theme.id);
    syncThemeSelects(theme.id);
    syncMonacoOverflowWidgetTheme(theme.dark);

    if (typeof monaco !== 'undefined' && window.__reaperMonacoThemesDefined) {
      syncMonacoEditorTheme(theme);
    }
    if (typeof window.syncTerminalTheme === 'function') {
      window.syncTerminalTheme();
    }
  }

  function themeSurfaceColorsFromCss(dark = true) {
    const root = document.documentElement;
    const styles = getComputedStyle(root);
    const pick = (name, fallback) => styles.getPropertyValue(name).trim() || fallback;
    return {
      panel: pick('--ij-panel', dark ? '#3C3F41' : '#FFFFFF'),
      text: pick('--ij-text', dark ? '#BBBBBB' : '#1E1E1E'),
      textBright: pick('--ij-text-bright', '#FFFFFF'),
      border: pick('--ij-border', dark ? '#515658' : '#C8C8C8'),
      selected: pick('--ij-selection', dark ? '#214283' : '#0060C0'),
    };
  }

  function themeMenuColorsFromCss(dark = true) {
    const { panel, text, textBright, border, selected } = themeSurfaceColorsFromCss(dark);
    return {
      'menu.background': panel,
      'menu.foreground': text,
      'menu.selectionBackground': selected,
      'menu.selectionForeground': textBright,
      'menu.border': border,
      'menu.separatorBackground': border,
    };
  }

  /** Hover/suggest/menu widgets render outside .monaco-editor (overflow root + body context view). */
  function syncMonacoOverflowWidgetTheme(dark = true) {
    const el = document.getElementById('editor-overflow-root');
    const { panel, text, textBright, border, selected } = themeSurfaceColorsFromCss(dark);
    const vars = {
      '--vscode-editorSuggestWidget-background': panel,
      '--vscode-editorSuggestWidget-foreground': text,
      '--vscode-editorSuggestWidget-border': border,
      '--vscode-editorSuggestWidget-selectedBackground': selected,
      '--vscode-editorSuggestWidget-selectedForeground': textBright,
      '--vscode-editorSuggestWidget-highlightForeground': textBright,
      '--vscode-editorHoverWidget-background': panel,
      '--vscode-editorHoverWidget-foreground': text,
      '--vscode-editorHoverWidget-border': border,
      '--vscode-widget-shadow': 'rgba(0, 0, 0, 0.36)',
      '--vscode-menu-background': panel,
      '--vscode-menu-foreground': text,
      '--vscode-menu-selectionBackground': selected,
      '--vscode-menu-selectionForeground': textBright,
      '--vscode-menu-border': border,
      '--vscode-menu-separatorBackground': border,
      '--vscode-foreground': text,
    };
    Object.entries(vars).forEach(([key, value]) => {
      document.documentElement.style.setProperty(key, value);
      el?.style.setProperty(key, value);
    });
  }

  function syncMonacoEditorTheme(theme) {
    if (typeof monaco === 'undefined' || !window.__reaperMonacoThemesDefined) return;
    const spec = MONACO_THEMES[theme.monaco];
    if (!spec) return;
    monaco.editor.defineTheme(theme.monaco, {
      ...spec,
      colors: {
        ...spec.colors,
        ...DIAG_EDITOR_COLORS,
        ...monacoWidgetColors(spec.base),
        ...themeMenuColorsFromCss(theme.dark),
      },
    });
    monaco.editor.setTheme(theme.monaco);
  }

  function defineMonacoThemes() {
    if (typeof monaco === 'undefined' || window.__reaperMonacoThemesDefined) return;
    window.__reaperMonacoThemesDefined = true;
    Object.entries(MONACO_THEMES).forEach(([id, spec]) => {
      monaco.editor.defineTheme(id, {
        ...spec,
        colors: { ...spec.colors, ...DIAG_EDITOR_COLORS, ...monacoWidgetColors(spec.base) },
      });
    });
  }

  function getMonacoThemeId() {
    return getTheme(getStoredTheme()).monaco;
  }

  function populateThemeSelect() {
    ['theme-select', 'theme-select-menu'].forEach((selId) => {
      const select = document.getElementById(selId);
      if (!select || select.dataset.populated) return;
      select.dataset.populated = '1';
      THEMES.forEach((t) => {
        const opt = document.createElement('option');
        opt.value = t.id;
        opt.textContent = t.label;
        select.appendChild(opt);
      });
      select.value = getStoredTheme();
      select.addEventListener('change', (e) => {
        applyTheme(e.target.value);
        document.querySelectorAll('.ij-menu-root.open').forEach((m) => m.classList.remove('open'));
      });
    });
  }

  async function initThemes() {
    await loadThemePref();
    applyTheme(getStoredTheme(), { persist: false });
    populateThemeSelect();
  }

  window.ReaperThemes = {
    THEMES,
    THEME_KEY,
    getStoredTheme,
    getTheme,
    applyTheme,
    loadThemePref,
    defineMonacoThemes,
    syncMonacoEditorTheme,
    getMonacoThemeId,
    syncMonacoOverflowWidgetTheme,
    initThemes,
  };

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', () => { void initThemes(); });
  } else {
    void initThemes();
  }
})();
