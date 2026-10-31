# Reaper 0.1.19

**UI build 502**

## Fixes

- **Agent (Cursor):** Loopback WebSocket chat in the native app (SSE via `reaper://` could not stream).
- **Agent (Cursor):** Cursor bridge uses **Node.js 23+** when available — bundled Node 22 + `@cursor/sdk` 1.2 could hang in `agent.send()` and truncate SSE (`error decoding response body`).
- **Agent (Cursor):** Bridge child **stdout** discarded; ignore SQLite stderr noise for bridge health.
- **Agent (Cursor):** Stable bridge URL during chat; HTTP/1.1 client for bridge SSE proxy.
- **Agent (Cursor):** WebKit no longer shows **Agent connection failed** after a successful WS stream.

## Includes

- **`@cursor/sdk` ^1.2.0** with bridge warm/send improvements from 0.1.18.

## Install

Download from [GitHub Releases v0.1.19](https://github.com/reaper-org/releases/releases/tag/v0.1.19).
