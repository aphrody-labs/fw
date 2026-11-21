# Chromium docs from the corpus

Distilled from the local corpus on `fr1` (`fw-corpus`, read 2026-11-21); paraphrased, each fact cites the stored URL (BSD-3-Clause source). Re-query before relying on a detail: `fw-corpus search "<words>" --source chromium-docs|chromium-repos`.

## Sources

| id | holds | notes |
| --- | --- | --- |
| `chromium-docs` | `chromium/src` `docs/`, `styleguide/`, and README files under `third_party/blink/renderer`, `third_party/blink/public`, `content/public` | gitiles file reads pinned to the HEAD commit read; `url` is `.../+/HEAD/<path>`, `pinned` carries the commit |
| `chromium-repos` | inventory of chromium.googlesource.com repositories with HEAD commit and description | one document per repository; the root listing is disallowed by robots.txt, so the HTML index is parsed |

## Threading and tasks

Source: `https://chromium.googlesource.com/chromium/src/+/HEAD/docs/threading_and_tasks.md` (pinned `59fa9c276a82c6b35a37d20ee08637a8b4ad27c0`).

- Chrome is multi-process and every process is heavily multi-threaded; the goal is a responsive browser, so expensive computation and blocking IO stay off the UI thread and the IO thread.
- Locks are discouraged: tasks posted to one sequence run in order, and the next task may run on a different worker thread, so a class that is only touched from one sequence needs no lock of its own (`#using-sequences-instead-of-locks`).
- A new sequence comes from `base::ThreadPool::CreateSequencedTaskRunner()`; the current thread's default queue is reached with `base::SequencedTaskRunner::GetCurrentDefault()`, and the "current" task runner is deliberately not exposed as a static getter (`#posting-to-a-new-sequence`, `#posting-to-the-current-virtual-thread`).
