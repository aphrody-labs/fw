# fw-google

Typed Google Workspace and YouTube client for `fw`: a library other crates can depend on and the
`fw-google` binary that `fw google <service> <op>` runs (the way `fw convert` runs `fw-video`).

## Upstream choice

Versions, licences and dates below were read from the crates.io API and the GitHub API on 2026-11-21.

| Candidate | Covers Workspace + YouTube | Latest release | Licence | Verdict |
|---|---|---|---|---|
| `google-apis-rs` generated crates (Byron/google-apis-rs, main pushed 2026-10-17, not archived) | Drive, Sheets, Gmail, Calendar, Docs, YouTube: yes | `google-drive3` 7.0.0+20251218, `google-sheets4` / `google-gmail1` / `google-docs1` 7.0.0+20251215, `google-calendar3` 7.0.0+20251214, `google-youtube3` 7.0.0+20251222, all published 2026-01-01; `google-apis-common` 8.0.0 (2025-07-24) | MIT (crate metadata; GitHub reports NOASSERTION for the repo) | **chosen** |
| `googleapis/google-cloud-rust` (official, Apache-2.0, `release-20261111`, `google-cloud-auth` 1.19.0 published 2026-11-11) | no: Cloud APIs only; `src/generated/apps` holds `card`, `events`, `meet`, `script`; there is no Drive, Sheets, Gmail, Calendar, Docs or YouTube crate (no `google-cloud-drive` on crates.io) | 2026-11-11 | Apache-2.0 | not usable for these services |
| thin hand client over REST | whatever we write | n/a | ours | rejected: would re-own the schemas of six large APIs; the generated crates already carry them and track Google's discovery documents |

`google-apis-rs` is the only maintained Rust source for these six APIs. Its crates are on crates.io, so
nothing here needs a git dependency and `fw-bin` stays publishable. Its auth layer is
`yup-oauth2` 12.1.2 (MIT OR Apache-2.0, published 2026-01-07, repo pushed 2026-02-06); both are
registered in `upstreams.toml` with `watch = true`.

Known weakness: the generated crates are released about once a year (6.0.0 on 2024-10-15, 7.0.0 on
2026-01-01), so new API fields can lag; the code is regenerated from Google discovery documents, and
`fw-google` pins the major version (`7`).

Cost: a cold `cargo check -p fw-google --all-targets` on fr1 (12 cores) took 1m31s including all
dependencies. That weight is why `fw-bin` does not link this crate: `fw google` spawns `fw-google`,
looked up next to `fw`, then on `PATH` (`cargo build --release -p fw-google`).

TLS is rustls with aws-lc-rs (already built for `fw-http`) and the webpki root set.

## Credentials

In order:

1. an explicit access token: `--access-token` or `FW_GOOGLE_ACCESS_TOKEN` (library: `Credentials::token`);
2. `gcloud auth print-access-token`, asked for the scopes of the call (`--scopes=...`), retried without scopes
   for user credentials (library: `Credentials::gcloud`);
3. Application Default Credentials: `GOOGLE_APPLICATION_CREDENTIALS`, then `$CLOUDSDK_CONFIG`,
   `%APPDATA%\gcloud` or `~/.config/gcloud` `application_default_credentials.json`; service-account and
   authorized-user files (library: `Credentials::adc`). The GCE metadata server is not queried.

`--auth gcloud|adc` pins one source. Tokens never appear in `Debug`, `Display` or error messages (unit tests).
Service accounts only see what was shared with them; user credentials from plain `gcloud auth login` carry
Cloud scopes, not Workspace ones, so use `gcloud auth application-default login --scopes=...` or a service account
for Workspace data.

## Operations

| Command | Library method | Scope |
|---|---|---|
| `drive list [--query Q] [--page-size N] [--page-token T] [--order-by O]` | `drive_list` | `drive.readonly` |
| `drive export <file-id> --mime MIME [-o FILE]` | `drive_export` | `drive.readonly` |
| `sheets get <id> <range>` | `sheets_values_get` | `spreadsheets.readonly` |
| `sheets update <id> <range> --values JSON [--raw]` | `sheets_values_update` | `spreadsheets` |
| `gmail list [--query Q] [--max N]` | `gmail_list` | `gmail.readonly` |
| `gmail draft --to A --subject S --body TEXT` | `gmail_draft_create` (creates a draft; there is no send operation) | `gmail.compose` |
| `calendar events [--calendar ID] [--from now\|RFC3339] [--max N]` | `calendar_events` | `calendar.events.readonly` |
| `docs text <id>` | `docs_text` | `documents.readonly` |
| `youtube channels (--mine \| --id ..)`, `videos --id ..`, `playlists (--mine \| --channel ID \| --id ..)`, `playlist-items <id>` | `youtube_*` | `youtube.readonly` |
| `youtube update-video <id> [--title ..] [--description ..] [--tags a,b] [--category-id N] --apply` | `youtube_video_update` | `youtube.force-ssl` |

`youtube update-video` is refused without `--apply`; in the library it takes a `WriteEnabled` value that can only
be built from an explicit flag. Output is JSON without null members (`docs text` prints text, `drive export`
prints the bytes or writes `-o FILE`).

## Tests

`cargo test -p fw-google` runs offline. `src/mock.rs` is an in-memory HTTP transport (a connector over
`tokio::io::duplex`): each test drives the real generated client against JSON fixtures in `tests/fixtures/`
(shaped like the API responses, trimmed) and asserts the request that was built (method, path, query,
`Authorization` header, body) and the typed parse of the response. No socket is opened.

## Live smoke (read-only)

```sh
cargo build --release -p fw-google
target/release/fw-google drive list --page-size 3                      # needs gcloud or ADC credentials
target/release/fw-google --access-token "$(gcloud auth print-access-token --scopes=https://www.googleapis.com/auth/drive.readonly)" drive list --page-size 3
fw google youtube channels --mine                                       # through the fw binary
```
