/**
 * Cursor agent chat regression — loopback WebSocket on reaper:// (SSE via custom protocol breaks).
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

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

/** Mirror static/app.js drainAgentSseEvents for unit tests. */
export function drainAgentSseEvents(sseBuffer, onEvent) {
  const parts = sseBuffer.split('\n\n');
  const rest = parts.pop() || '';
  for (const part of parts) {
    if (!part.trim() || part.trimStart().startsWith(':')) continue;
    const line = part.split('\n').find((l) => l.startsWith('data: '));
    if (!line) continue;
    let data;
    try {
      data = JSON.parse(line.slice(6));
    } catch {
      continue;
    }
    onEvent(data);
  }
  return rest;
}

export function testDrainAgentSseEventsUnit(ok) {
  const events = [];
  let buf = ': keepalive\n\ndata: {"type":"activity","text":"Warming…"}\n\n';
  buf = drainAgentSseEvents(buf, (e) => events.push(e));
  ok(events.length === 1 && events[0].type === 'activity', 'cursor SSE drain: skips keepalive, parses activity event');
  ok(buf === '', 'cursor SSE drain: consumes complete events');

  buf += 'data: {"type":"text","text":"hi"}\n\n';
  buf = drainAgentSseEvents(buf, (e) => events.push(e));
  ok(events.length === 2 && events[1].text === 'hi', 'cursor SSE drain: parses text event');

  buf = 'data: {"type":"done","status":"finished"}\n\npartial';
  buf = drainAgentSseEvents(buf, (e) => events.push(e));
  ok(events[events.length - 1].type === 'done', 'cursor SSE drain: parses done event');
  ok(buf === 'partial', 'cursor SSE drain: leaves incomplete trailing buffer');
}

export function testCursorAgentLoopbackWsRegression(appSrc, cursorRs, customProtocolRs, ok) {
  ok(
    cursorRs.includes('/cursor/chat/ws') && cursorRs.includes('run_cursor_chat_ws'),
    'backend: Cursor agent chat WebSocket route proxies bridge SSE',
  );
  ok(
    cursorRs.includes('Message::Binary') && cursorRs.includes('bytes_stream()'),
    'backend: chat WS forwards bridge SSE bytes to the WebView',
  );
  ok(
    customProtocolRs.includes('body.collect()') || customProtocolRs.includes('.collect()'),
    'gui: custom protocol buffers full HTTP bodies (SSE chat cannot stream via reaper://)',
  );

  ok(appSrc.includes('function drainAgentSseEvents'), 'cursor agent: shared SSE frame parser for fetch and WS');
  ok(appSrc.includes('function consumeAgentChatLoopbackWs'), 'cursor agent: loopback WebSocket consumer exists');
  ok(appSrc.includes('function processAgentStreamEvent'), 'cursor agent: shared stream event handler');

  const runBody = extractFunctionBody(appSrc, 'runAgentChat');
  ok(runBody.includes('useLoopbackAgentWs'), 'cursor agent: runAgentChat chooses loopback WS in native app');
  ok(
    runBody.includes("def.chatPath === '/cursor/chat'"),
    'cursor agent: loopback WS limited to Cursor provider path',
  );
  ok(
    runBody.includes('consumeAgentChatLoopbackWs'),
    'cursor agent: native app uses WebSocket instead of reaper:// POST stream',
  );
  ok(
    !runBody.includes('resolveStreamingApiUrl'),
    'cursor agent: does not loopback HTTP fetch from reaper:// (WebKit Load failed)',
  );

  const wsBody = extractFunctionBody(appSrc, 'consumeAgentChatLoopbackWs');
  ok(wsBody.includes('ensureLoopbackWsBase()'), 'cursor agent: WS resolves loopback base like terminal');
  ok(wsBody.includes('new WebSocket(wsUrl)'), 'cursor agent: opens chat WebSocket on loopback');
  ok(wsBody.includes("binaryType = 'arraybuffer'"), 'cursor agent: WS receives binary bridge SSE chunks');
  ok(wsBody.includes('drainAgentSseEvents'), 'cursor agent: WS path reuses SSE parser');

  const fetchBranch = runBody.slice(runBody.indexOf('} else {'), runBody.indexOf('buffer = streamCtx'));
  ok(fetchBranch.includes('repoApi(state.repo, def.chatPath)'), 'cursor agent: browser/HTTP mode still POSTs via repo API');
}

export function testCursorAgentSourceFilesExist(ok) {
  ok(
    fs.existsSync(path.join(ROOT, 'src/web/cursor.rs')),
    'cursor agent: cursor.rs present',
  );
  ok(
    fs.existsSync(path.join(ROOT, 'cursor-bridge/server.mjs')),
    'cursor agent: cursor-bridge server present',
  );
}
