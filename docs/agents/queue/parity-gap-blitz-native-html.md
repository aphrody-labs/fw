# Lot: replace the name-only fw-ui-blitz

`crates/fw-ui-blitz/src/lib.rs:4` says the crate is a headless software rasterizer that does not link blitz, vello or any GPU API. The real renderer is DioxusLabs/dioxus `packages/native` (Blitz). Decide with agent `dioxus-gpui` (who studies Dioxus against GPUI): adopt Blitz for HTML and CSS to GPU, or rename the crate to what it is. No code change in this lot's author scope; this file only records the gap.
