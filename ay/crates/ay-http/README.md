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
  requests over QUIC. A QUIC attempt that fails before the response head (connect error, timeout, stream reset, request-phase error) is
  replayed once over TCP when the method is idempotent, and the origin stays on TCP until its Alt-Svc freshness ends; a reset after the
  response head is a body error the caller sees. Early data is enabled for `GET`, `HEAD` and `OPTIONS`, but reqwest 0.13.5 does not send
  0-RTT yet (seanmonstar/reqwest#3148). `with_retries(n)` retries transient failures with backoff.
- `ay_http::server` (feature `server`): `serve` runs one axum `Router` over TCP (h1, h2, optional TLS) and QUIC (h3) on the same port and adds
  `Alt-Svc` to the TCP answers; `serve_listener` is the cleartext replacement of `axum::serve`.
- reqwest gates its h3 client behind `--cfg reqwest_unstable`, set for the whole workspace in `.cargo/config.toml`; `h3` and `h3-quinn` are
  patched to hyperium/h3 master in the root `Cargo.toml`.
- The server lets a request end by itself before it stops reading (at most 250 ms, then H3_NO_ERROR); stopping earlier makes quinn send
  STOP_SENDING with code 0 and reqwest fails the exchange (seanmonstar/reqwest#3147, covered by `tests/h3_client_threads.rs`). A request
  that arrives as early data carries `Early-Data: 1` and the `EarlyData` extension; early requests with unsafe methods get `425`.
- `cargo bench -p ay-http --bench latency` measures h1, h2 and h3 against the local server.
