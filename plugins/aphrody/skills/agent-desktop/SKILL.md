---
name: agent-desktop
description: "Operate desktop apps on macOS, Windows and Linux through native accessibility trees with the agent-desktop CLI. Use when an agent must see or act on a desktop GUI."
---
# agent-desktop

`references/core.md` is the platform-neutral guide; the platform references add per-OS details.

Read the reference that matches the task in full before acting. Paths written inside reference files are relative to this skill's directory (the one holding this SKILL.md). Where a reference names another merged skill, open `references/<name>.md`; its helper files are under `references/<name>/`.

| Reference | Use when |
| --- | --- |
| [references/core.md](references/core.md) | Former body of this skill (read first) |
| [references/agent-desktop-windows.md](references/agent-desktop-windows.md) | Windows platform guide for the agent-desktop CLI. Lists per command group what works on. Triggers on: "windows desktop automation", "UIA tree", "automate Windows app", "agent-desktop on Windows" |
| [references/agent-desktop-linux.md](references/agent-desktop-linux.md) | Linux platform guide for the agent-desktop CLI. Lists per command group what works on X11 and on. Triggers on: "linux desktop automation", "AT-SPI tree", "automate GNOME app", "agent-desktop on Linux" |
| [references/agent-desktop-macos.md](references/agent-desktop-macos.md) | macOS platform guide for the agent-desktop CLI. Use for desktop automation on macOS: granting Accessibility and Screen Recording permission (TCC) |
| [references/agent-desktop-ffi.md](references/agent-desktop-ffi.md) | C-ABI bindings over agent-desktop's PlatformAdapter. Consumers (Python ctypes, Swift, Node ffi-napi, Go cgo, C++ |

<!-- merge-skill-families:v1 -->
