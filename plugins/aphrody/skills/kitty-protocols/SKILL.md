---
name: kitty-protocols
description: "Kitty terminal protocols (aphrody-labs/kitty synced with kovidgoyal/kitty): Graphics Protocol (APC), Keyboard Protocol (CSI u), Shell Integration (OSC 133), Remote Control, OSC 8 hyperlinks"
---
> , OSC 52 clipboard, OSC 99 notifications, and DEC 2026 synchronized output.


# Kitty Terminal Protocols (`aphrody-labs/kitty`) Skill

Verified live against `kovidgoyal/kitty` (`master`, synced into `aphrody-labs/kitty` `2026-11-19`).

## 1. Core Protocol Specifications

| Protocol | Escape Sequence | Purpose |
| --- | --- | --- |
| **Graphics Protocol (APC)** | `\x1b_Ga=T,f=32,s=<w>,v=<h>;<b64>\x1b\\` | Inline pixel-accurate RGBA/RGB/PNG images composited above or below terminal text (`z` index, chunked `m=1/0`, placement IDs `i=`). |
| **Keyboard Protocol (`CSI u`)** | Enable: `\x1b[><flags>u`, Pop: `\x1b[<u` | Unambiguous key events with flags bitmask: `1` disambiguate escapes, `2` report event types (press/repeat/release), `4` alternate keys, `8` all keys as escapes, `16` associated text. |
| **Shell Integration (`OSC 133`)** | `\x1b]133;A\x07` (prompt), `B` (input), `C` (exec), `D;<code>` (exit) | Semantic prompt jumping, last-command output capture, and automatic failure detection (autofix). |
| **Synchronized Output (`DEC 2026`)** | Begin: `\x1b[?2026h`, End: `\x1b[?2026l` | Tear-free atomic frame updates for high-FPS TUIs and coding agents. |
| **Hyperlinks (`OSC 8`)** | `\x1b]8;;<url>\x07<label>\x1b]8;;\x07` | Clickable file/URL links inside terminal output. |
| **Clipboard (`OSC 52`)** | `\x1b]52;c;<base64>\x07` | Cross-SSH/tmux/container clipboard read/write. |
| **Desktop Notifications (`OSC 99`)** | `\x1b]99;i=<id>:d=0;<title>\x1b\\` | Rich terminal notifications when long-running agent goals complete. |
| **Remote Control** | `kitten @ <ls|launch|send-text|set-tab-title|close-window>` | Scriptable terminal control (`allow_remote_control=yes` / `remote_control_password`). |

## 2. Unified Pure-Rust WGPU / Wasm Integration in `fw`

- `crates/fw-wayland-wgpu/src/unified_wgpu_terminal.rs`: Pure-Rust (`#![forbid(unsafe_code)]`) encoder/decoder for Kitty Graphics APC, Kitty Keyboard `CSI u`, `DEC 2026`, `OSC 8`, `OSC 133`, and `OSC 7501` merged with Rio's pull-based dirty-row grid and Material 3 `1.5.0` Expressive WGPU rendering.
- `crates/fw-wayland-wgpu/shaders/fw_unified_terminal_m3.wgsl`: Validated (`naga` + `tint`) pure WGSL shader rendering terminal cells, inline Kitty RGBA placements, and M3 overlays in one draw pass.
