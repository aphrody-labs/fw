# ay-http-error

One JSON error envelope for HTTP services built on axum:

```json
{"error": {"code": "404", "message": "missing", "data": {"optional": "details"}}}
```

`data` is omitted when absent. `ErrorEnvelope::new` builds the value, `error_response` builds the axum
`Response` for a status code. Servers that must mimic a provider's own error format (OpenAI, Anthropic)
should keep that format instead of using this envelope.

Legacy published name: `aphrody-http-error` (crates.io, 1.4.7). This crate is the restored source under the
repository's own name; it is not published.

## Test

```sh
cargo test -p ay-http-error
cargo clippy -p ay-http-error --all-targets -- -D warnings
```
