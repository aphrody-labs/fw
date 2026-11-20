// fw web UI. One script for `fw serve` and the fw edge Worker: everything it shows
// comes from the server it is loaded from (/health, /v1/models, /mcp, /openapi.json).

const STORE_KEY = "fw_chat_v1";
const CFG_KEY = "fw_chat_cfg_v1";
const $ = (id) => document.getElementById(id);
const VIEWS = ["chat", "tools", "models", "system"];

const load = (key, fallback) => {
  try { return JSON.parse(localStorage.getItem(key) ?? "") ?? fallback; } catch { return fallback; }
};
let convs = load(STORE_KEY, []);
const cfg = Object.assign(
  { systemPrompt: "You are fw, a concise engineering assistant. Prefer exact commands and code.", endpoint: "/v1/chat/completions", temperature: 0.7, theme: "", model: "" },
  load(CFG_KEY, {}),
);
let activeId = convs[0]?.id ?? null;
let inflight = null;
let tools = [];
let models = [];

const save = () => {
  localStorage.setItem(STORE_KEY, JSON.stringify(convs));
  localStorage.setItem(CFG_KEY, JSON.stringify(cfg));
};
const esc = (s) => String(s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);
const icon = (name) => `<svg class="icon" aria-hidden="true"><use href="#i-${name}"/></svg>`;
const activeConv = () => convs.find((c) => c.id === activeId) ?? null;

function snack(text) {
  const el = $("snackbar");
  el.textContent = text;
  el.hidden = false;
  clearTimeout(snack.t);
  snack.t = setTimeout(() => { el.hidden = true; }, 3200);
}

async function copy(text) {
  try { await navigator.clipboard.writeText(text); snack("Copied"); } catch { snack("Copy is blocked in this browser"); }
}

// ---------- Markdown (escaped first, then a small safe subset) ----------
function inline(text) {
  return esc(text)
    .replace(/`([^`]+)`/g, "<code>$1</code>")
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/(^|[^*])\*([^*\n]+)\*/g, "$1<em>$2</em>")
    .replace(/\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)/g, '<a href="$2" target="_blank" rel="noopener noreferrer">$1</a>');
}

function renderMarkdown(src) {
  const out = [];
  const parts = String(src ?? "").split(/```([\w+-]*)\n?([\s\S]*?)(?:```|$)/g);
  for (let i = 0; i < parts.length; i += 3) {
    const prose = parts[i];
    if (prose?.trim()) {
      let list = null;
      let para = [];
      const flush = () => {
        if (para.length) { out.push(`<p>${para.map(inline).join("<br>")}</p>`); para = []; }
        if (list) { out.push(`</${list}>`); list = null; }
      };
      for (const line of prose.split("\n")) {
        const h = /^(#{1,4})\s+(.*)$/.exec(line);
        const ul = /^\s*[-*]\s+(.*)$/.exec(line);
        const ol = /^\s*\d+[.)]\s+(.*)$/.exec(line);
        if (h) { flush(); out.push(`<h${h[1].length > 2 ? 4 : 3}>${inline(h[2])}</h${h[1].length > 2 ? 4 : 3}>`); }
        else if (ul || ol) {
          if (para.length) { out.push(`<p>${para.map(inline).join("<br>")}</p>`); para = []; }
          const kind = ul ? "ul" : "ol";
          if (list !== kind) { if (list) out.push(`</${list}>`); out.push(`<${kind}>`); list = kind; }
          out.push(`<li>${inline((ul ?? ol)[1])}</li>`);
        } else if (!line.trim()) flush();
        else { if (list) { out.push(`</${list}>`); list = null; } para.push(line); }
      }
      flush();
    }
    if (i + 2 < parts.length) {
      const lang = parts[i + 1] || "text";
      out.push(`<div class="code-block"><div class="code-head"><span>${esc(lang)}</span><button type="button" class="icon-btn copy-code" aria-label="Copy code">${icon("copy")}</button></div><pre><code>${esc(parts[i + 2])}</code></pre></div>`);
    }
  }
  return out.join("");
}

// ---------- Navigation ----------
function route() {
  const view = VIEWS.find((v) => location.hash === `#/${v}`) ?? "chat";
  for (const v of VIEWS) $(`view-${v}`).hidden = v !== view;
  document.querySelectorAll(".dest").forEach((a) => {
    if (a.dataset.view === view) a.setAttribute("aria-current", "page");
    else a.removeAttribute("aria-current");
  });
  document.title = view === "chat" ? `${activeConv()?.title ?? "New chat"} - fw` : `${view[0].toUpperCase()}${view.slice(1)} - fw`;
  if (view === "chat") $("prompt").focus({ preventScroll: true });
  if (view === "system") loadSystem();
}

