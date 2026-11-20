// Settle, then extract one page into the raw shape `capture::Capture::from_raw` post-processes (de-duplication,
// credit-line filter, turn ordering). Called as `(<this file>)(quietMs, maxMs)`; the result is a promise.
async (quietMs, maxMs) => {
  const started = performance.now();
  let settledBy = "timeout";
  await new Promise((resolve) => {
    let last = performance.now();
    const bump = () => {
      last = performance.now();
    };
    const mutations = new MutationObserver(bump);
    mutations.observe(document, { subtree: true, childList: true, characterData: true });
    let resources = null;
    try {
      resources = new PerformanceObserver(bump);
      resources.observe({ type: "resource" });
    } catch {}
    const finish = (reason) => {
      settledBy = reason;
      mutations.disconnect();
      if (resources) resources.disconnect();
      resolve();
    };
    const tick = () => {
      const now = performance.now();
      if (document.readyState === "complete" && now - last >= quietMs) finish("stable");
      else if (now - started >= maxMs) finish("timeout");
      else setTimeout(tick, 100);
    };
    tick();
  });

  const clean = (s) => (s || "").replace(/\s+/g, " ").trim();
  const HIDDEN = '.cdk-visually-hidden,.sr-only,script,style,noscript,mat-icon,button,[aria-hidden="true"]';
  const BLOCK = "p,li,div,h1,h2,h3,h4,h5,h6,pre,tr,blockquote,table,ul,ol";
  // Text with line breaks kept, screen-reader labels ("You said") and controls removed.
  const lines = (el) => {
    const copy = el.cloneNode(true);
    copy.querySelectorAll(HIDDEN).forEach((n) => n.remove());
    copy.querySelectorAll("br").forEach((n) => n.replaceWith("\n"));
    copy.querySelectorAll(BLOCK).forEach((n) => {
      n.prepend("\n");
      n.append("\n");
    });
    return (copy.textContent || "")
      .split("\n")
      .map((l) => l.replace(/[ \t ]+/g, " ").trim())
      .join("\n")
      .replace(/\n{3,}/g, "\n\n")
      .trim();
  };

  // One tree walk in document order through open shadow roots: turns carry their position.
  const turns = [];
  let pos = 0;
  const walk = (root) => {
    for (const el of root.children) {
      const tag = el.localName;
      if (tag === "user-query") {
        turns.push({ pos: pos++, role: "user", text: lines(el) });
        continue;
      }
      // The app renders `model-response`; public share pages render `response-container` in `share-turn-viewer`.
      if (tag === "model-response" || tag === "response-container") {
        const parts = [...el.querySelectorAll("message-content")];
        const text = parts.length ? parts.map(lines).join("\n\n") : lines(el);
        turns.push({ pos: pos++, role: "model", text });
        continue;
      }
      if (el.shadowRoot) walk(el.shadowRoot);
      walk(el);
    }
  };
  walk(document.documentElement);

  const main = document.querySelector("main, [role=main], article") || document.body;
  // Not article text: chrome, media fallbacks, and the contributor, related-card and feed blocks of article sites.
  const SKIP =
    'nav,footer,aside,video,audio,noscript,[aria-hidden="true"],[hidden],' +
    '[class*="contributor" i],[class*="related" i],[class*="feed" i],[class*="cookie" i],[class*="consent" i]';
  const blocks = [...main.querySelectorAll("h1,h2,h3,h4,p,li,figcaption,blockquote")]
    .filter((el) => !el.closest(SKIP))
    .map((el) => ({ tag: el.localName, text: clean(el.textContent) }));
  const links = [...document.querySelectorAll("a[href]")].slice(0, 1000).map((a) => ({
    href: a.href,
    text: clean(a.innerText || a.getAttribute("aria-label") || a.title),
  }));

  return {
    url: location.href,
    title: document.title,
    lang: document.documentElement.lang,
    settled_by: settledBy,
    settled_ms: Math.round(performance.now() - started),
    blocks,
    links,
    turns,
    body: document.body ? document.body.innerText : "",
  };
}
