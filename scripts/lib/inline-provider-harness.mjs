/**
 * Inline AI provider chain regression — Cursor → Gemini → Claude → LSP fallback.
 */

function extractRustFnBody(src, name) {
  const block = src.match(new RegExp(`pub async fn ${name}[\\s\\S]*?(?=\\npub async fn |\\npub fn |\\nfn )`))?.[0]
    || src.match(new RegExp(`pub fn ${name}[\\s\\S]*?(?=\\npub async fn |\\npub fn |\\nfn )`))?.[0]
    || '';
  const brace = block.indexOf('{');
  if (brace === -1) return '';
  let depth = 1;
  for (let i = brace + 1; i < block.length; i += 1) {
    const ch = block[i];
    if (ch === '{') depth += 1;
    else if (ch === '}') {
      depth -= 1;
      if (depth === 0) return block.slice(brace + 1, i);
    }
  }
  return '';
}

function extractEmptyLineInlineBlock(langSrc) {
  const start = langSrc.indexOf('if (isWhitespaceOnlyLine(linePrefix)) {');
  const anchor = start !== -1
    ? langSrc.indexOf('const emptyLspCached = inlineSuffixFromCachedIndex', start)
    : -1;
  if (anchor === -1) return '';
  const blockStart = langSrc.lastIndexOf('if (isWhitespaceOnlyLine(linePrefix)) {', anchor);
  const end = langSrc.indexOf(
    'if (shouldPreferLspInlineGhost(path, linePrefix, content, position.lineNumber))',
    blockStart,
  );
  if (blockStart === -1 || end === -1) return '';
  return langSrc.slice(blockStart, end);
}