function setThreads(open) {
  $("threads").classList.toggle("open", open);
  $("scrim").hidden = !open;
  $("toggle-threads").setAttribute("aria-expanded", String(open));
}

// ---------- Conversations ----------
function createConv() {
  // An untouched chat is reused, so repeated clicks never stack empty entries.
  convs = convs.filter((x) => x.messages.length);
  const c = { id: `c-${Date.now().toString(36)}`, title: "New chat", messages: [] };
  convs.unshift(c);
  activeId = c.id;
  save();
  if (location.hash !== "#/chat" && location.hash !== "") location.hash = "#/chat";
  renderThreads();
  renderStream();
  setThreads(false);
  $("prompt").focus();
  return c;
}

function renderThreads() {
  const q = $("conv-search").value.trim().toLowerCase();
  const list = $("conv-list");
  const shown = convs.filter((c) => !q || c.title.toLowerCase().includes(q));
  list.innerHTML = shown.length ? "" : `<p class="threads-empty body-medium">${q ? "No chat matches this search." : "Your chats appear here."}</p>`;
  for (const c of shown) {
    const row = document.createElement("div");
    row.className = "conv-item";
    if (c.id === activeId) row.setAttribute("aria-current", "true");
    row.innerHTML = `<a class="ellipsis" href="#/chat">${esc(c.title)}</a><button type="button" class="icon-btn" aria-label="Delete chat ${esc(c.title)}">${icon("delete")}</button>`;
    row.querySelector("a").addEventListener("click", () => {
      activeId = c.id; renderThreads(); renderStream(); setThreads(false);
    });
    row.querySelector("button").addEventListener("click", () => {
      convs = convs.filter((x) => x.id !== c.id);
      if (activeId === c.id) activeId = convs[0]?.id ?? null;
      save(); renderThreads(); renderStream(); snack("Chat deleted");
    });
    list.appendChild(row);
  }
}

function emptyState() {
  return `<div class="empty">
    <div class="mark" aria-hidden="true">fw</div>
    <h2>What should fw do?</h2>
  </div>`;
}

function renderStream() {
  const el = $("stream");
  const conv = activeConv();
  $("chat-title").textContent = conv?.title ?? "New chat";
  if (!$("view-chat").hidden) document.title = `${conv?.title ?? "New chat"} - fw`;
  const empty = !conv || !conv.messages.length;
  $("view-chat").classList.toggle("is-empty", empty);
  if (empty) {
    el.innerHTML = emptyState();
    return;
  }
  el.innerHTML = "";
  conv.messages.forEach((m, idx) => el.appendChild(renderTurn(conv, m, idx)));
  el.scrollTop = el.scrollHeight;
}

function renderTurn(conv, m, idx) {
  const art = document.createElement("article");
  art.className = `turn ${m.error ? "error" : m.role}`;
  if (m.role === "user") {
    art.innerHTML = `<div class="bubble">${esc(m.content)}</div>`;
    return art;
  }
  if (m.error) {
    art.innerHTML = `<div class="bubble"><strong>${esc(m.errorTitle ?? "fw could not answer")}</strong>${esc(m.content)}</div>
      <div class="meta"><button type="button" class="icon-btn retry" aria-label="Try again">${icon("refresh")}</button></div>`;
  } else {
    const isLast = idx === conv.messages.length - 1;
    art.innerHTML = `${m.reasoning ? `<details class="reasoning"><summary>Reasoning</summary><div>${esc(m.reasoning)}</div></details>` : ""}
      <div class="bubble">${renderMarkdown(m.content)}</div>
      <div class="meta">
        <button type="button" class="icon-btn copy-msg" aria-label="Copy answer">${icon("copy")}</button>
        ${isLast ? `<button type="button" class="icon-btn retry" aria-label="Regenerate answer">${icon("refresh")}</button>` : ""}
        <span>${[m.model, m.latencyMs ? `${(m.latencyMs / 1000).toFixed(1)} s` : ""].filter(Boolean).map(esc).join(", ")}</span>
      </div>`;
  }
  art.querySelector(".copy-msg")?.addEventListener("click", () => copy(m.content));
  art.querySelectorAll(".copy-code").forEach((b) => b.addEventListener("click", () => copy(b.closest(".code-block").querySelector("code").textContent)));
  art.querySelector(".retry")?.addEventListener("click", () => {
    if (inflight) return;
    conv.messages.pop();
    save(); renderStream(); send(null);
  });
  return art;
}

