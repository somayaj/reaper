import "./windows-hide-patch.mjs";
import http from "node:http";
import { randomUUID } from "node:crypto";
import { Agent, Cursor } from "@cursor/sdk";

const PORT = Number(process.env.REAPER_CURSOR_BRIDGE_PORT || 8091);
const sessions = new Map();

function json(res, status, body) {
  res.writeHead(status, { "Content-Type": "application/json" });
  res.end(JSON.stringify(body));
}

function normalizeSdkMode(mode) {
  if (mode === "plan" || mode === "ask") return "plan";
  return "agent";
}

function preparePrompt(prompt, mode) {
  if (mode !== "ask") return prompt;
  return `[Ask mode — answer questions about the codebase only. Do not edit, create, or delete files, and do not run commands that modify anything.]\n\n${prompt}`;
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
  const path = toolTargetPath(event.args);
  if (event.status === "running") {
    return path ? `→ ${name}: ${path}` : `→ ${name}…`;
  }
  if (event.status === "completed") return path ? `✓ ${name}: ${path}` : `✓ ${name}`;
  if (event.status === "error") return `✗ ${name} failed`;
  return null;
}

function toolTargetPath(args) {
  if (!args || typeof args !== "object") return null;
  return args.path || args.file || args.file_path || args.target || null;
}

function emitTool(res, text, extra = {}) {
  emit(res, { type: "tool", text: `${text}\n`, ...extra });
}

function handleSdkMessage(res, event) {
  if (event.type === "status") {
    if (event.status === "ERROR" && event.message) {
      emit(res, { type: "error", error: event.message });
    }
    return;
  }

  if (event.type === "assistant") {
    for (const block of event.message?.content || []) {
      if (block.type === "text" && block.text) {
        emit(res, { type: "text", text: block.text });
      } else if (block.type === "tool_use" && block.name) {
        const path = toolTargetPath(block.input);
        emitTool(res, path ? `→ ${block.name}: ${path}` : `→ ${block.name}…`, {
          tool: block.name,
          path,
        });
      }
    }
    return;
  }

  if (event.type === "tool_call") {
    const text = describeToolCall(event);
    if (text) {
      emitTool(res, text, {
        tool: event.name || null,
        path: toolTargetPath(event.args),
        status: event.status || null,
      });
    }
    return;
  }

  if (event.type === "task" && event.text) {
    emitTool(res, event.text);
    return;
  }

  if (event.type === "thinking" && event.text) {
    emitTool(res, `… ${event.text.slice(0, 120)}${event.text.length > 120 ? "…" : ""}`);
  }
}

const server = http.createServer(async (req, res) => {
  try {
    const url = new URL(req.url || "/", `http://127.0.0.1:${PORT}`);

    if (req.method === "GET" && url.pathname === "/health") {
      return json(res, 200, { ok: true, sessions: sessions.size });
    }

    if (req.method === "POST" && url.pathname === "/models") {
      const body = await readBody(req);
      const { apiKey } = body;
      if (!apiKey) {
        return json(res, 400, { error: "apiKey required" });
      }
      const models = await Cursor.models.list({ apiKey });
      return json(res, 200, {
        models: models.map((m) => ({
          id: m.id,
          label: m.displayName || m.id,
          description: m.description || null,
        })),
      });
    }

    if (req.method === "POST" && url.pathname === "/sessions") {
      const body = await readBody(req);
      const { cwd, apiKey, model, mode } = body;
      if (!cwd || !apiKey) {
        return json(res, 400, { error: "cwd and apiKey required" });
      }

      const sdkMode = normalizeSdkMode(mode || "agent");
      const agent = await Agent.create({
        apiKey,
        model: { id: model || "composer-2.5" },
        mode: sdkMode,
        local: { cwd, settingSources: [] },
      });

      const sessionId = randomUUID();
      sessions.set(sessionId, { agent, cwd, createdAt: Date.now(), activeChat: null });
      return json(res, 201, { sessionId });
    }

    const stopMatch = url.pathname.match(/^\/sessions\/([^/]+)\/stop$/);
    if (req.method === "POST" && stopMatch) {
      const sessionId = decodeURIComponent(stopMatch[1]);
      const session = sessions.get(sessionId);
      if (!session?.activeChat?.run) {
        return json(res, 404, { error: "no active run" });
      }
      const { run, res: sseRes } = session.activeChat;
      session.activeChat = null;
      try {
        await run.cancel();
      } catch {
        /* ignore */
      }
      if (sseRes && !sseRes.writableEnded) {
        emit(sseRes, { type: "done", status: "cancelled" });
        sseRes.end();
      }
      return json(res, 200, { ok: true });
    }

    const chatMatch = url.pathname.match(/^\/sessions\/([^/]+)\/chat$/);
    if (req.method === "POST" && chatMatch) {
      const sessionId = decodeURIComponent(chatMatch[1]);
      const session = sessions.get(sessionId);
      if (!session) {
        return json(res, 404, { error: "session not found" });
      }
      if (session.activeChat?.run) {
        return json(res, 409, { error: "agent run already in progress" });
      }

      const body = await readBody(req);
      const rawPrompt = body.prompt?.trim();
      if (!rawPrompt) {
        return json(res, 400, { error: "prompt required" });
      }
      const mode = body.mode || "agent";
      const prompt = preparePrompt(rawPrompt, mode);
      const sdkMode = normalizeSdkMode(mode);
      const sendOptions = { mode: sdkMode };
      if (body.model) {
        sendOptions.model = { id: body.model };
      }

      res.writeHead(200, {
        "Content-Type": "text/event-stream",
        "Cache-Control": "no-cache",
        Connection: "keep-alive",
      });

      let run;
      try {
        run = await session.agent.send(prompt, sendOptions);
      } catch (err) {
        emit(res, { type: "error", error: err instanceof Error ? err.message : String(err) });
        res.end();
        return;
      }

      session.activeChat = { run, res };

      const onClose = () => {
        if (session.activeChat?.run === run) {
          session.activeChat = null;
          run.cancel().catch(() => {});
        }
      };
      req.on("close", onClose);
      res.on("close", onClose);

      try {
        for await (const event of run.stream()) {
          if (session.activeChat?.run !== run) break;
          handleSdkMessage(res, event);
        }

        if (session.activeChat?.run === run) {
          const result = await run.wait();
          if (result.status === "error") {
            const detail =
              result.result?.trim() ||
              run.result?.trim() ||
              "Cursor agent run failed — try Settings → Retry bridge, or restart Reaper";
            console.error("cursor-bridge agent run error:", JSON.stringify(result));
            emit(res, { type: "error", error: detail });
          }
          emit(res, {
            type: "done",
            status: result.status,
            runId: result.id,
            summary: result.result || null,
            error: result.status === "error" ? result.result || null : null,
          });
        }
      } catch (err) {
        if (session.activeChat?.run === run) {
          const message = err instanceof Error ? err.message : String(err);
          console.error("cursor-bridge chat error:", message);
          emit(res, { type: "error", error: message });
        }
      } finally {
        if (session.activeChat?.run === run) {
          session.activeChat = null;
        }
        if (!res.writableEnded) {
          res.end();
        }
      }
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
