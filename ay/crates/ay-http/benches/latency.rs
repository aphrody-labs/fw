//! Latency of h1, h2 and h3 against the localhost server of this crate: `cargo bench -p ay-http --bench latency`.
//!
//! Warm: one connection reused for every request. Cold: a new client (new handshake) per request. Blob: a 1 MiB download on a warm connection.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use axum::Router;
use axum::routing::get;
use ay_http::server::{ServeConfig, Tls, serve};
use ay_http::{Client, Version};
use bytes::Bytes;

const WARM: usize = 2_000;
const COLD: usize = 100;
const BLOB: usize = 100;

fn percentile(sorted: &[Duration], p: f64) -> Duration {
    let index = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[index]
}

fn report(name: &str, mode: &str, mut samples: Vec<Duration>) {
    samples.sort();
    let mean = samples.iter().sum::<Duration>() / samples.len() as u32;
    println!(
        "{name:<4} {mode:<5} n={:<5} mean {:>9.1?}  p50 {:>9.1?}  p95 {:>9.1?}  p99 {:>9.1?}",
        samples.len(),
        mean,
        percentile(&samples, 0.50),
        percentile(&samples, 0.95),
        percentile(&samples, 0.99)
    );
}

fn build(pem: &str, addr: SocketAddr, protocol: &str) -> Client {
    let cert = ay_http::reqwest::Certificate::from_pem(pem.as_bytes()).expect("certificate");
    let builder = Client::builder().add_root_certificate(cert).resolve("localhost", addr).http3(protocol == "h3");
    let builder = match protocol {
        "h1" => builder.with(|b| b.http1_only()),
        _ => builder,
    };
    builder.build().expect("client")
}

async fn prepared(pem: &str, addr: SocketAddr, protocol: &str, url: &str) -> Client {
    let client = build(pem, addr, protocol);
    let first = client.get(url).send().await.expect("first request");
    let expected = match protocol {
        "h1" => Version::HTTP_11,
        "h2" => Version::HTTP_2,
        _ => Version::HTTP_2, // the first answer arrives over TCP and carries Alt-Svc
    };
    assert_eq!(first.version(), expected);
    first.bytes().await.expect("body");
    if protocol == "h3" {
        let second = client.get(url).send().await.expect("h3 request");
        assert_eq!(second.version(), Version::HTTP_3, "the client must be on h3 for the measurement");
        second.bytes().await.expect("body");
    }
    client
}

#[tokio::main]
async fn main() {
    let (tls, pem) = Tls::self_signed(&["localhost"]).expect("self-signed certificate");
    let blob = Bytes::from(vec![7u8; 1 << 20]);
    let router = Router::new().route("/ping", get(|| async { "pong" })).route("/blob", get(move || {
        let blob = blob.clone();
        async move { blob }
    }));
    let server = serve(router, ServeConfig::secure("127.0.0.1:0".parse().expect("addr"), tls)).await.expect("server");
    let addr: SocketAddr = format!("127.0.0.1:{}", server.tcp_addr().port()).parse().expect("addr");
    let ping = format!("https://localhost:{}/ping", addr.port());
    let blob_url = format!("https://localhost:{}/blob", addr.port());

    println!("ay-http latency, TLS on localhost ({} warm, {} cold, {} x 1 MiB)", WARM, COLD, BLOB);
    let only = std::env::var("AY_BENCH_ONLY").ok();
    for protocol in ["h1", "h2", "h3"] {
        if only.as_deref().is_some_and(|o| o != protocol) {
            continue;
        }
        let client = prepared(&pem, addr, protocol, &ping).await;
        let mut warm = Vec::with_capacity(WARM);
        for i in 0..WARM {
            let start = Instant::now();
            let response = match client.get(&ping).send().await {
                Ok(response) => response,
                Err(error) => panic!("{protocol} warm request {i}: {error:?}"),
            };
            response.bytes().await.expect("body");
            warm.push(start.elapsed());
        }
        report(protocol, "warm", warm);

        let mut cold = Vec::with_capacity(COLD);
        for _ in 0..COLD {
            let fresh = build(&pem, addr, protocol);
            if protocol == "h3" {
                let origin = ay_http::altsvc::Origin::of_https(&ping.parse().expect("url")).expect("origin");
                fresh.alt_svc().insert(origin, Duration::from_secs(60));
            }
            let start = Instant::now();
            let response = fresh.get(&ping).send().await.expect("request");
            response.bytes().await.expect("body");
            cold.push(start.elapsed());
        }
        report(protocol, "cold", cold);

        let client = prepared(&pem, addr, protocol, &blob_url).await;
        let mut big = Vec::with_capacity(BLOB);
        for _ in 0..BLOB {
            let start = Instant::now();
            let response = client.get(&blob_url).send().await.expect("request");
            assert_eq!(response.bytes().await.expect("body").len(), 1 << 20);
            big.push(start.elapsed());
        }
        report(protocol, "1MiB", big);
    }
    server.shutdown().await;
}
