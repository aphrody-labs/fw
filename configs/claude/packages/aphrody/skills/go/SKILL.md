---
name: go
description: Authoritative Go 1.27+ toolchain, module/workspace architecture, golang/tools (gopls, govulncheck), golangci-lint v2, fuzzing/benchmarks, and c-shared C ABI FFI integration for Aphrody and Fairwind. Use for any Go code, go.mod/go.work, CLI/server builds, or Go-to-Bun/Rust FFI bridges.
---

# Go Engineering Reference (Verified 2026-11-19)

Live-verified against `https://go.dev/dl/?mode=json`, `https://github.com/golang/tools`, and `https://github.com/golangci/golangci-lint`.

## Verified Toolchain & Ecosystem

| Component | Version | Source |
| --- | --- | --- |
| Go (latest stable & local) | `go1.27.4` (`windows/amd64`) | `https://go.dev/dl/?mode=json` |
| Go (previous stable) | `go1.26.11` | `https://go.dev/dl/?mode=json` |
| `golang.org/x/tools` (`gopls`, `goimports`) | `v0.53.0` | `github.com/golang/tools` |
| `golangci-lint` | `v2.15.0` | `github.com/golangci/golangci-lint` |

## Standard Verification Gates

```sh
go vet ./...
go test -race -count=1 ./...
golangci-lint run ./...
govulncheck ./...
go build -trimpath -ldflags="-s -w" ./...
```

## C ABI / `bun:ffi` Single-Binary Interop

Build a Go package as a C-compatible shared library for `bun:ffi`:

```sh
go build -buildmode=c-shared -trimpath -ldflags="-s -w" -o dist/libgo_bridge.dll ./cmd/bridge
```

- Export C functions with `import "C"` and `//export <fn_name>`.
- Keep FFI boundaries allocation-safe (caller-allocated buffers or explicit `Free` exports).
- Never claim a Go build or test passed without running `go vet ./...` and `go test ./...` live (`exit 0`).

## Google Go Style Guide (`google/styleguide`, 8,913 lines, 4 docs)

- **Core Principles** (`guide.md`): Clarity > Simplicity > Concision > Maintainability > Consistency. Always `gofmt`, `MixedCaps` (no underscores except test/generated), no rigid line length limit (refactor over wrapping), local consistency.
- **Normative Decisions** (`decisions.md`, 72 sections):
  - **Naming**: short lowercase package names, 1–2 letter receiver names (no `this`/`self`), consistent initialisms (`URL`, `ID`, `HTTP`), no `Get` prefix on getters, avoid stutter (`ch.Read` not `ch.ReadChannel`).
  - **Errors**: return `error` as last value, lowercase error strings with no trailing punctuation, handle every error, indent error flow (`if err != nil { return ... }`, no `else`), never panic in normal error handling (`Must*` only at init/test).
  - **Language**: prefer `nil` slices over empty slice literals, prefer synchronous functions (let caller spawn goroutines), always document goroutine lifetimes, define interfaces at the consumer side, use `any` over `interface{}`, use `%q` for quoted strings in logs/errors, use `crypto/rand` for security.
  - **Testing**: no assertion libraries (use standard `testing` + `google/go-cmp/cmp`), `got` before `want` (`Foo(x) = %v, want %v`), prefer `t.Error` over `t.Fatal` unless continuing is meaningless, never call `t.Fatal` from a separate goroutine, use table-driven subtests (`t.Run`) and `t.Helper()`.
- **Best Practices** (`best-practices.md`, 57 sections):
  - Avoid `util`/`common`/`helper` packages; place `%w` at the end of wrapped errors (`fmt.Errorf("...: %w", err)`); pass `context.Context` as first parameter (never store in structs); use `strings.Builder` for piecemeal concatenation; avoid package-level mutable global state; accept interfaces, return concrete structs.

