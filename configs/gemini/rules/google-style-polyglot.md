# Google-Style Polyglot Standard (Python, Go, C, C++)

- **Reference canonique locale** : `~/.gemini/brain/google/styleguide` (Python, Go, C++, Shell, Markdown, Protobuf).
- **Python (3.15+)** : `uv` + `ruff` (formatage, lint, Google docstrings `D`, imports triés `I`, annotations strictes). Jamais `pip` nu.
- **Go** : `gofmt` / `go vet` / `gopls` / `golangci-lint` v2 / `govulncheck`, respect du Google Go Style Guide (noms courts idiomatiques, erreurs explicites, zéro `init()` magique).
- **C / C++ (LLVM 23 + Bazel 9 / CMake+Ninja+vcpkg)** : `clang`/`clang++` 23 + `lld`, formatage `clang-format --style=Google`, analyse statique `clang-tidy`, build `bazelisk` (Bazel 9) ou `cmake -G Ninja` + `vcpkg` manifeste (`C:\vcpkg`).