// ---------- Sending ----------
function setBusy(busy) {
  $("view-chat").classList.toggle("is-busy", busy);
  const b = $("send-btn");
  b.classList.toggle("stop", busy);
  b.setAttribute("aria-label", busy ? "Stop answer" : "Send");
  b.querySelector("use").setAttribute("href", busy ? "#i-stop" : "#i-send");
}

function describeError(status, payload, fallback) {
  const msg = payload?.error?.message ?? payload?.error ?? fallback;
  const titles = { 400: "The request was rejected", 503: "No model is available", 502: "The model gateway failed" };
  return { title: titles[status] ?? `The server answered HTTP ${status}`, message: typeof msg === "string" ? msg : JSON.stringify(msg) };
}

async function readSse(res, onDelta) {
  const reader = res.body.getReader();
  const dec = new TextDecoder();
  let buf = "";
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    buf += dec.decode(value, { stream: true });
    const frames = buf.split("\n\n");
    buf = frames.pop() ?? "";
    for (const f of frames) {
      const data = f.split("\n").filter((l) => l.startsWith("data:")).map((l) => l.slice(5).trim()).join("");
      if (!data || data === "[DONE]") continue;
      try {
        const ev = JSON.parse(data);
        const d = ev.choices?.[0]?.delta ?? {};
        onDelta(d.content ?? "", d.reasoning_content ?? "", ev.model);
      } catch { /* keep-alive */ }
    }
  }
}

async function send(text) {
  if (inflight) return;
  const conv = activeConv() ?? createConv();
  if (text !== null) {
    const t = text.trim();
    if (!t) return;
    if (!conv.messages.length) conv.title = t.replace(/\s+/g, " ").slice(0, 60);
    conv.messages.push({ role: "user", content: t });
    $("prompt").value = "";
    autosize();
  }
  save(); renderThreads(); renderStream();

  const stream = $("stream");
  const pending = document.createElement("article");
  pending.className = "turn assistant";
  pending.innerHTML = `<div class="thinking"><div class="morph" aria-hidden="true"></div><span class="body-medium">fw is answering</span></div><div class="bubble" hidden></div>`;
  stream.appendChild(pending);
  stream.scrollTop = stream.scrollHeight;

  const ctrl = new AbortController();
  inflight = ctrl;
  setBusy(true);
  const t0 = performance.now();
  const msg = { role: "assistant", content: "", reasoning: "", model: $("model-select").value || undefined };
  try {
    const res = await fetch(cfg.endpoint, {
      method: "POST",
      headers: { "content-type": "application/json", accept: "text/event-stream, application/json" },
      signal: ctrl.signal,
      body: JSON.stringify({
        model: $("model-select").value || undefined,
        temperature: Number(cfg.temperature),
        stream: true,
        messages: [{ role: "system", content: cfg.systemPrompt }, ...conv.messages.filter((m) => !m.error).map((m) => ({ role: m.role, content: m.content }))],
      }),
    });
    const type = res.headers.get("content-type") ?? "";
    if (!res.ok) {
      const body = type.includes("json") ? await res.json().catch(() => null) : null;
      const e = describeError(res.status, body, await res.text().catch(() => res.statusText));
      Object.assign(msg, { error: true, errorTitle: e.title, content: e.message });
    } else if (type.includes("text/event-stream")) {
      const bubble = pending.querySelector(".bubble");
      await readSse(res, (delta, reasoning, model) => {
        msg.content += delta;
        msg.reasoning += reasoning;
        if (model) msg.model = model;
        if (msg.content) {
          pending.querySelector(".thinking").hidden = true;
          bubble.hidden = false;
          bubble.innerHTML = renderMarkdown(msg.content);
          stream.scrollTop = stream.scrollHeight;
        }
      });
    } else {
      const body = await res.json();
      msg.content = body.choices?.[0]?.message?.content ?? "";
      msg.model = body.model ?? msg.model;
    }
    if (!msg.error && !msg.content) Object.assign(msg, { error: true, errorTitle: "The model returned no text", content: "Try again, or pick another model." });
  } catch (err) {
    if (err.name === "AbortError") {
      if (!msg.content) msg.content = "Stopped.";
    } else {
      Object.assign(msg, { error: true, errorTitle: "fw is unreachable", content: `${err.message}. Check that the server is running, then try again.` });
    }
  } finally {
    inflight = null;
    setBusy(false);
    msg.latencyMs = Math.round(performance.now() - t0);
    if (!msg.reasoning) delete msg.reasoning;
    conv.messages.push(msg);
    save(); renderStream();
  }
}

