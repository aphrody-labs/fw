# Lot: auth parity gaps (ay auto-sdk against better-auth)

Source: the better-auth rows of `docs/research/web-vs-rust-parity-2026-11.md`. Order is by user impact.

## 1. Defect: the scrypt compat hash skips NFKC

- `ay/crates/auto-sdk/src/auth/password.rs` `hash_password_scrypt_compat` normalises through `NfkcShim`, whose body is `self.as_bytes().to_vec()` (a no-op). better-auth 1.7.27 hashes `password.normalize("NFKC")` (`@better-auth/utils` `dist/password.node.mjs`).
- Measured on prd with the same parameters and salt: the Rust scrypt crate 0.12.0 gives `3705e184...` for `fi`, equal to Bun for both `fi` and the ligature U+FB01; Rust over the raw ligature bytes gives `772eb502...`. A user whose password contains a character that NFKC rewrites cannot be verified against a hash written by better-auth, and the reverse.
- Fix: normalise with `unicode-normalization` (0.1.26 is already in the workspace `Cargo.lock`) in both hash and verify, add a test with the ligature vector above.

## 2. Allocation cost of scrypt

On prd the Rust scrypt bench spent 9.5 s and 10.6 s of system time in two runs of 405 hashes (user 20.5 s and 20.8 s), the Bun run 0.9 s and 0.5 s (user 27.8 s and 27.7 s). CPU per hash 74.1 ms and 77.6 ms against 70.8 ms and 69.7 ms. Try a reusing allocator (mimalloc) or a reused scratch buffer and re-measure; target is the Bun figure.

## 3. Provider catalogue

better-auth core ships 37 social provider files (`@better-auth/core@1.7.27 dist/social-providers/`); `oauth.rs` `OAuthProvider` has 3 (Google, Microsoft, GitHub). Port the provider table (endpoints, scopes, profile mapping) on top of `oauth2` and `openidconnect`.

## 4. Missing flows

Email verification, password reset, account linking, magic link, email and phone OTP, admin (ban, impersonate), organization invitations and teams, OAuth and OIDC provider endpoints (authorize, token; metadata is served), passkey registration ceremony (adopt `webauthn-rs`), rate limiting (adopt `tower-governor`), OpenAPI (adopt `utoipa-axum`), social-provider breadth, SQL beyond SQLite.

## Exit

A conformance test runs the better-auth TypeScript client against the Rust server for sign-up, sign-in, get-session, sign-out, 2FA and token; each flow above is a test or is listed as out of scope with a reason.
