# ADR-3: Browser on Chromium behind one engine interface

Status: proposed, 2026-11-16; accepted after the measurements below.

## Decision (proposed)
Chromium from chromium.googlesource.com, fetched with depot_tools, built with `autoninja` and ThinLTO on the build
host, with our UI, the agent panel, WebMCP and Aphrody Search as the default engine. The engine sits behind one
interface so Ladybird or Servo can be evaluated without touching the UI.

## Measurements required before acceptance
Disk and time of a release build on the build host, binary size, web-platform-tests pass rate, and how Rust code
enters (Chromium's `docs/rust/README.md`).

## Blocker
Chromium's Linux build instructions require at least 100 GB of free disk; the build host has less today. A build
volume is a prerequisite.