export function testInlineProviderRegression(appSrc, langSrc, agentModSrc, apiSrc, cursorRsSrc, ok) {
  ok(
    appSrc.includes('function getAiInlineProviderAvailable()'),
    'inline provider: getAiInlineProviderAvailable defined in app.js',
  );
  ok(
    appSrc.includes('state.cursorConfigured && state.cursorBridgeOk')
      && appSrc.includes('state.geminiConfigured')
      && appSrc.includes('state.anthropicConfigured'),
    'inline provider: availability checks Cursor then Gemini then Claude',
  );
  ok(
    appSrc.indexOf('state.cursorConfigured && state.cursorBridgeOk')
      < appSrc.indexOf('state.geminiConfigured')
      && appSrc.indexOf('state.geminiConfigured')
        < appSrc.indexOf('state.anthropicConfigured'),
    'inline provider: app.js provider order is Cursor → Gemini → Claude',
  );
  ok(
    appSrc.includes('getAiInlineProviderAvailable: () => getAiInlineProviderAvailable()'),
    'inline provider: helper exported to Monaco editor',
  );
  ok(
    appSrc.includes('getCursorInlineAvailable: () => state.cursorConfigured && state.cursorBridgeOk'),
    'inline provider: cursor bridge availability helper exported',
  );
  ok(
    appSrc.includes('await warmCursorSession(state.repo)'),
    'inline provider: agent chat awaits cursor session warm before first message',
  );

  ok(
    langSrc.includes('getAiInlineProviderAvailable?.()'),
    'inline provider: Monaco gates AI inline on configured provider',
  );
  ok(
    langSrc.includes('return !!helpers.getAiInlineComplete?.() && !!helpers.getAiInlineProviderAvailable?.()'),
    'inline provider: aiInlineFetchEnabled requires setting and provider',
  );
  ok(
    langSrc.includes('const INLINE_AI_FETCH_MS = 18000'),
    'inline provider: AI fetch timeout allows full provider chain',
  );

  const emptyLineBlock = extractEmptyLineInlineBlock(langSrc);
  ok(!!emptyLineBlock, 'inline provider: empty-line inline block present');
  const aiFetchIdx = emptyLineBlock.indexOf('scheduleAiInlineFetch()');
  const lspIdx = emptyLineBlock.indexOf('const emptyLspCached = inlineSuffixFromCachedIndex');
  const contextIdx = emptyLineBlock.indexOf('const emptyLocal = inferEmptyLineContinuationSuffix(');
  ok(lspIdx !== -1, 'inline provider: empty line falls back to LSP cache');
  ok(contextIdx !== -1, 'inline provider: empty line uses context template after LSP');
  ok(
    lspIdx < contextIdx,
    'inline provider: empty line order is LSP cache before context templates',
  );
  ok(
    !emptyLineBlock.includes('await fetchInlineComplete(model, position, linePrefix, false)'),
    'inline provider: empty line does not block provider on AI network fetch',
  );
  ok(
    aiFetchIdx !== -1,
    'inline provider: empty line schedules async AI fetch',
  );

  ok(
    langSrc.includes("local_only: localOnly"),
    'inline provider: inline-complete request sends local_only flag',
  );
  ok(
    langSrc.includes('fetchInlineComplete(model, position, linePrefix, true)'),
    'inline provider: local_only path skips AI on non-AI routes',
  );

  ok(
    agentModSrc.includes('cursor_bridge: Option<&crate::cursor::CursorBridge>'),
    'inline provider: suggest_inline_completion accepts cursor bridge',
  );
  ok(
    agentModSrc.includes('try_inline_via_cursor'),
    'inline provider: cursor inline helper defined',
  );
  ok(
    agentModSrc.includes('try_inline_via_gemini'),
    'inline provider: gemini inline helper defined',
  );
  ok(
    agentModSrc.includes('try_inline_via_anthropic'),
    'inline provider: anthropic inline helper defined',
  );
  ok(
    agentModSrc.includes('INLINE_CURSOR_TIMEOUT'),
    'inline provider: cursor inline timeout defined',
  );
  ok(
    agentModSrc.includes('INLINE_GEMINI_TIMEOUT'),
    'inline provider: gemini inline timeout defined',
  );

  const suggestBody = extractRustFnBody(agentModSrc, 'suggest_inline_completion');
  ok(!!suggestBody, 'inline provider: suggest_inline_completion body extracted');
  const aiChain = suggestBody.slice(suggestBody.indexOf('let context ='));
  ok(
    aiChain.includes('try_inline_via_cursor')
      && aiChain.includes('try_inline_via_gemini')
      && aiChain.includes('try_inline_via_anthropic')
      && aiChain.includes('apply_inline_fallback'),
    'inline provider: backend walks AI providers then LSP fallback',
  );
  const cursorIdx = aiChain.indexOf('try_inline_via_cursor');
  const geminiIdx = aiChain.indexOf('try_inline_via_gemini');
  const anthropicIdx = aiChain.indexOf('try_inline_via_anthropic');
  const fallbackIdx = aiChain.lastIndexOf('apply_inline_fallback');
  ok(
    cursorIdx !== -1 && geminiIdx !== -1 && anthropicIdx !== -1 && fallbackIdx !== -1,
    'inline provider: all backend steps present',
  );
  ok(
    cursorIdx < geminiIdx && geminiIdx < anthropicIdx && anthropicIdx < fallbackIdx,
    'inline provider: backend order is Cursor → Gemini → Claude → LSP fallback',
  );
  ok(
    suggestBody.includes('if local_only')
      && suggestBody.indexOf('if local_only') < suggestBody.indexOf('let context ='),
    'inline provider: local_only skips AI provider chain',
  );

  ok(
    apiSrc.includes('Some(&state.cursor_bridge)'),
    'inline provider: API passes cursor bridge into suggest_inline_completion',
  );
  ok(
    apiSrc.includes('git_agent::suggest_inline_completion'),
    'inline provider: workspace inline-complete endpoint wired',
  );

  const cursorFn = agentModSrc.match(
    /async fn try_inline_via_cursor\([\s\S]*?(?=\nasync fn |\nfn )/,
  )?.[0] || '';
  ok(
    cursorFn.includes('health_cached') && cursorFn.includes('INLINE_CURSOR_TIMEOUT'),
    'inline provider: cursor inline checks bridge health and times out',
  );

  const cursorSessionFn = agentModSrc.match(
    /async fn suggest_inline_completion_via_cursor\([\s\S]*?(?=\nasync fn |\nfn )/,
  )?.[0] || '';
  ok(
    cursorSessionFn.includes('create_session') && cursorSessionFn.includes('"ask"'),
    'inline provider: cursor inline uses ephemeral ask session',
  );

  const anthropicFn = agentModSrc.match(
    /async fn try_inline_via_anthropic\([\s\S]*?(?=\nasync fn |\nfn )/,
  )?.[0] || '';
  ok(
    anthropicFn.includes('Claude/Anthropic inline'),
    'inline provider: anthropic slot reserved as third provider',
  );
}

export function testCursorSessionRegression(appSrc, cursorRsSrc, ok) {
  ok(
    cursorRsSrc.includes('fn cursor_session_stale'),
    'cursor session: stale session detector defined',
  );
  ok(
    cursorRsSrc.includes('cursor_chat_stream_with_retry'),
    'cursor session: chat retries with fresh session on stale id',
  );
  ok(
    cursorRsSrc.includes('session not found'),
    'cursor session: handles bridge session-not-found error',
  );
  ok(
    cursorRsSrc.includes('cursor_sessions.drain_all()'),
    'cursor session: bridge restart clears cached session ids',
  );
  const retryBody = cursorRsSrc.match(
    /async fn cursor_chat_stream_with_retry\([\s\S]*?(?=\nasync fn cursor_chat)/,
  )?.[0] || '';
  ok(!!retryBody, 'cursor session: retry helper body present');
  ok(
    retryBody.includes('for attempt in 0..2'),
    'cursor session: single automatic retry on stale session',
  );
  ok(
    retryBody.includes('cursor_sessions.remove(name)'),
    'cursor session: evicts stale session id before retry',
  );
  ok(
    appSrc.includes('await warmCursorSession(state.repo)'),
    'cursor session: UI awaits warm session before cursor chat',
  );
  const restartBridgeBody = appSrc.match(/async function restartBridge\(\) \{[\s\S]*?\n\}/)?.[0] || '';
  ok(
    restartBridgeBody.includes('await warmCursorSession(state.repo)'),
    'cursor session: bridge restart re-warms repo session',
  );
}