function autosize() {
  const ta = $("prompt");
  ta.style.height = "auto";
  ta.style.height = `${Math.min(ta.scrollHeight, innerHeight * 0.4)}px`;
}

// ---------- Tools ----------
function exampleArgs(schema) {
  const props = schema?.properties ?? {};
  const out = {};
  for (const [k, p] of Object.entries(props)) {
    if (!(schema.required ?? []).includes(k) && Object.keys(out).length >= 3) continue;
    out[k] = p.default ?? p.enum?.[0] ?? ({ string: "", number: 0, integer: 0, boolean: false, array: [], object: {} })[p.type] ?? null;
  }
  return out;
}

function renderTools(error) {
  const list = $("tool-list");
  $("tools-sub").textContent = error ? `Tools could not be listed: ${error}` : tools.length ? `${tools.length} tools on this server. Run one to see its exact output.` : "This server exposes no tools.";
  list.innerHTML = "";
  for (const t of tools) {
    const li = document.createElement("li");
    li.innerHTML = `<div class="item-text"><span class="item-name">${esc(t.name)}</span><span class="item-desc">${esc(t.description ?? "")}</span>${t.source ? `<div class="tags"><span class="tag">${esc(t.source)}</span></div>` : ""}</div>
      <button type="button" class="btn tonal">${icon("play")}Run</button>`;
    li.querySelector("button").addEventListener("click", () => openTool(t.name));
    list.appendChild(li);
  }
}

function renderToolsMenu(error) {
  const menu = $("tools-menu");
  menu.innerHTML = tools.length ? "" : `<p class="menu-empty">${error ? `Tools could not be listed: ${esc(error)}` : "This server exposes no tools."}</p>`;
  for (const t of tools) {
    const b = document.createElement("button");
    b.type = "button"; b.className = "menu-item"; b.setAttribute("role", "menuitem");
    b.innerHTML = `<span class="item-name">${esc(t.name)}</span><span class="item-desc">${esc(t.description ?? "")}</span>`;
    b.addEventListener("click", () => { setToolsMenu(false); openTool(t.name); });
    menu.appendChild(b);
  }
}

function setToolsMenu(open) {
  $("tools-menu").hidden = !open;
  $("tools-btn").setAttribute("aria-expanded", String(open));
  if (open) $("tools-menu").querySelector("button")?.focus();
}

function openTool(name) {
  const t = tools.find((x) => x.name === name);
  if (!t) return;
  $("tool-dlg-title").textContent = t.name;
  $("tool-dlg-desc").textContent = t.description ?? "";
  $("tool-args").value = JSON.stringify(exampleArgs(t.inputSchema), null, 2);
  $("tool-result").hidden = true;
  $("tool-dlg").dataset.tool = t.name;
  $("tool-dlg").showModal();
}

