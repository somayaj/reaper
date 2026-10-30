import http from "node:http";
import { randomUUID } from "node:crypto";
import { Agent, Cursor } from "@cursor/sdk";

const PORT = Number(process.env.REAPER_CURSOR_BRIDGE_PORT || 8091);
const sessions = new Map();
const CONTEXT_LIMIT = 8000;

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

function clip(text) {
  if (!text) return "";
  return text.length <= CONTEXT_LIMIT ? text : text.slice(-CONTEXT_LIMIT);
}

function withPriorTurn(prompt, session) {
  if (!session.lastUser && !session.lastAssistant) return prompt;
  const parts = ["Previous turn (context only, do not repeat it):"];
  if (session.lastUser) parts.push(`User: ${clip(session.lastUser)}`);
  if (session.lastAssistant) parts.push(`Assistant: ${clip(session.lastAssistant)}`);
  parts.push("", "New request:", prompt);
  return parts.join("\n");
}

async function disposeAgent(agent) {
  if (!agent) return;
  try {
    await agent[Symbol.asyncDispose]?.();
  } catch {
    try {
      await agent.close?.();
    } catch {
      /* ignore */
    }
  }
}

function createAgent(session) {
  return Agent.create({
    apiKey: session.apiKey,
    model: { id: session.model || "composer-2.5" },
    mode: normalizeSdkMode(session.mode || "agent"),
    local: { cwd: session.cwd, settingSources: [] },
  });
}

function armNextAgent(session) {
  session.agent = null;
  const pending = createAgent(session)
    .then((agent) => {
      if (session.closed) {
        disposeAgent(agent);
        return null;
      }
      session.agent = agent;
      return agent;
    })
    .finally(() => {
      if (session.warming === pending) session.warming = null;
    });
  session.warming = pending;
  return pending;
}

async function readyAgent(session) {
  if (session.agent) return session.agent;
  if (session.warming) return session.warming;
  return armNextAgent(session);
}

async function disposeSession(id) {
  const session = sessions.get(id);
  if (!session) return false;
  session.closed = true;
  sessions.delete(id);
  const current = session.agent;
  session.agent = null;
  if (current) await disposeAgent(current);
  if (session.warming) {
    session.warming.then((next) => disposeAgent(next)).catch(() => {});
  }
  return true;
}

function emit(res, payload) {
  res.write(`data: ${JSON.stringify(payload)}\n\n`);
}

function emitActivity(res, payload) {
  emit(res, { type: "activity", ...payload });
}

function friendlyToolActivity(name, path, status) {
  const tool = (name || "").toLowerCase();
  const short = path ? String(path).replace(/\\/g, "/").split("/").filter(Boolean).slice(-2).join("/") : null;
  if (status === "completed" || status === "error") return null;
  if (tool.includes("read")) return short ? `Reading ${short}…` : "Reading…";
  if (tool.includes("write") || tool.includes("edit") || tool.includes("replace") || tool.includes("patch")) {
    return short ? `Editing ${short}…` : "Editing…";
  }
  if (tool.includes("shell") || tool.includes("bash") || tool.includes("terminal")) return "Running command…";
  if (tool.includes("grep") || tool.includes("glob") || tool.includes("search")) {
    return short ? `Searching ${short}…` : "Searching…";
  }
  if (tool.includes("delete")) return short ? `Deleting ${short}…` : "Deleting…";
  if (tool.includes("task") || tool.includes("agent")) return "Delegating…";
  if (tool.includes("web") || tool.includes("fetch")) return "Fetching…";
  if (name) return short ? `${name}: ${short}…` : `${name}…`;
  return "Working…";
}

