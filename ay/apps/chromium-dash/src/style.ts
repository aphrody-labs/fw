/** App stylesheet. Colors, type and shape come from the brand M3 tokens (`brand/tokens/m3-tokens.css`, served at /assets/m3-tokens.css). */
export const FONT_CSS = "https://fonts.googleapis.com/css2?family=Google+Sans+Flex:wght@300..700&display=swap";

export const APP_CSS = /* css */ `
*{box-sizing:border-box}
html{color-scheme:light dark}
body{margin:0;font:400 15px/1.5 "Google Sans Flex",system-ui,sans-serif;background:var(--md-sys-color-surface);color:var(--md-sys-color-on-surface)}
a{color:var(--md-sys-color-primary);text-decoration:none}
a:hover{text-decoration:underline}
.bar{position:sticky;top:0;z-index:2;display:flex;align-items:center;gap:16px;height:64px;padding:0 24px;background:var(--md-sys-color-primary-container);color:var(--md-sys-color-on-primary-container)}
.bar a{color:inherit}
.brand{display:flex;align-items:center;gap:12px;font-size:22px;font-weight:500}
.brand img{width:32px;height:32px}
.bar h1{margin:0;font-size:22px;font-weight:500}
.bar .sub{opacity:.8;font-size:18px}
.bar .grow{flex:1}
.shell{display:grid;grid-template-columns:220px minmax(0,1fr);min-height:calc(100vh - 64px)}
nav.drawer{padding:12px 12px 24px;background:var(--md-sys-color-surface-container-low);border-right:1px solid var(--md-sys-color-outline-variant)}
nav.drawer h2{margin:20px 12px 6px;font-size:12px;font-weight:500;letter-spacing:.08em;text-transform:uppercase;color:var(--md-sys-color-on-surface-variant)}
nav.drawer a{display:block;padding:8px 16px;border-radius:999px;color:var(--md-sys-color-on-surface)}
nav.drawer a:hover{background:var(--md-sys-color-surface-container-high);text-decoration:none}
nav.drawer a[aria-current=page]{background:var(--md-sys-color-secondary-container);color:var(--md-sys-color-on-secondary-container);font-weight:500}
main{padding:24px 32px 48px;max-width:1280px}
main h1{font-size:28px;font-weight:500;margin:0 0 4px}
main h2{font-size:20px;font-weight:500;margin:32px 0 8px}
.lead{color:var(--md-sys-color-on-surface-variant);margin:0 0 16px;max-width:70ch}
.cards{display:grid;grid-template-columns:repeat(auto-fill,minmax(340px,1fr));gap:16px}
.card{background:var(--md-sys-color-surface-container);border-radius:16px;overflow:hidden}
.card>header{display:flex;justify-content:space-between;align-items:center;padding:12px 16px;font-weight:500;background:var(--md-sys-color-primary);color:var(--md-sys-color-on-primary)}
.card.stable>header{background:var(--md-sys-color-tertiary);color:var(--md-sys-color-on-tertiary)}
.card.beta>header{background:var(--md-sys-color-secondary);color:var(--md-sys-color-on-secondary)}
.card.extended>header{background:var(--md-sys-color-on-tertiary-container);color:var(--md-sys-color-tertiary-container)}
.card.canary>header,.card.canary_asan>header{background:var(--md-sys-color-error);color:var(--md-sys-color-on-error)}
.card .row{display:flex;align-items:center;gap:12px;padding:10px 16px;border-top:1px solid var(--md-sys-color-outline-variant)}
.card .row .grow{flex:1;min-width:0}
.card .row .meta{color:var(--md-sys-color-on-surface-variant);font-size:13px;text-align:right}
.card .more{display:block;padding:10px 16px;text-align:center;border-top:1px solid var(--md-sys-color-outline-variant)}
.chip{display:inline-block;min-width:36px;padding:1px 8px;border-radius:8px;text-align:center;font-weight:600;font-size:13px;font-variant-numeric:tabular-nums}
.m0{background:var(--md-sys-color-primary-container);color:var(--md-sys-color-on-primary-container)}
.m1{background:var(--md-sys-color-secondary-container);color:var(--md-sys-color-on-secondary-container)}
.m2{background:var(--md-sys-color-tertiary-container);color:var(--md-sys-color-on-tertiary-container)}
.m3{background:var(--md-sys-color-error-container);color:var(--md-sys-color-on-error-container)}
table{width:100%;border-collapse:collapse;background:var(--md-sys-color-surface-container);border-radius:16px;overflow:hidden}
th,td{padding:10px 16px;text-align:left;border-bottom:1px solid var(--md-sys-color-outline-variant);vertical-align:top}
th{font-weight:500;font-size:13px;background:var(--md-sys-color-surface-container-high);color:var(--md-sys-color-on-surface-variant)}
tr:last-child td{border-bottom:0}
code,.mono{font-family:ui-monospace,Consolas,monospace;font-size:13px}
.tabs{display:flex;gap:8px;flex-wrap:wrap;margin:8px 0 20px}
.tabs a{padding:6px 16px;border-radius:999px;border:1px solid var(--md-sys-color-outline);color:var(--md-sys-color-on-surface)}
.tabs a[aria-current=page]{background:var(--md-sys-color-secondary-container);border-color:transparent;font-weight:500}
.pager{display:flex;gap:16px;margin:12px 0}
form.search{display:flex;gap:8px;margin:8px 0 20px}
input[type=search],input[type=text]{flex:1;max-width:560px;padding:10px 16px;border-radius:999px;border:1px solid var(--md-sys-color-outline);background:var(--md-sys-color-surface);color:inherit;font:inherit}
button{padding:10px 20px;border:0;border-radius:999px;background:var(--md-sys-color-primary);color:var(--md-sys-color-on-primary);font:inherit;font-weight:500;cursor:pointer}
.sched{display:grid;grid-template-columns:repeat(auto-fill,minmax(300px,1fr));gap:16px}
.sched .card>header{justify-content:flex-start;font-size:20px}
.sched .row{justify-content:space-between}
.sched .row.past{color:var(--md-sys-color-on-surface-variant);font-style:italic}
.sched .group{padding:10px 16px;font-weight:500;border-top:1px solid var(--md-sys-color-outline-variant)}
.feat{display:flex;gap:12px;padding:8px 0;border-bottom:1px solid var(--md-sys-color-outline-variant)}
.feat .name{font-weight:500}
.feat .summary{color:var(--md-sys-color-on-surface-variant);font-size:14px}
.pill{display:inline-block;padding:0 10px;border-radius:999px;font-size:12px;background:var(--md-sys-color-surface-container-high);color:var(--md-sys-color-on-surface-variant)}
.kv{display:grid;grid-template-columns:200px 1fr;gap:6px 16px}
.kv dt{color:var(--md-sys-color-on-surface-variant)}
.kv dd{margin:0}
.panel{background:var(--md-sys-color-surface-container);border-radius:16px;padding:8px 20px 16px;margin:16px 0}
.panel h2{margin-top:12px}
.road{display:grid;grid-template-columns:repeat(auto-fit,minmax(360px,1fr));gap:16px;align-items:start}
.road .card{padding:0 0 12px}
.road .head{padding:20px 20px 8px;text-align:center}
.road .head .tag{font-weight:600;letter-spacing:.08em;font-size:14px;text-transform:uppercase}
.road .head .big{font-size:40px;font-weight:300;margin:4px 0}
.road h3{margin:12px 20px 4px;font-size:15px;font-weight:500;color:var(--md-sys-color-on-surface-variant)}
.road ul{margin:0 20px;padding:0;list-style:none}
.road li{padding:4px 0}
.muted{color:var(--md-sys-color-on-surface-variant)}
.stale{margin:0 0 16px;padding:8px 16px;border-radius:12px;background:var(--md-sys-color-error-container);color:var(--md-sys-color-on-error-container)}
footer{padding:16px 32px;color:var(--md-sys-color-on-surface-variant);font-size:13px}
@media(max-width:800px){.shell{grid-template-columns:1fr}nav.drawer{display:flex;flex-wrap:wrap;gap:4px;border-right:0}nav.drawer h2{display:none}main{padding:16px}.kv{grid-template-columns:1fr}}
`;
