# aphrody

Cross-CLI engineering plugin for Claude Code and Antigravity (`agy`), served by the `fw` marketplace.

- `agents/`: `worker` and `researcher` autonomous profiles.
- `output-styles/autonome.md`: the Autonome output style.
- `skills/`: curated cross-project skills, one tree shared by both CLIs.
- `rules/AGENTS.md`: context file for agy / Gemini CLI.
- `.claude-plugin/plugin.json` (Claude Code), `plugin.json` and `gemini-extension.json` (agy): same tree, three manifests.

## Install

```text
claude plugin marketplace add $WORKSPACE_ROOT/fw
claude plugin install aphrody@fw
bun ~/.gemini/scripts/install.ts   # agy: materializes every fw plugin
```
