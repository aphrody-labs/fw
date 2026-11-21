// Progressive enhancement only: every action also works as a plain HTML form.
// Drag files onto a folder's file list to upload them there.
for (const zone of document.querySelectorAll("[data-drop]")) {
  const target = zone.dataset.drop;
  const set = (on) => zone.classList.toggle("over", on);
  zone.addEventListener("dragover", (e) => { e.preventDefault(); set(true); });
  zone.addEventListener("dragleave", () => set(false));
  zone.addEventListener("drop", async (e) => {
    e.preventDefault();
    set(false);
    if (!e.dataTransfer?.files?.length) return;
    const body = new FormData();
    for (const f of e.dataTransfer.files) body.append("file", f, f.name);
    zone.setAttribute("aria-busy", "true");
    const res = await fetch(target, { method: "POST", body });
    location.href = res.redirected ? res.url : location.href;
  });
}
// Close an open menu when another opens or on Escape.
document.addEventListener("toggle", (e) => {
  if (e.target instanceof HTMLDetailsElement && e.target.open) {
    for (const d of document.querySelectorAll("details[open]")) if (d !== e.target && !d.contains(e.target)) d.open = false;
  }
}, true);
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") for (const d of document.querySelectorAll("details[open]")) { d.open = false; d.querySelector("summary")?.focus(); }
});
