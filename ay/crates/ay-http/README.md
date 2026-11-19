# ay-http

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
cargo test -p ay-http --all-features
cargo clippy -p ay-http --all-features --all-targets -- -D warnings
```

## HTTP stack

`ay-http` is the one HTTP stack of the SDK.

- `ay_http::Client` (feature `client`): reqwest with rustls, h1, h2 and h3. An origin that answers with `Alt-Svc: h3=":443"` gets its next
  requests over QUIC; a failed QUIC attempt falls back to TCP and the origin stays on TCP for five minutes. Early data (0-RTT) is sent for
  `GET`, `HEAD` and `OPTIONS` only. `with_retries(n)` retries transient failures with backoff.
- `ay_http::server` (feature `server`): `serve` runs one axum `Router` over TCP (h1, h2, optional TLS) and QUIC (h3) on the same port and adds
  `Alt-Svc` to the TCP answers; `serve_listener` is the cleartext replacement of `axum::serve`.
- reqwest gates its h3 client behind `--cfg reqwest_unstable`, set for the whole workspace in `.cargo/config.toml`; `h3` and `h3-quinn` are
  patched to hyperium/h3 master in the root `Cargo.toml`.
- `cargo bench -p ay-http --bench latency` measures h1, h2 and h3 against the local server. The reqwest h3 client resets responses under a
  multi-thread runtime with concurrent connection churn (`tests/h3_client_threads.rs`, ignored); use it on a current-thread runtime until fixed.
