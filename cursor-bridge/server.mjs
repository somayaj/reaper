import http from "node:http";
import { randomUUID } from "node:crypto";
import { Agent } from "@cursor/sdk";

const PORT = Number(process.env.REAPER_CURSOR_BRIDGE_PORT || 8091);
const sessions = new Map();

function json(res, status, body) {
  res.writeHead(status, { "Content-Type": "application/json" });
  res.end(JSON.stringify(body));
}

async function readBody(req) {
  const chunks = [];
  for await (const chunk of req) chunks.push(chunk);
  const raw = Buffer.concat(chunks).toString("utf8");
  return raw ? JSON.parse(raw) : {};
}

async function disposeSession(id) {
  const session = sessions.get(id);
  if (!session) return false;
  try {
    await session.agent[Symbol.asyncDispose]?.();
  } catch {
    try {
      await session.agent.close?.();
    } catch {
      /* ignore */
    }
  }
  sessions.delete(id);
  return true;
}

function emit(res, payload) {
  res.write(`data: ${JSON.stringify(payload)}\n\n`);
}

function describeToolCall(event) {
  const name = event.name || "tool";
  if (event.status === "running") {
    const target = event.args?.path || event.args?.file || event.args?.command;
    return target ? `→ ${name}: ${target}` : `→ ${name}…`;
  }
  if (event.status === "completed") return `✓ ${name}`;
  if (event.status === "error") return `✗ ${name} failed`;
  return null;
}

function handleSdkMessage(res, event) {
  if (event.type === "assistant") {
    for (const block of event.message?.content || []) {
      if (block.type === "text" && block.text) {
        emit(res, { type: "text", text: block.text });
      } else if (block.type === "tool_use" && block.name) {
        emit(res, { type: "tool", text: `→ ${block.name}…\n` });
      }
    }
    return;
  }

  if (event.type === "tool_call") {
    const text = describeToolCall(event);
    if (text) emit(res, { type: "tool", text: `${text}\n` });
    return;
  }

  if (event.type === "task" && event.text) {
    emit(res, { type: "tool", text: `${event.text}\n` });
    return;
  }

  if (event.type === "thinking" && event.text) {
    emit(res, { type: "tool", text: `… ${event.text.slice(0, 120)}${event.text.length > 120 ? "…" : ""}\n` });
  }
}

const server = http.createServer(async (req, res) => {
  try {
    const url = new URL(req.url || "/", `http://127.0.0.1:${PORT}`);

    if (req.method === "GET" && url.pathname === "/health") {
      return json(res, 200, { ok: true, sessions: sessions.size });
    }

    if (req.method === "POST" && url.pathname === "/sessions") {
      const body = await readBody(req);
      const { cwd, apiKey, model } = body;
      if (!cwd || !apiKey) {
        return json(res, 400, { error: "cwd and apiKey required" });
      }

      const agent = await Agent.create({
        apiKey,
        model: { id: model || "composer-2.5" },
        local: { cwd, settingSources: [] },
      });

      const sessionId = randomUUID();
      sessions.set(sessionId, { agent, cwd, createdAt: Date.now() });
      return json(res, 201, { sessionId });
    }

    const chatMatch = url.pathname.match(/^\/sessions\/([^/]+)\/chat$/);
    if (req.method === "POST" && chatMatch) {
      const sessionId = decodeURIComponent(chatMatch[1]);
      const session = sessions.get(sessionId);
      if (!session) {
        return json(res, 404, { error: "session not found" });
      }

      const body = await readBody(req);
      const prompt = body.prompt?.trim();
      if (!prompt) {
        return json(res, 400, { error: "prompt required" });
      }

      res.writeHead(200, {
        "Content-Type": "text/event-stream",
        "Cache-Control": "no-cache",
        Connection: "keep-alive",
      });

      const run = await session.agent.send(prompt);
      for await (const event of run.stream()) {
        handleSdkMessage(res, event);
      }

      const result = await run.wait();
      emit(res, {
        type: "done",
        status: result.status,
        runId: result.id,
        summary: result.result || null,
      });
      res.end();
      return;
    }

    const deleteMatch = url.pathname.match(/^\/sessions\/([^/]+)$/);
    if (req.method === "DELETE" && deleteMatch) {
      const sessionId = decodeURIComponent(deleteMatch[1]);
      const removed = await disposeSession(sessionId);
      return json(res, removed ? 204 : 404, removed ? undefined : { error: "session not found" });
    }

    json(res, 404, { error: "not found" });
  } catch (err) {
    console.error(err);
    if (!res.headersSent) {
      json(res, 500, { error: err instanceof Error ? err.message : String(err) });
    } else {
      res.write(`data: ${JSON.stringify({ type: "error", error: err.message })}\n\n`);
      res.end();
    }
  }
});

server.listen(PORT, "127.0.0.1", () => {
  console.log(`Reaper Cursor bridge listening on http://127.0.0.1:${PORT}`);
});

process.on("SIGINT", async () => {
  for (const id of [...sessions.keys()]) {
    await disposeSession(id);
  }
  process.exit(0);
});
