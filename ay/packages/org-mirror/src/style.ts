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
.brand{display:flex;align-items:center;gap:12px;min-width:0;font-size:22px;font-weight:500}
.brand img{width:32px;height:32px;flex:none}
.brand span{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.bar .grow{flex:1}
.bar form{margin:0;flex:0 1 260px;min-width:0}
.bar input{width:100%}
.shell{display:grid;grid-template-columns:220px minmax(0,1fr);min-height:calc(100vh - 64px)}
nav.drawer{padding:12px 12px 24px;background:var(--md-sys-color-surface-container-low);border-right:1px solid var(--md-sys-color-outline-variant)}
nav.drawer h2{margin:20px 12px 6px;font-size:12px;font-weight:500;letter-spacing:.08em;text-transform:uppercase;color:var(--md-sys-color-on-surface-variant)}
nav.drawer a{display:block;padding:8px 16px;border-radius:999px;color:var(--md-sys-color-on-surface)}
nav.drawer a:hover{background:var(--md-sys-color-surface-container-high);text-decoration:none}
nav.drawer a[aria-current=page]{background:var(--md-sys-color-secondary-container);color:var(--md-sys-color-on-secondary-container);font-weight:500}
main{padding:24px 32px 48px;max-width:1280px;min-width:0}
main h1{font-size:28px;font-weight:500;margin:0 0 4px;overflow-wrap:anywhere}
main h2{font-size:20px;font-weight:500;margin:28px 0 8px}
.lead{color:var(--md-sys-color-on-surface-variant);margin:0 0 16px;max-width:70ch;overflow-wrap:anywhere}
.muted{color:var(--md-sys-color-on-surface-variant)}
code,.mono{font-family:ui-monospace,Consolas,monospace;font-size:13px}
.stats{display:grid;grid-template-columns:repeat(auto-fill,minmax(140px,1fr));gap:12px}
.stat{background:var(--md-sys-color-surface-container);border-radius:16px;padding:12px 16px}
.stat b{display:block;font-size:28px;font-weight:400;font-variant-numeric:tabular-nums}
.stat a,.stat span{font-size:13px;color:var(--md-sys-color-on-surface-variant)}
.chips{display:flex;flex-wrap:wrap;gap:6px;margin:8px 0}
.chip{display:inline-block;padding:0 10px;border-radius:999px;font-size:12px;line-height:22px;background:var(--md-sys-color-surface-container-high);color:var(--md-sys-color-on-surface-variant)}
a.chip:hover{text-decoration:none;background:var(--md-sys-color-surface-container-highest)}
.chip b{margin-left:4px;font-weight:600}
.chip.label{background:var(--md-sys-color-secondary-container);color:var(--md-sys-color-on-secondary-container)}
.chip.tag{background:var(--md-sys-color-tertiary-container);color:var(--md-sys-color-on-tertiary-container)}
.chip.open{background:var(--md-sys-color-tertiary-container);color:var(--md-sys-color-on-tertiary-container)}
.chip.closed{background:var(--md-sys-color-error-container);color:var(--md-sys-color-on-error-container)}
.chip.merged{background:var(--md-sys-color-primary-container);color:var(--md-sys-color-on-primary-container)}
.scroll{overflow-x:auto;border-radius:16px}
table{width:100%;border-collapse:collapse;background:var(--md-sys-color-surface-container)}
th,td{padding:10px 16px;text-align:left;border-bottom:1px solid var(--md-sys-color-outline-variant);vertical-align:top}
th{font-weight:500;font-size:13px;background:var(--md-sys-color-surface-container-high);color:var(--md-sys-color-on-surface-variant);white-space:nowrap}
tr:last-child td{border-bottom:0}
td.num,th.num{text-align:right;font-variant-numeric:tabular-nums}
.items{list-style:none;margin:0;padding:0;background:var(--md-sys-color-surface-container);border-radius:16px;overflow:hidden}
.item{padding:12px 16px;border-bottom:1px solid var(--md-sys-color-outline-variant)}
.item:last-child{border-bottom:0}
.item .title{font-weight:500;overflow-wrap:anywhere}
.item .meta{font-size:13px;color:var(--md-sys-color-on-surface-variant)}
.tabs{display:flex;gap:8px;flex-wrap:wrap;margin:8px 0 20px}
.tabs a{padding:6px 16px;border-radius:999px;border:1px solid var(--md-sys-color-outline);color:var(--md-sys-color-on-surface)}
.tabs a:hover{text-decoration:none;background:var(--md-sys-color-surface-container-high)}
.tabs a[aria-current=page]{background:var(--md-sys-color-secondary-container);border-color:transparent;font-weight:500}
.pager{display:flex;align-items:center;justify-content:space-between;gap:16px;margin:16px 0;flex-wrap:wrap}
form.search{display:flex;gap:8px;margin:8px 0 20px}
input[type=search]{flex:1;max-width:560px;min-width:0;padding:10px 16px;border-radius:999px;border:1px solid var(--md-sys-color-outline);background:var(--md-sys-color-surface);color:inherit;font:inherit}
button{padding:10px 20px;border:0;border-radius:999px;background:var(--md-sys-color-primary);color:var(--md-sys-color-on-primary);font:inherit;font-weight:500;cursor:pointer}
.kv{display:grid;grid-template-columns:200px minmax(0,1fr);gap:6px 16px;margin:8px 0}
.kv dt{color:var(--md-sys-color-on-surface-variant)}
.kv dd{margin:0;overflow-wrap:anywhere}
.langbar{display:flex;height:10px;border-radius:5px;overflow:hidden;background:var(--md-sys-color-surface-container-high)}
.legend{display:flex;flex-wrap:wrap;gap:4px 16px;list-style:none;margin:8px 0;padding:0}
.legend i{display:inline-block;width:10px;height:10px;border-radius:50%;margin-right:6px}
.l0{background:var(--md-sys-color-primary)}
.l1{background:var(--md-sys-color-secondary)}
.l2{background:var(--md-sys-color-tertiary)}
.l3{background:var(--md-sys-color-primary-fixed-dim)}
.l4{background:var(--md-sys-color-secondary-fixed-dim)}
.l5{background:var(--md-sys-color-tertiary-fixed-dim)}
.body{white-space:pre-wrap;overflow-wrap:anywhere;margin:8px 0;font:inherit}
.comment{background:var(--md-sys-color-surface-container);border-radius:16px;padding:12px 16px;margin:12px 0}
.comment header{display:flex;flex-wrap:wrap;gap:4px 12px;font-size:13px}
.delta{color:var(--md-sys-color-error)}
.ok{color:var(--md-sys-color-tertiary)}
.stale{margin:0 0 16px;padding:8px 16px;border-radius:12px;background:var(--md-sys-color-error-container);color:var(--md-sys-color-on-error-container)}
footer{padding:16px 32px;color:var(--md-sys-color-on-surface-variant);font-size:13px}
@media(max-width:800px){.shell{grid-template-columns:1fr}nav.drawer{display:flex;flex-wrap:wrap;gap:4px;border-right:0}nav.drawer h2{display:none}main{padding:16px}.kv{grid-template-columns:1fr}.bar{padding:0 16px}footer{padding:16px}}
@media(max-width:480px){.bar{gap:8px}.brand{font-size:18px}.bar form{flex-basis:140px}th,td{padding:8px 10px}}
`;
