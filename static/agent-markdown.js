/**
 * Markdown + Mermaid rendering for Agent chat messages.
 */
(function () {
  let mermaidReady = false;
  let mermaidLoading = null;
  let mermaidCounter = 0;
  let markedConfigured = false;

  const LIGHT_THEMES = new Set(['offwhite', 'softgray']);
  const MERMAID_SRC = '/vendor/mermaid.min.js';

  function escapeHtml(str) {
    return String(str)
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;');
  }

  function decodeHtmlEntities(str) {
    const el = document.createElement('textarea');
    el.innerHTML = str;
    return el.value;
  }

  function isLightTheme() {
    const id = document.documentElement.getAttribute('data-theme') || 'navy';
    return LIGHT_THEMES.has(id);
  }

  function markedApi() {
    const md = window.marked;
    if (!md) return null;
    const parse = typeof md.parse === 'function' ? md.parse.bind(md)
      : typeof md.marked === 'function' ? md.marked.bind(md)
      : typeof md === 'function' ? md.bind(window)
      : null;
    if (!parse) return null;
    return { parse };
  }

  function libsReady() {
    return !!(markedApi() && window.DOMPurify && typeof window.DOMPurify.sanitize === 'function');
  }

  function initMermaid() {
    if (mermaidReady || !window.mermaid) return;
    window.mermaid.initialize({
      startOnLoad: false,
      theme: isLightTheme() ? 'default' : 'dark',
      securityLevel: 'loose',
      fontFamily: 'Inter, system-ui, -apple-system, sans-serif',
    });
    mermaidReady = true;
  }

  function loadMermaidScript() {
    if (window.mermaid) return Promise.resolve(true);
    if (mermaidLoading) return mermaidLoading;
    mermaidLoading = new Promise((resolve) => {
      const existing = document.querySelector(`script[src="${MERMAID_SRC}"]`);
      if (existing) {
        existing.addEventListener('load', () => resolve(!!window.mermaid), { once: true });
        existing.addEventListener('error', () => resolve(false), { once: true });
        return;
      }
      const script = document.createElement('script');
      script.src = MERMAID_SRC;
      script.async = true;
      script.onload = () => resolve(!!window.mermaid);
      script.onerror = () => resolve(false);
      document.head.appendChild(script);
    });
    return mermaidLoading;
  }

  function ensureMarkedRenderer() {
    if (markedConfigured) return true;
    const api = markedApi();
    if (!api) return false;

    window.marked.use({
      gfm: true,
      breaks: true,
      renderer: {
        code(first, second) {
          const code = typeof first === 'object' && first && 'text' in first
            ? first.text || ''
            : String(first ?? '');
          const lang = (typeof first === 'object' && first && first.lang != null
            ? first.lang
            : second || '').trim().toLowerCase();
          if (lang === 'mermaid') return mermaidBlock(code);
          const safeLang = escapeHtml(lang || 'text');
          return `<pre class="agent-code"><code class="language-${safeLang}">${escapeHtml(code)}</code></pre>`;
        },
      },
    });
    markedConfigured = true;
    return true;
  }

  function mermaidBlock(code) {
    mermaidCounter += 1;
    const id = `agent-mermaid-${Date.now()}-${mermaidCounter}`;
    return `<div class="agent-mermaid-wrap"><pre class="mermaid" id="${id}">${escapeHtml(code)}</pre></div>`;
  }

  /** Strip streamed tool status lines before markdown parse. */
  function prepareAgentMarkdown(text) {
    if (!text) return '';
    let cleaned = String(text)
      .replace(/\r\n/g, '\n')
      .replace(/^→[^\n]*\n?/gm, '')
      .replace(/^✓[^\n]*\n?/gm, '')
      .replace(/^✗[^\n]*\n?/gm, '')
      .replace(/^…[^\n]*\n?/gm, '')
      .replace(/\n{3,}/g, '\n\n')
      .trim();

    const parts = cleaned.split(/\n---\n/);
    if (parts.length > 1) {
      const last = parts[parts.length - 1].trim();
      const prev = parts.slice(0, -1).join('\n---\n').trim();
      if (last.length > 200 && prev.includes(last.slice(0, Math.min(120, last.length)))) {
        cleaned = last;
      }
    }

    const half = Math.floor(cleaned.length / 2);
    if (cleaned.length > 800 && cleaned.slice(0, half).trim() === cleaned.slice(half).trim()) {
      cleaned = cleaned.slice(0, half).trim();
    }

    return cleaned;
  }

  function renderMarkdown(text) {
    const prepared = prepareAgentMarkdown(text);
    if (!prepared) return '<p class="agent-md-empty">(empty)</p>';

    const api = markedApi();
    if (!api || !ensureMarkedRenderer()) {
      return `<p class="agent-md-fallback">${escapeHtml(prepared).replace(/\n/g, '<br>')}</p>`;
    }

    let html = api.parse(prepared);

    html = html.replace(
      /<pre><code class="language-mermaid">([\s\S]*?)<\/code><\/pre>/gi,
      (_, code) => mermaidBlock(decodeHtmlEntities(code)),
    );

    if (window.DOMPurify && typeof window.DOMPurify.sanitize === 'function') {
      return window.DOMPurify.sanitize(html, {
        ADD_TAGS: ['pre', 'code', 'span', 'div', 'table', 'thead', 'tbody', 'tr', 'th', 'td', 'hr', 'del', 'input'],
        ADD_ATTR: ['class', 'id', 'href', 'target', 'rel', 'align', 'type', 'checked', 'disabled'],
      });
    }
    return html;
  }

  async function renderMermaidIn(el) {
    if (!el || typeof el.querySelectorAll !== 'function') return;
    const nodes = el.querySelectorAll('pre.mermaid:not([data-processed])');
    if (!nodes.length) return;

    const loaded = await loadMermaidScript();
    if (!loaded || !window.mermaid) {
      nodes.forEach((node) => {
        const err = document.createElement('div');
        err.className = 'agent-mermaid-error';
        err.textContent = 'Mermaid failed to load — diagram source shown above.';
        node.closest('.agent-mermaid-wrap')?.appendChild(err);
      });
      return;
    }

    initMermaid();
    for (const node of nodes) {
      try {
        await window.mermaid.run({ nodes: [node], suppressErrors: false });
      } catch (e) {
        const err = document.createElement('div');
        err.className = 'agent-mermaid-error';
        err.textContent = `Diagram error: ${e.message || String(e)}`;
        node.closest('.agent-mermaid-wrap')?.appendChild(err);
      }
    }
  }

  function renderPlain(el, text) {
    if (!el) return;
    el.classList.remove('agent-text-rich');
    el.classList.add('agent-text-plain', 'whitespace-pre-wrap');
    el.textContent = text || '';
  }

  async function renderAgentContent(el, text, { streaming = false } = {}) {
    if (!el) return;
    if (streaming || !text || text === '…') {
      renderPlain(el, text);
      return;
    }

    if (!libsReady()) {
      console.error('[Reaper] Markdown libs missing — marked:', !!markedApi(), 'DOMPurify:', !!(window.DOMPurify?.sanitize));
      renderPlain(el, prepareAgentMarkdown(text) || text);
      return;
    }

    try {
      el.classList.remove('agent-text-plain', 'whitespace-pre-wrap');
      el.classList.add('agent-text-rich');
      el.innerHTML = renderMarkdown(text);
    } catch (e) {
      console.error('[Reaper] Agent markdown render failed', e);
      renderPlain(el, prepareAgentMarkdown(text) || text);
      return;
    }

    try {
      await renderMermaidIn(el);
    } catch (e) {
      // Keep rendered markdown/tables even if diagram rendering fails.
      console.error('[Reaper] Mermaid render failed', e);
    }
  }

  window.ReaperAgentMarkdown = {
    renderAgentContent,
    renderPlain,
    prepareAgentMarkdown,
    libsReady,
  };
})();
