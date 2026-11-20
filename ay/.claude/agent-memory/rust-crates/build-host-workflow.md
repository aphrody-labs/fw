---
name: build-host-workflow
description: Gotchas of syncing and committing the ay workspace with the build host and the shared git index
metadata:
  type: feedback
---

`bun tools/sync-host.ts . --role build` applies `git diff HEAD` on the host; if a previous sync left an ignored file that the patch adds (the `*.p12` test data of crates/auto-sdk/tests/data), `git apply` fails and the host silently keeps the old tree. Remove those files on the host before every sync (the scratch script `rm -f .../crates/auto-sdk/tests/data/*`).

**Why:** a stale tree produced false "Finished" checks for a whole round.

**How to apply:** after each sync confirm `.sync.patch` is gone on the host or check one changed file's md5; HEAD must be pushed or the sync refuses.

`git commit --only -- <paths>` refuses untracked paths: run `git add -N <new dirs>` first, and for new files inside an already tracked crate directory add them too, otherwise the commit contains only the deletions.

cargo on the build host: use separate CARGO_TARGET_DIR per concurrent job (check, test, clippy, bench) or they block on the target lock; do not sync while a job still compiles from the sync tree.
