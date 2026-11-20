const STORE_KEY = "fw_chat_v1";
const CFG_KEY = "fw_chat_cfg_v1";
const $ = id => document.getElementById(id);
let convs = JSON.parse(localStorage.getItem(STORE_KEY) || "[]");
let cfg = Object.assign({
  systemPrompt: "You are Fw, a concise native Rust coding assistant.",
  endpoint: "/v1/chat/completions",
  temperature: 0.7,
  theme: ""
}, JSON.parse(localStorage.getItem(CFG_KEY) || "{}"));
let activeId = convs[0]?.id || null;
let sending = false;

if (cfg.theme) document.documentElement.dataset.theme = cfg.theme;
const save = () => {
  localStorage.setItem(STORE_KEY, JSON.stringify(convs));
  localStorage.setItem(CFG_KEY, JSON.stringify(cfg));
};
const esc = s => String(s).replace(/[&<>"]/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "\"": "&quot;" }[c]));

function renderMarkdown(text) {
  const parts = String(text || "").split(/```(\w*)\n([\s\S]*?)```/g);
  let html = "";
  for (let i = 0; i < parts.length; i++) {
    if (i % 3 === 0) {
      html += esc(parts[i])
        .replace(/`([^`]+)`/g, '<code class="inline">$1</code>')
        .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
        .replace(/\n/g, "<br>");
    } else if (i % 3 === 1) {
      const lang = esc(parts[i] || "text");
      const code = parts[i + 1] || "";
      html += `<div class="code-wrap"><div class="code-hdr"><span>${lang}</span><button type="button" class="btn btn-ghost copy-code">Copy</button></div><pre><code>${esc(code)}</code></pre></div>`;
      i++;
    }
  }
  return html;
}

const activeConv = () => convs.find(c => c.id === activeId) || null;

function createConv() {
  const c = { id: "c-" + Date.now().toString(36), title: "New chat", messages: [] };
  convs.unshift(c);
  activeId = c.id;
  save();
  renderSidebar();
  renderStream();
  $("prompt").focus();
  return c;
}

function renderSidebar() {
  const q = $("conv-search").value.trim().toLowerCase();
  const list = $("conv-list");
  list.innerHTML = "";
  for (const c of convs.filter(x => !q || x.title.toLowerCase().includes(q))) {
    const row = document.createElement("button");
    row.type = "button";
    row.className = "conv-item" + (c.id === activeId ? " active" : "");
    row.innerHTML = `<span class="conv-title">${esc(c.title)}</span><span class="conv-del" title="Delete" aria-label="Delete chat">&times;</span>`;
    row.addEventListener("click", e => {
      if (e.target.classList.contains("conv-del")) {
        e.stopPropagation();
        convs = convs.filter(x => x.id !== c.id);
        if (activeId === c.id) activeId = convs[0]?.id || null;
        save(); renderSidebar(); renderStream();
      } else {
        activeId = c.id; renderSidebar(); renderStream();
      }
    });
    list.appendChild(row);
  }
}

function renderStream() {
  const el = $("stream");
  const conv = activeConv();
  if (!conv || !conv.messages.length) {
    el.innerHTML = `<div class="empty">
      <h1>Fw Chat</h1>
      <p>Native Rust server with local conversation history, Markdown, and zero-latency in-process MCP tools.</p>
      <div class="starters">
        <button type="button" class="btn starter" data-tool="fw_pm_scan" data-prompt="Scan workspace packages (Cargo + npm + APM)">Scan workspace packages</button>
        <button type="button" class="btn starter" data-tool="linux_kernel_mcp" data-prompt="Probe Linux kernel MCP capabilities">Probe kernel MCP</button>
        <button type="button" class="btn starter" data-tool="glob_files" data-prompt="List Rust source files in crates/fw-fx">Index workspace files</button>
      </div>
    </div>`;
    el.querySelectorAll(".starter").forEach(b => b.addEventListener("click", () => {
      $("tool-select").value = b.dataset.tool || "";
      sendMessage(b.dataset.prompt);
    }));
    return;
  }
  el.innerHTML = "";
  conv.messages.forEach((m, idx) => {
    const div = document.createElement("article");
    div.className = "msg " + m.role;
    const toolHtml = m.toolResult
      ? `<details class="tool-card"><summary>Tool: ${esc(m.toolName || "mcp")} (${m.latencyMs || 0} ms)</summary><pre>${esc(JSON.stringify(m.toolResult, null, 2))}</pre></details>`
      : "";
    div.innerHTML = `${toolHtml}<div class="bubble">${m.role === "user" ? esc(m.content) : renderMarkdown(m.content)}</div>
      <div class="msg-meta">
        <span>${m.role === "assistant" && m.latencyMs ? `${m.latencyMs} ms` : ""}</span>
        <button type="button" class="copy-msg">Copy</button>
        ${m.role === "assistant" && idx === conv.messages.length - 1 ? '<button type="button" class="regen-msg">Regenerate</button>' : ""}
      </div>`;
    div.querySelector(".copy-msg").addEventListener("click", () => navigator.clipboard?.writeText(m.content));
    div.querySelectorAll(".copy-code").forEach(btn => btn.addEventListener("click", () => {
      const code = btn.closest(".code-wrap").querySelector("code").textContent;
      navigator.clipboard?.writeText(code);
      btn.textContent = "Copied";
      setTimeout(() => { btn.textContent = "Copy"; }, 1200);
    }));
    const regen = div.querySelector(".regen-msg");
    if (regen) regen.addEventListener("click", () => {
      const prevUser = [...conv.messages].reverse().find(x => x.role === "user");
      if (prevUser) { conv.messages.pop(); sendMessage(prevUser.content, true); }
    });
    el.appendChild(div);
  });
  el.parentElement.scrollTop = el.parentElement.scrollHeight;
}

async function sendMessage(text, isRegen = false) {
  if (sending || !text.trim()) return;
  const conv = activeConv() || createConv();
  if (!isRegen) {
    if (!conv.messages.length) conv.title = text.trim().slice(0, 48);
    conv.messages.push({ role: "user", content: text.trim() });
  }
  sending = true;
  $("send-btn").disabled = true;
  $("prompt").value = "";
  save(); renderSidebar(); renderStream();

  const t0 = performance.now();
  const toolName = $("tool-select").value;
  try {
    let toolResult = null;
    if (toolName) {
      const args = toolName === "glob_files" ? { pattern: "crates/fw-fx/**/*.rs" } : { action: "probe" };
      const mcpRes = await fetch("/mcp", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ jsonrpc: "2.0", id: Date.now(), method: "tools/call", params: { name: toolName, arguments: args } })
      }).then(r => r.json());
      toolResult = mcpRes.result || mcpRes.error || mcpRes;
    }
    const payload = {
      model: $("model-select").value,
      temperature: Number(cfg.temperature),
      messages: [{ role: "system", content: cfg.systemPrompt }, ...conv.messages.map(m => ({ role: m.role, content: m.content }))],
      ...(toolName ? { tool_choice: toolName } : {})
    };
    const res = await fetch(cfg.endpoint, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(payload)
    }).then(r => r.json());
    const content = res.choices?.[0]?.message?.content || JSON.stringify(res, null, 2);
    conv.messages.push({
      role: "assistant",
      content,
      toolName,
      toolResult: toolResult || res.kernelToolCall || null,
      latencyMs: Math.round(performance.now() - t0)
    });
  } catch (err) {
    conv.messages.push({ role: "assistant", content: "Request failed: " + String(err), latencyMs: Math.round(performance.now() - t0) });
  } finally {
    sending = false;
    $("send-btn").disabled = false;
    save(); renderStream();
  }
}

async function boot() {
  try {
    const [health, models, mcp] = await Promise.all([
      fetch("/health").then(r => r.json()),
      fetch("/v1/models").then(r => r.json()),
      fetch("/mcp", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "tools/list" }) }).then(r => r.json())
    ]);
    $("srv-dot").classList.add(health.ok ? "ok" : "err");
    $("srv-status").textContent = health.ok ? `v${health.version}` : "Offline";
    if (models.data?.length) {
      $("model-select").innerHTML = models.data.map(m => `<option value="${esc(m.id)}">${esc(m.id)}</option>`).join("");
    }
    const tools = mcp.result?.tools || [];
    $("mcp-badge").textContent = `MCP: ${tools.length} tools`;
    for (const t of tools) {
      const opt = document.createElement("option");
      opt.value = t.name; opt.textContent = t.name;
      $("tool-select").appendChild(opt);
    }
  } catch {
    $("srv-dot").classList.add("err");
    $("srv-status").textContent = "Disconnected";
  }
  if (!navigator.gpu) $("mcp-badge").title = "WebGPU unavailable — CPU fallback active";
}

$("new-chat").addEventListener("click", createConv);
$("conv-search").addEventListener("input", renderSidebar);
$("toggle-sb").addEventListener("click", () => document.body.classList.toggle("sidebar-collapsed"));
$("theme-btn").addEventListener("click", () => {
  cfg.theme = document.documentElement.dataset.theme === "light" ? "dark" : "light";
  document.documentElement.dataset.theme = cfg.theme; save();
});
$("settings-btn").addEventListener("click", () => {
  $("sys-prompt").value = cfg.systemPrompt;
  $("api-endpoint").value = cfg.endpoint;
  $("temp-input").value = cfg.temperature;
  $("temp-val").textContent = cfg.temperature;
  $("settings-dlg").showModal();
});
$("temp-input").addEventListener("input", e => { $("temp-val").textContent = e.target.value; });
$("close-dlg").addEventListener("click", () => {
  cfg.systemPrompt = $("sys-prompt").value;
  cfg.endpoint = $("api-endpoint").value;
  cfg.temperature = Number($("temp-input").value);
  save(); $("settings-dlg").close();
});
$("clear-all").addEventListener("click", () => {
  convs = []; activeId = null; save(); renderSidebar(); renderStream(); $("settings-dlg").close();
});
$("composer").addEventListener("submit", e => { e.preventDefault(); sendMessage($("prompt").value); });
$("prompt").addEventListener("keydown", e => {
  if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); sendMessage($("prompt").value); }
});

renderSidebar();
renderStream();
boot();
