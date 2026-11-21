$ErrorActionPreference = "Stop"
param(
  [string]$Prompt = "Run concise security and type-safety review of current git diff."
)

Write-Host "[Triad CI] Running headless Claude, Codex, and Gemini checks..."
if (Get-Command claude.exe -ErrorAction SilentlyContinue) {
  & claude.exe --dangerously-skip-permissions -p $Prompt --output-format json | Out-File -Encoding utf8 triad-claude.json
}
if (Get-Command codex.cmd -ErrorAction SilentlyContinue) {
  & codex.cmd exec $Prompt --json --skip-git-repo-check | Out-File -Encoding utf8 triad-codex.jsonl
}
if (Get-Command gemini.exe -ErrorAction SilentlyContinue) {
  & gemini.exe -p $Prompt -o json | Out-File -Encoding utf8 triad-gemini.json
}
Write-Host "[Triad CI] Completed."
