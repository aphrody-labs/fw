# Lot: serve fw-proto through fw-http (axum)

`fw-http` states that every other crate goes through it (`crates/fw-http/src/lib.rs:10`). `fw-proto` does not: `crates/fw-proto/src/server.rs` is a hand-written HTTP/1.1 listener that answers one request per connection (`Connection: close`) and writes `Access-Control-Allow-Origin: *` by hand in two places.

Measured on prd (Bun 1.4.5, rustc 1.100.0, load 35 to 40 on 8 cores, server pinned to 2 cores, `oha` 32 connections, keep-alive, 3 runs of 6 s): `fw serve` `/health` costs 69.8 CPU microseconds per request and 2,180 requests per second against 11.9 microseconds and 14,517 for a hello route on axum. Part of the gap is the connection per request.

Work:

1. Move the router of `fw-proto` onto `fw_http::server` (h1, h2, h3, Alt-Svc); keep the MCP, chat, GraphQL and cluster handlers as axum handlers.
2. Replace the hard-coded CORS headers with `tower-http` `CorsLayer`; add compression with `CompressionLayer`.
3. Replace the hand-written `handle_graphql` with `async-graphql` only if the schema grows; otherwise keep and record why.
4. Re-run `crates/fw-proto/tests/mcp_protocol.rs` and the MCP conformance suite (agent `mcp-conformance` owns it); re-measure with the same `oha` script.

Exit: `fw serve /health` within 2x of the axum hello CPU per request, conformance green.