async function runTool() {
  const name = $("tool-dlg").dataset.tool;
  const out = $("tool-result");
  let args;
  try { args = JSON.parse($("tool-args").value || "{}"); } catch (e) {
    out.hidden = false; out.textContent = `Arguments are not valid JSON: ${e.message}`; return;
  }
  const btn = $("tool-run");
  btn.disabled = true;
  out.hidden = false;
  out.textContent = "Running";
  const t0 = performance.now();
  try {
    const res = await rpc("tools/call", { name, arguments: args });
    out.textContent = `${Math.round(performance.now() - t0)} ms\n\n${JSON.stringify(res.result ?? res.error ?? res, null, 2)}`;
  } catch (e) {
    out.textContent = `The call failed: ${e.message}`;
  } finally {
    btn.disabled = false;
  }
}

async function rpc(method, params) {
  const res = await fetch("/mcp", {
    method: "POST",
    headers: { "content-type": "application/json", accept: "application/json, text/event-stream" },
    body: JSON.stringify({ jsonrpc: "2.0", id: Date.now(), method, ...(params ? { params } : {}) }),
  });
  if (!res.ok) throw new Error(`/mcp answered HTTP ${res.status}`);
  return res.json();
}

// ---------- Models ----------
function renderModels(error, defaultId) {
  const sel = $("model-select");
  sel.innerHTML = models.map((m) => `<option value="${esc(m.id)}">${esc(m.id.replace(/^@cf\//, ""))}</option>`).join("");
  const pick = models.some((m) => m.id === cfg.model) ? cfg.model : defaultId ?? models[0]?.id ?? "";
  sel.value = pick;
  sel.disabled = !models.length;
  $("models-sub").textContent = error ? `Models could not be listed: ${error}` : `${models.length} models. The one marked in use answers new messages.`;
  const list = $("model-list");
  list.innerHTML = "";
  for (const m of models) {
    const tags = [
      m.context_window ? `${Math.round(m.context_window / 1024)}K context` : "",
      m.function_calling ? "Tools" : "",
      m.reasoning ? "Reasoning" : "",
      m.vision ? "Images" : "",
      m.paid ? "Paid plan" : "",
    ].filter(Boolean);
    const li = document.createElement("li");
    const inUse = m.id === sel.value;
    li.innerHTML = `<div class="item-text"><span class="item-name">${esc(m.id)}</span>${m.description ? `<span class="item-desc">${esc(m.description.slice(0, 220))}</span>` : ""}
      <div class="tags">${inUse ? '<span class="tag on">In use</span>' : ""}${tags.map((t) => `<span class="tag">${esc(t)}</span>`).join("")}</div></div>
      <button type="button" class="btn ${inUse ? "text" : "tonal"}" ${inUse ? "disabled" : ""}>${inUse ? "In use" : "Use"}</button>`;
    li.querySelector("button").addEventListener("click", () => {
      cfg.model = m.id; save(); renderModels(null, defaultId); snack(`New messages use ${m.id}`);
    });
    list.appendChild(li);
  }
}

// ---------- System ----------
async function loadSystem() {
  const body = $("system-body");
  const sub = $("system-sub");
  sub.textContent = "Checking /health";
  const [health, api] = await Promise.allSettled([
    fetch("/health").then((r) => r.json()),
    fetch("/openapi.json").then((r) => r.json()),
  ]);
  if (health.status !== "fulfilled") {
    sub.textContent = `The server did not answer /health: ${health.reason?.message ?? health.reason}`;
    body.innerHTML = "";
    return;
  }
  const h = health.value;
  sub.textContent = h.ok ? "The server is up." : "The server reports a problem.";
  const facts = [
    ["Product", h.product ?? "fw"],
    ["Version", h.version ?? "unknown"],
    ["Runtime", h.runtime ?? (h.zeroSystemDeps ? "native" : "unknown")],
    ["Environment", h.env ?? "local"],
  ];
  if (h.deployment?.id) facts.push(["Deployment", h.deployment.id.slice(0, 8)]);
  const bindings = Object.entries(h.bindings ?? {});
  const features = h.consolidatedFeatures ?? [];
  const paths = api.status === "fulfilled" ? Object.keys(api.value.paths ?? {}) : [];
  body.innerHTML = `<dl class="facts">${facts.map(([k, v]) => `<div class="fact"><dt>${esc(k)}</dt><dd>${esc(v)}</dd></div>`).join("")}</dl>
    ${bindings.length ? `<h2 class="title-large">Bindings</h2><div class="tags">${bindings.map(([k, v]) => `<span class="tag ${v ? "on" : "off"}">${esc(k)} ${v ? "connected" : "not set"}</span>`).join("")}</div>` : ""}
    ${features.length ? `<h2 class="title-large">Features</h2><div class="tags">${features.map((f) => `<span class="tag">${esc(f)}</span>`).join("")}</div>` : ""}
    ${paths.length ? `<h2 class="title-large">Endpoints</h2><ul class="endpoints">${paths.map((p) => `<li>${esc(p)}</li>`).join("")}</ul>` : ""}`;
}

