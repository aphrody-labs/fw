# syntax=docker/dockerfile:1.7
# Fairwind v4.1 — Zero-Debian/Ubuntu Alpine Linux static musl multi-stage image
FROM rust:1.91-alpine AS builder
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY web ./web
RUN cargo build --release --target x86_64-unknown-linux-musl -p fw-bin \
    && strip target/x86_64-unknown-linux-musl/release/fairwind-native

FROM alpine:3.22 AS runtime
RUN apk add --no-cache ca-certificates wget \
    && adduser -D -u 10001 fairwind
COPY --from=builder /src/target/x86_64-unknown-linux-musl/release/fairwind-native /usr/local/bin/fairwind-native
USER fairwind
EXPOSE 4380
HEALTHCHECK --interval=15s --timeout=3s --start-period=5s --retries=3 \
    CMD wget -qO- http://127.0.0.1:4380/health || exit 1
ENTRYPOINT ["/usr/local/bin/fairwind-native"]
CMD ["serve", "0.0.0.0:4380"]
