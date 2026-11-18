import type { PortalSnapshot, StudioEntry } from "./catalog.ts";

export function escHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

interface LayoutOpts {
  title: string;
  description: string;
  path: string;
  origin: string;
  body: string;
}

const NAV_ITEMS = [
  { href: "/", label: "Portal" },
  { href: "/webos", label: "WebOS" },
  { href: "/search", label: "Search" },
  { href: "/wasm", label: "WASM & ABI" },
  { href: "/dash/", label: "Chromium Dash" },
  { href: "/docs", label: "Docs" },
] as const;

export function renderShell(opts: LayoutOpts): string {
  const canonical = `${opts.origin.replace(/\/+$/, "")}${opts.path}`;
  const navLinks = NAV_ITEMS.map((item) => {
    const active = item.href === opts.path || (item.href !== "/" && opts.path.startsWith(item.href));
    return `<a href="${item.href}"${active ? ' aria-current="page" class="active"' : ""}>${escHtml(item.label)}</a>`;
  }).join("");

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>${escHtml(opts.title)}</title>
  <meta name="description" content="${escHtml(opts.description)}">
  <meta property="og:title" content="${escHtml(opts.title)}">
  <meta property="og:description" content="${escHtml(opts.description)}">
  <meta property="og:type" content="website">
  <meta property="og:url" content="${escHtml(canonical)}">
  <link rel="canonical" href="${escHtml(canonical)}">
  <link rel="icon" href="/favicon.ico" sizes="32x32">
  <link rel="icon" href="/assets/icon.svg" type="image/svg+xml">
  <link rel="apple-touch-icon" href="/apple-touch-icon.png">
  <link rel="manifest" href="/manifest.webmanifest">
  <link rel="stylesheet" href="/assets/m3-tokens.css">
  <style>
    :root {
      color-scheme: light dark;
      --surface: var(--md-sys-color-background, #fef9eb);
      --on-surface: var(--md-sys-color-on-surface, #1d1c14);
      --muted: var(--md-sys-color-on-surface-variant, #49473a);
      --card: var(--md-sys-color-surface-container, #f3eee0);
      --card-hi: var(--md-sys-color-surface-container-high, #ede8da);
      --primary: var(--md-sys-color-primary, #676013);
      --on-primary: var(--md-sys-color-on-primary, #ffffff);
      --primary-container: var(--md-sys-color-primary-container, #efe58b);
      --on-primary-container: var(--md-sys-color-on-primary-container, #4e4800);
      --tertiary: var(--md-sys-color-tertiary, #406653);
      --outline: var(--md-sys-color-outline-variant, #cbc7b5);
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      font-family: "Google Sans Flex", system-ui, -apple-system, sans-serif;
      background: var(--surface);
      color: var(--on-surface);
      line-height: 1.5;
    }
    .skip-link {
      position: absolute;
      left: -9999px;
      padding: .5rem 1rem;
      background: var(--primary);
      color: var(--on-primary);
    }
    .skip-link:focus { left: 1rem; top: 1rem; z-index: 100; }
    header.top {
      border-bottom: 1px solid var(--outline);
      background: var(--card);
      padding: .85rem 1.5rem;
      display: flex;
      flex-wrap: wrap;
      gap: 1rem;
      align-items: center;
      justify-content: space-between;
    }
    .brand {
      display: flex;
      align-items: center;
      gap: .65rem;
      text-decoration: none;
      color: var(--on-surface);
      font-weight: 700;
      font-size: 1.1rem;
    }
    .brand img { width: 28px; height: 28px; border-radius: 8px; }
    nav.primary { display: flex; flex-wrap: wrap; gap: .4rem; }
    nav.primary a {
      text-decoration: none;
      color: var(--muted);
      padding: .4rem .8rem;
      border-radius: 999px;
      font-size: .92rem;
      font-weight: 500;
    }
    nav.primary a.active, nav.primary a:hover {
      background: var(--primary-container);
      color: var(--on-primary-container);
    }
    nav.primary a:focus-visible, button:focus-visible, input:focus-visible {
      outline: 2px solid var(--primary);
      outline-offset: 2px;
    }
    main { max-width: 1180px; margin: 0 auto; padding: 1.75rem 1.5rem 3rem; }
    .hero {
      background: var(--card);
      border: 1px solid var(--outline);
      border-radius: 20px;
      padding: 1.5rem 1.75rem;
      margin-bottom: 1.5rem;
    }
    .hero h1 { margin: 0 0 .5rem; font-size: clamp(1.5rem, 3vw, 2.25rem); }
    .hero p { margin: 0; color: var(--muted); max-width: 70ch; }
    .pill-row { display: flex; flex-wrap: wrap; gap: .5rem; margin-top: 1rem; }
    .pill {
      display: inline-block;
      padding: .25rem .7rem;
      border-radius: 999px;
      font-size: .82rem;
      background: var(--card-hi);
      border: 1px solid var(--outline);
      color: var(--on-surface);
    }
    .grid {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
      gap: 1rem;
      margin-top: 1rem;
    }
    .card {
      background: var(--card);
      border: 1px solid var(--outline);
      border-radius: 16px;
      padding: 1rem 1.15rem;
    }
    .card h2, .card h3 { margin: 0 0 .4rem; font-size: 1.05rem; }
    .card p { margin: .25rem 0; color: var(--muted); font-size: .92rem; }
    code, pre {
      font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
      background: var(--card-hi);
      border-radius: 8px;
    }
    code { padding: .1rem .35rem; font-size: .88em; }
    pre { padding: .85rem; overflow-x: auto; font-size: .85rem; border: 1px solid var(--outline); }
    form.bar { display: flex; gap: .5rem; margin: 1rem 0; }
    form.bar input {
      flex: 1;
      padding: .65rem .9rem;
      border-radius: 999px;
      border: 1px solid var(--outline);
      background: var(--surface);
      color: var(--on-surface);
      font-size: 1rem;
    }
    form.bar button, .btn {
      padding: .65rem 1.15rem;
      border-radius: 999px;
      border: none;
      background: var(--primary);
      color: var(--on-primary);
      font-weight: 600;
      cursor: pointer;
      text-decoration: none;
      font-size: .92rem;
    }
    footer.foot {
      max-width: 1180px;
      margin: 0 auto;
      padding: 1rem 1.5rem 2rem;
      color: var(--muted);
      font-size: .85rem;
      border-top: 1px solid var(--outline);
      display: flex;
      flex-wrap: wrap;
      justify-content: space-between;
      gap: .75rem;
    }
  </style>
</head>
<body>
  <a class="skip-link" href="#main">Skip to content</a>
  <header class="top">
    <a class="brand" href="/">
      <img src="/assets/icon.svg" alt="Aphrody mark">
      <span>Aphrody OS &amp; SDK</span>
    </a>
    <nav class="primary" aria-label="Primary">${navLinks}</nav>
  </header>
  <main id="main">${opts.body}</main>
  <footer class="foot">
    <span>Unified Platform — CLI (<code>ay</code>) · Website · WebOS · Corelib WASM · Native GUI</span>
    <span><a href="/api/status">API Status</a> · <a href="/api/abi">C23 ABI</a> · <a href="/api/quota">Cloud Quota</a> · <a href="/llms.txt">llms.txt</a></span>
  </footer>
</body>
</html>`;
}

export function renderPortalHome(snapshot: PortalSnapshot, origin: string): string {
  const productCards = snapshot.products
    .map(
      (p) => `<article class="card">
        <h3>${escHtml(p.title)}</h3>
        <p>Key: <code>${escHtml(p.key)}</code> · Owner: <code>${escHtml(p.owner)}</code></p>
        <p>${p.keywords.slice(0, 6).map(escHtml).join(" · ")}</p>
      </article>`,
    )
    .join("");

  return renderShell({
    title: "Aphrody — Unified OS, SDK, WebOS, WASM & Cloud Platform",
    description: "One unified platform spanning CLI (ay), WebOS creative shell, Search, Corelib WASM & C23 ABI, Chromium Dash, and Cloudflare/VPS deployment.",
    path: "/",
    origin,
    body: `
      <section class="hero">
        <h1>Aphrody Unified Platform</h1>
        <p>Converged architecture uniting OS, SDK (<code>ay</code>), WebOS Creative Shell, AI OS, Reverse Engineering, Corelib WASM, Chromium Dash, and Material 3 Neural Expressive UI with zero legacy duplication.</p>
        <div class="pill-row">
          <span class="pill">${snapshot.products.length} Products</span>
          <span class="pill">${snapshot.repos.length} Workspace Repos</span>
          <span class="pill">${snapshot.featureStats.total} Matrix Features (${snapshot.featureStats.done} Done)</span>
          <span class="pill">C ABI v${snapshot.abi.version} (${snapshot.abi.constantsCount} constants)</span>
          <span class="pill">${snapshot.studios.length} WebOS Studios</span>
        </div>
      </section>
      <section class="grid">
        <article class="card">
          <h2>Unified CLI (<code>ay</code>)</h2>
          <p>Run <code>ay web</code>, <code>ay ai</code>, <code>ay shell</code>, <code>ay re</code>, <code>ay wasm</code>, <code>ay quota</code>, or <code>ay doctor</code> from one Bun-native entrypoint.</p>
        </article>
        <article class="card">
          <h2>WebOS &amp; Virtual Shell</h2>
          <p>Launch the interactive <a href="/webos">WebOS Desktop</a> with Creative Studios, R2 buckets, and sandboxed <code>nysh</code> pipeline execution.</p>
        </article>
        <article class="card">
          <h2>WASM Corelib &amp; C23 ABI</h2>
          <p>Inspect and execute freestanding C23 WebAssembly exports in the <a href="/wasm">WASM Sandbox</a>.</p>
        </article>
        <article class="card">
          <h2>Chromium &amp; Search</h2>
          <p>Query Chromium channels via <a href="/dash/">Chromium Dash</a> or search platform specs via <a href="/search">Aphrody Search</a>.</p>
        </article>
      </section>
      <h2 style="margin-top:2rem">Platform Products</h2>
      <section class="grid">${productCards}</section>
    `,
  });
}

export function renderWebOsPage(snapshot: PortalSnapshot, origin: string, activeStudio?: StudioEntry): string {
  const studioCards = snapshot.studios
    .map(
      (s) => `<article class="card">
        <h3>${escHtml(s.title)}</h3>
        <p>${escHtml(s.summary)}</p>
        <p><code>${escHtml(s.host)}</code> · <a href="/webos?studio=${encodeURIComponent(s.id)}">Open</a></p>
      </article>`,
    )
    .join("");

  const activeBanner = activeStudio
    ? `<p class="pill">Active Studio: <strong>${escHtml(activeStudio.title)}</strong> (${escHtml(activeStudio.host)})</p>`
    : "";

  return renderShell({
    title: activeStudio ? `${activeStudio.title} — Aphrody WebOS` : "Aphrody WebOS — Unified Desktop & Creative Studios",
    description: "Browser-native WebOS launcher uniting Aphrody creative studios, R2 storage mounts, fleet telemetry, and sandboxed nysh shell execution.",
    path: "/webos",
    origin,
    body: `
      <section class="hero">
        <h1>Aphrody WebOS Desktop</h1>
        <p>Unified browser workspace combining GPUI/Dioxus design tokens, 14 creative suites, edge R2 mounts, and sandboxed <code>nysh</code> execution.</p>
        ${activeBanner}
        <form class="bar" id="nysh-form">
          <input id="nysh-cmd" name="cmd" value="cat /ay/manifest.json | grep aphrody" aria-label="Virtual nysh command">
          <button type="submit">Run nysh</button>
        </form>
        <pre id="nysh-out">Ready. Run a sandboxed command (cat, echo, grep, wc -l) via POST /api/nysh.</pre>
      </section>
      <script>
        const form = document.getElementById("nysh-form");
        const out = document.getElementById("nysh-out");
        form?.addEventListener("submit", async (e) => {
          e.preventDefault();
          const cmd = document.getElementById("nysh-cmd").value;
          const r = await fetch("/api/nysh", {
            method: "POST",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({ cmd })
          });
          out.textContent = JSON.stringify(await r.json(), null, 2);
        });
      </script>
      <h2>Creative Studios</h2>
      <section class="grid">${studioCards}</section>
    `,
  });
}

export function renderWasmPage(snapshot: PortalSnapshot, origin: string, wasmAvailable: boolean): string {
  const fnRows = snapshot.abi.functions
    .map((fn) => `<li><code>${escHtml(fn.name)}</code> -&gt; <code>${escHtml(fn.returns)}</code>${fn.doc ? ` — ${escHtml(fn.doc)}` : ""}</li>`)
    .join("");

  return renderShell({
    title: "Aphrody WASM & C23 ABI — Corelib Sandbox",
    description: "Freestanding C23 corelib compiled cleanly to WebAssembly (wasm32) and native targets from abi/ay.abi.toml.",
    path: "/wasm",
    origin,
    body: `
      <section class="hero">
        <h1>Corelib WebAssembly &amp; C23 ABI</h1>
        <p>Single C23 codebase running bare-metal x86_64, native FFI, and sandboxed WebAssembly modules.</p>
        <div class="pill-row">
          <span class="pill">ABI v${snapshot.abi.version}</span>
          <span class="pill">Spec SHA: <code>${escHtml(snapshot.abi.specSha256.slice(0, 12) || "dev")}</code></span>
          <span class="pill">WASM artifact: ${wasmAvailable ? "Built (/wasm/corelib.wasm)" : "Run ay wasm to build"}</span>
        </div>
        <div class="pill-row">
          <button class="btn" id="run-wasm" type="button">Execute Server /api/wasm/run</button>
        </div>
        <pre id="wasm-out">Click to inspect/execute corelib WASM exports.</pre>
      </section>
      <section class="card">
        <h2>Exported ABI Functions (${snapshot.abi.functions.length})</h2>
        <ul>${fnRows || "<li>No functions loaded.</li>"}</ul>
      </section>
      <script>
        document.getElementById("run-wasm")?.addEventListener("click", async () => {
          const r = await fetch("/api/wasm/run");
          document.getElementById("wasm-out").textContent = JSON.stringify(await r.json(), null, 2);
        });
      </script>
    `,
  });
}

export function renderSearchPage(
  snapshot: PortalSnapshot,
  origin: string,
  query: string,
  hits: ReadonlyArray<{ kind: string; id: string; title: string; detail: string }>,
): string {
  const items = hits
    .map(
      (h) => `<article class="card">
        <h3>${escHtml(h.title)}</h3>
        <p><code>${escHtml(h.kind)}</code> · <code>${escHtml(h.id)}</code></p>
        <p>${escHtml(h.detail)}</p>
      </article>`,
    )
    .join("");

  return renderShell({
    title: query ? `${query} — Aphrody Search` : "Aphrody Search — Unified Platform Index",
    description: "Fast search across Aphrody products, feature matrix proofs, ABI symbols, Creative Studios, and workspace repositories.",
    path: "/search",
    origin,
    body: `
      <section class="hero">
        <h1>Aphrody Search</h1>
        <p>Instant search across ${snapshot.products.length} products, ${snapshot.featureStats.total} feature rows, ${snapshot.studios.length} WebOS studios, and C ABI symbols.</p>
        <form class="bar" method="GET" action="/search">
          <input name="q" value="${escHtml(query)}" placeholder="Search features, studios, ABI, repos..." aria-label="Search query">
          <button type="submit">Search</button>
        </form>
      </section>
      <p>${hits.length} result(s)${query ? ` for <code>${escHtml(query)}</code>` : ""}.</p>
      <section class="grid">${items}</section>
    `,
  });
}

export function renderDocsPage(snapshot: PortalSnapshot, origin: string): string {
  return renderShell({
    title: "Aphrody Docs — OS, SDK, CLI & Craft Studio",
    description: "Technical documentation for Aphrody OS, SDK corelib, CLI commands, and Material 3 design tokens.",
    path: "/docs",
    origin,
    body: `
      <section class="hero">
        <h1>Aphrody Documentation</h1>
        <p>Single-source technical reference. Supports Markdown content negotiation via <code>Accept: text/markdown</code> or <a href="/llms.txt">llms.txt</a>.</p>
        <pre>ay web      # Launch unified Website, WebOS, Search, WASM, Chromium Dash
ay ai       # Run AI OS kernel (MCP, A2A, orchestrator, brain, fabric)
ay shell    # Launch or audit native shells (ay-sh, fish, nu, bash)
ay re       # Reverse engineering &amp; Ghidra analysis
ay wasm     # Run Corelib WebAssembly component conformance tests
ay quota    # Audit Cloudflare + GitHub Actions + VPS fleet routing
ay doctor   # Inspect fast modern CLI toolchain (eza, dust, xh, fd, rg, bat)</pre>
      </section>
      <section class="grid">
        <article class="card">
          <h2>Supported Languages</h2>
          <p>${snapshot.sdk.languages.map(escHtml).join(", ")}</p>
        </article>
        <article class="card">
          <h2>Architectures &amp; GPUs</h2>
          <p>${snapshot.sdk.arch.map(escHtml).join(", ")} · ${snapshot.sdk.gpu.map(escHtml).join(", ")}</p>
        </article>
      </section>
    `,
  });
}

export function renderDocsMarkdown(snapshot: PortalSnapshot): string {
  return [
    "# Aphrody Documentation",
    "",
    `- Unified CLI: \`${snapshot.product.cli}\``,
    `- Products: ${snapshot.products.map((p) => p.key).join(", ")}`,
    `- Languages: ${snapshot.sdk.languages.join(", ")}`,
    `- Architectures: ${snapshot.sdk.arch.join(", ")}`,
    `- Features tracked: ${snapshot.featureStats.total} (${snapshot.featureStats.done} done)`,
    "",
    "## Endpoints",
    "- `/` Portal",
    "- `/webos` Creative WebOS & Virtual `nysh` shell (`POST /api/nysh`)",
    "- `/search` Search UI (`GET /api/search?q=...`)",
    "- `/wasm` Corelib WebAssembly Sandbox (`GET /api/wasm/run`, `GET /api/abi`)",
    "- `/dash/` Chromium Dash & Chrome Status",
    "- `/api/status`, `/api/orgs`, `/api/fleet`, `/api/registries`, `/api/quota`, `/healthz`",
    "",
  ].join("\n");
}
