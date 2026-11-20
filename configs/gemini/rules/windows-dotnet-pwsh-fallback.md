---
trigger: always_on
description: Windows-native tooling fallback using pwsh 7.7+, .NET 10/11, and vcpkg when bun/rust are absent.
---
# Windows Native Tooling Fallback (`pwsh` + `.NET` + `vcpkg`)

- Avant d'invoquer `bun` ou `cargo`, vérifier leur présence (`Get-Command bun, cargo -ErrorAction SilentlyContinue`) ou utiliser directement `pwsh` 7.7+ et `dotnet` 10/11 lorsqu'ils sont absents.
- **Scripting & JSON/TOML/Git** : utiliser `pwsh` pur (`ConvertFrom-Json`, `ConvertTo-Json`, `Select-String`, `Get-FileHash`, `gh api ... | ConvertFrom-Json`). Toujours parenthéser les expressions passées aux méthodes .NET (`[regex]::Escape(($a -replace '-', '_'))`).
- **Outils compilés / performants sans Rust/Bun** : utiliser les file-based apps C# de `.NET 10/11` (`dotnet run tool.cs`) ou compiler en binaire natif AOT (`dotnet publish -c Release -r win-x64 -p:PublishAot=true`).
- **Dépendances C/C++** : utiliser `vcpkg` en mode manifeste (`vcpkg.json`) couplé à `cmake` + `ninja`, jamais de téléchargements manuels de DLL/headers.
- **Documentation** : consulter en priorité `~/.gemini/brain/microsoft/{PowerShell-Docs,dotnet-docs}` ou le MCP `microsoft-learn` (`https://learn.microsoft.com/api/mcp`).
