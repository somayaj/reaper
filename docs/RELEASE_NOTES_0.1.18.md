# Reaper 0.1.18

**UI build 501**

## Fixes

- **Agent (Cursor):** Revert bridge to `@cursor/sdk` **1.0.22** so session/agent creation is reliable again (0.1.17’s 1.2.x path could fail validation and return 500 / “chat request failed”).
- **Cursor bridge:** Do not block `POST /sessions` on agent warm; surface `Agent.create` errors when chat runs.
- **Cursor bridge:** Reload saved bridge port from disk before create/chat so a restarted bridge is picked up without “Retry bridge”.

## Still included (from 0.1.17)

- **Agent (Cursor):** Await session warm (up to 12s) before chat when possible for a snappier first reply.

## Install

Download from [GitHub Releases v0.1.18](https://github.com/reaper-org/releases/releases/tag/v0.1.18).
