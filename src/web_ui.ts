export function renderFairwindWebHtml(): string {
  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Fairwind Unified OS Control Plane</title>
  <style>
    :root {
      --bg: #0b1220;
      --surface: #111a2e;
      --border: #24324d;
      --text: #f1f5f9;
      --muted: #94a3b8;
      --accent: #38bdf8;
      --ok: #22c55e;
      --font: ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, sans-serif;
      --mono: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    }
    * { box-sizing: border-box; }
    body { margin: 0; font-family: var(--font); background: var(--bg); color: var(--text); line-height: 1.5; }
    header { padding: 1.25rem 1.5rem; border-bottom: 1px solid var(--border); display: flex; justify-content: space-between; align-items: center; }
    h1 { margin: 0; font-size: 1.2rem; letter-spacing: 0.02em; }
    main { max-width: 1120px; margin: 0 auto; padding: 1.5rem; display: grid; gap: 1rem; grid-template-columns: repeat(auto-fit, minmax(300px, 1fr)); }
    section { background: var(--surface); border: 1px solid var(--border); border-radius: 10px; padding: 1rem 1.25rem; }
    h2 { margin: 0 0 0.75rem; font-size: 0.95rem; color: var(--accent); text-transform: uppercase; letter-spacing: 0.06em; }
    pre { margin: 0; font-family: var(--mono); font-size: 0.8rem; color: #e2e8f0; overflow: auto; max-height: 280px; white-space: pre-wrap; }
    .badge { font-family: var(--mono); font-size: 0.75rem; padding: 0.2rem 0.55rem; border-radius: 999px; background: #0f291e; color: var(--ok); border: 1px solid #166534; }
    a { color: var(--accent); }
  </style>
</head>
<body>
  <header>
    <h1>Fairwind Unified OS — Fat-LTO / AOT / VFS / AI Control Plane</h1>
    <span class="badge" id="health">loading...</span>
  </header>
  <main>
    <section><h2>System &amp; Fat-LTO ABI</h2><pre id="status">Loading /status...</pre></section>
    <section><h2>AI Router &amp; OpenAI Models</h2><pre id="models">Loading /v1/models...</pre></section>
    <section><h2>VFS OS Mounts</h2><pre id="vfs">Loading /vfs/ls...</pre></section>
    <section><h2>Protocols &amp; Specs</h2>
      <ul>
        <li><a href="/openapi.json">OpenAPI 3.1 Spec (/openapi.json)</a></li>
        <li><a href="/.well-known/agent.json">A2A AgentCard (/.well-known/agent.json)</a></li>
        <li><a href="/v1/models">OpenAI Models (/v1/models)</a></li>
        <li><a href="/lowlevel/matrix">Low-Level LTO/AOT Matrix (/lowlevel/matrix)</a></li>
        <li><a href="/soul">AI Soul (/soul)</a></li>
      </ul>
    </section>
  </main>
  <script>
    async function load(id, url) {
      const r = await fetch(url);
      const j = await r.json();
      document.getElementById(id).textContent = JSON.stringify(j, null, 2);
      return j;
    }
    Promise.all([
      load("status", "/status"),
      load("models", "/v1/models"),
      load("vfs", "/vfs/ls"),
    ]).then(([st]) => {
      document.getElementById("health").textContent = st.ok ? "ONLINE | ABI v" + (st.abi?.version ?? "?") : "DEGRADED";
    });
  </script>
</body>
</html>`;
}