// ---------- Boot ----------
async function boot() {
  const [m, t] = await Promise.allSettled([fetch("/v1/models").then((r) => r.json()), rpc("tools/list")]);
  if (m.status === "fulfilled") {
    models = m.value.data ?? [];
    renderModels(m.value.error && !models.length ? m.value.error : null, m.value.default);
  } else renderModels(m.reason?.message ?? String(m.reason));
  if (t.status === "fulfilled") {
    tools = t.value.result?.tools ?? [];
    renderTools(t.value.error?.message);
    renderToolsMenu(t.value.error?.message);
  } else {
    renderTools(t.reason?.message ?? String(t.reason));
    renderToolsMenu(t.reason?.message ?? String(t.reason));
  }
  if (!activeConv()?.messages.length) renderStream();
}

function applyTheme() {
  if (cfg.theme) document.documentElement.dataset.theme = cfg.theme;
  else delete document.documentElement.dataset.theme;
}

$("new-chat").addEventListener("click", createConv);
$("new-chat-drawer").addEventListener("click", createConv);
$("conv-search").addEventListener("input", renderThreads);
$("toggle-threads").addEventListener("click", () => setThreads(!$("threads").classList.contains("open")));
$("scrim").addEventListener("click", () => setThreads(false));
$("theme-btn").addEventListener("click", () => {
  const dark = cfg.theme ? cfg.theme === "dark" : matchMedia("(prefers-color-scheme: dark)").matches;
  cfg.theme = dark ? "light" : "dark";
  applyTheme(); save();
});
$("model-select").addEventListener("change", (e) => { cfg.model = e.target.value; save(); renderModels(null); });
$("settings-btn").addEventListener("click", () => {
  $("sys-prompt").value = cfg.systemPrompt;
  $("api-endpoint").value = cfg.endpoint;
  $("temp-input").value = cfg.temperature;
  $("temp-val").textContent = cfg.temperature;
  $("settings-dlg").showModal();
});
$("temp-input").addEventListener("input", (e) => { $("temp-val").textContent = e.target.value; });
$("settings-dlg").addEventListener("close", () => {
  if ($("settings-dlg").returnValue !== "save") return;
  cfg.systemPrompt = $("sys-prompt").value.trim() || cfg.systemPrompt;
  cfg.endpoint = $("api-endpoint").value.trim() || "/v1/chat/completions";
  cfg.temperature = Number($("temp-input").value);
  save(); snack("Settings saved");
});
$("clear-all").addEventListener("click", () => {
  if (!confirm("Delete every chat stored in this browser?")) return;
  convs = []; activeId = null; save(); renderThreads(); renderStream(); $("settings-dlg").close(); snack("All chats deleted");
});
$("tool-run").addEventListener("click", runTool);
$("refresh-system").addEventListener("click", loadSystem);
$("composer").addEventListener("submit", (e) => {
  e.preventDefault();
  if (inflight) inflight.abort();
  else send($("prompt").value);
});
$("prompt").addEventListener("input", autosize);
$("prompt").addEventListener("keydown", (e) => {
  if (e.key === "Enter" && !e.shiftKey && !e.isComposing) { e.preventDefault(); if (!inflight) send($("prompt").value); }
});
$("tools-btn").addEventListener("click", () => setToolsMenu($("tools-menu").hidden));
addEventListener("click", (e) => { if (!e.target.closest("#tools-menu, #tools-btn")) setToolsMenu(false); });
addEventListener("keydown", (e) => {
  if (e.key !== "Escape") return;
  if (!$("tools-menu").hidden) { setToolsMenu(false); $("tools-btn").focus(); } else setThreads(false);
});
addEventListener("hashchange", route);

applyTheme();
renderThreads();
renderStream();
route();
boot();
