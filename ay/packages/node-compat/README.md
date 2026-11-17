# @aphrody/ay-node-compat

Node.js compatibility shims on Bun. Each shim is its own module, so importing one never loads the others.

Legacy: restored from npm `@aphrody/node-cp@0.1.0`, `@aphrody/node-sqlite@0.1.0` and `@aphrody/node-test@0.1.0`.

| Subpath | Shims | Notes |
|---|---|---|
| `@aphrody/ay-node-compat/node-cp` | `getuid`, `getgid`, `geteuid`, `getegid`, `spawnWithIds` | libc through bun:ffi (`libc.so.6`, Linux); `spawnWithIds` adds `uid` and `gid` to the `Bun.spawn` result |
| `@aphrody/ay-node-compat/node-sqlite` | `DatabaseSync` | `node:sqlite` API (Node v22.5+) over `bun:sqlite`; foreign keys on by default |
| `@aphrody/ay-node-compat/node-test` | `test`, `it`, `describe`, `suite`, `before`, `after`, `beforeEach`, `afterEach`, `TestContext` | `node:test` over `bun:test`; `t.skip()` throws (bun:test has no runtime skip) |

```ts
import { DatabaseSync } from "@aphrody/ay-node-compat/node-sqlite";
import { test } from "@aphrody/ay-node-compat/node-test";
```

## Test

```
bun test
```

The `node-cp` tests need Linux (`libc.so.6`).
