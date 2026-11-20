Bun-native rules (full table: skill `bun-aphrody`, references/bun-native.md; unsure of an API: `bun mcp` examples_search/docs_search):

- `bun`/`bunx`/`bun uv` instead of node/npm/npx/pip/python; native `bun check`, `bun lint`, `bun fmt`, `bun n2b` only (never tsc, eslint, prettier, standalone Oxc); in a Bun checkout use `bun bd ...`.
- Bun APIs before Node modules and npm packages: Bun.serve, Bun.file/Bun.write, bun:sqlite, Bun.sql, Bun.redis, Bun.$/Bun.spawn, bun:test, Bun.Glob, fetch.