function handleSdkDelta(res, update) {
  if (!update || typeof update !== "object") return;
  switch (update.type) {
    case "thinking-delta":
      emitActivity(res, { text: "Thinking…", phase: "thinking" });
      break;
    case "thinking-completed":
      emitActivity(res, { text: "Thinking…", phase: "thinking" });
      break;
    case "tool-call-started":
      emitActivity(res, {
        text: friendlyToolActivity(update.name || update.toolName, toolTargetPath(update.args), "running"),
        tool: update.name || update.toolName || null,
        path: toolTargetPath(update.args),
        status: "running",
      });
      break;
    case "step-started":
      emitActivity(res, { text: "Working…", phase: "step" });
      break;
    default:
      break;
  }
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

function handleSdkMessage(res, event, collect) {
  if (event.type === "status") {
    if (event.status === "ERROR" && event.message) {
      emit(res, { type: "error", error: event.message });
    }
    return;
  }

  if (event.type === "assistant") {
    for (const block of event.message?.content || []) {
      if (block.type === "text" && block.text) {
        if (collect) collect.text += block.text;
        emit(res, { type: "text", text: block.text });
      } else if (block.type === "tool_use" && block.name) {
        const path = toolTargetPath(block.input);
        emitActivity(res, {
          text: friendlyToolActivity(block.name, path, "running"),
          tool: block.name,
          path,
          status: "running",
        });
      }
    }
    return;
  }

  if (event.type === "tool_call") {
    const path = toolTargetPath(event.args);
    if (event.status === "running") {
      emitActivity(res, {
        text: friendlyToolActivity(event.name, path, "running"),
        tool: event.name || null,
        path,
        status: "running",
      });
      return;
    }
    const text = describeToolCall(event);
    if (text) {
      emitTool(res, text, {
        tool: event.name || null,
        path,
        status: event.status || null,
      });
    }
    return;
  }

  if (event.type === "task" && event.text) {
    emitActivity(res, { text: "Delegating…", phase: "task", detail: event.text.slice(0, 120) });
    return;
  }

  if (event.type === "thinking") {
    emitActivity(res, {
      text: "Thinking…",
      phase: "thinking",
      detail: event.text ? event.text.slice(0, 120) : null,
    });
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

      const sessionId = randomUUID();
      const now = Date.now();
      const session = {
        cwd,
        apiKey,
        model: model || "composer-2.5",
        mode: mode || "agent",
        agent: null,
        warming: null,
        createdAt: now,
        lastUsedAt: now,
        activeChat: null,
        lastUser: "",
        lastAssistant: "",
        closed: false,
        busy: false,
      };
      sessions.set(sessionId, session);
      await armNextAgent(session);
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
      if (session.busy || session.activeChat?.run) {
        return json(res, 409, { error: "agent run already in progress" });
      }

      const body = await readBody(req);
      const rawPrompt = body.prompt?.trim();
      if (!rawPrompt) {
        return json(res, 400, { error: "prompt required" });
      }
      const mode = body.mode || session.mode || "agent";
      if (body.model) session.model = body.model;
      session.mode = mode;
      session.busy = true;

      res.writeHead(200, {
        "Content-Type": "text/event-stream",
        "Cache-Control": "no-cache",
        Connection: "keep-alive",
      });

      let agent;
      let run;
      const collect = { text: "" };
      try {
        agent = await readyAgent(session);
        if (!agent) throw new Error("Cursor agent failed to start");
        session.agent = null;
        const prompt = withPriorTurn(preparePrompt(rawPrompt, mode), session);
        const sendOptions = {
          mode: normalizeSdkMode(mode),
          local: { force: true },
        };
        if (session.model) sendOptions.model = { id: session.model };
        sendOptions.onDelta = (update) => handleSdkDelta(res, update);
        emitActivity(res, { text: "Starting agent…", phase: "start" });
        run = await agent.send(prompt, sendOptions);
      } catch (err) {
        session.busy = false;
        if (agent) {
          await disposeAgent(agent);
          if (!session.closed) armNextAgent(session);
        }
        emit(res, { type: "error", error: err instanceof Error ? err.message : String(err) });
        res.end();
        return;
      }

      session.lastUsedAt = Date.now();
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
          handleSdkMessage(res, event, collect);
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
        session.lastUser = rawPrompt;
        session.lastAssistant = collect.text.trim();
        session.busy = false;
        await disposeAgent(agent);
        if (!session.closed) armNextAgent(session);
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
