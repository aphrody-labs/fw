//! The server speaks h1, h2 and h3 on one router; the client upgrades to h3 through Alt-Svc.

use std::net::SocketAddr;
use std::time::Duration;

use axum::Router;
use axum::extract::Request;
use axum::routing::{get, post};
use ay_http::server::{ServeConfig, Server, Tls, serve};
use ay_http::{Client, Version};

fn router() -> Router {
    Router::new()
        .route("/ping", get(|| async { "pong" }))
        .route("/version", get(|request: Request| async move { format!("{:?}", request.version()) }))
        .route("/echo", post(|body: axum::body::Bytes| async move { body }))
}

struct Fixture {
    server: Server,
    pem: String,
    port: u16,
}

async fn secure_server() -> Fixture {
    let (tls, pem) = Tls::self_signed(&["localhost"]).unwrap();
    let server = serve(router(), ServeConfig::secure("127.0.0.1:0".parse().unwrap(), tls)).await.unwrap();
    let port = server.tcp_addr().port();
    assert_eq!(server.quic_addr().map(|a| a.port()), Some(port), "h3 listens on the TCP port number");
    Fixture { server, pem, port }
}

fn client(fixture: &Fixture) -> Client {
    let cert = ay_http::reqwest::Certificate::from_pem(fixture.pem.as_bytes()).unwrap();
    let addr: SocketAddr = format!("127.0.0.1:{}", fixture.port).parse().unwrap();
    Client::builder()
        .add_root_certificate(cert)
        .resolve("localhost", addr)
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap()
}

#[tokio::test]
async fn tcp_response_advertises_alt_svc_and_the_next_request_goes_over_h3() {
    let fixture = secure_server().await;
    let client = client(&fixture);
    let url = format!("https://localhost:{}/ping", fixture.port);

    let first = client.get(&url).send().await.unwrap();
    assert_eq!(first.version(), Version::HTTP_2, "TLS over TCP negotiates h2");
    let alt_svc = first.headers().get("alt-svc").expect("Alt-Svc is advertised").to_str().unwrap().to_owned();
    assert_eq!(alt_svc, format!("h3=\":{}\"; ma=86400", fixture.port));
    assert_eq!(first.text().await.unwrap(), "pong");

    let second = client.get(&url).send().await.unwrap();
    assert_eq!(second.version(), Version::HTTP_3, "the client upgraded to h3 after Alt-Svc");
    assert_eq!(second.text().await.unwrap(), "pong");

    let version = client.get(format!("https://localhost:{}/version", fixture.port)).send().await.unwrap();
    assert_eq!(version.version(), Version::HTTP_3);
    assert_eq!(version.text().await.unwrap(), "HTTP/3.0", "the router saw an HTTP/3 request");
    fixture.server.shutdown().await;
}

#[tokio::test]
async fn h3_carries_request_and_response_bodies() {
    let fixture = secure_server().await;
    let client = client(&fixture);
    let url = format!("https://localhost:{}/echo", fixture.port);
    client.get(format!("https://localhost:{}/ping", fixture.port)).send().await.unwrap();

    let payload: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    let response = client.post(&url).body(payload.clone()).send().await.unwrap();
    assert_eq!(response.version(), Version::HTTP_3);
    assert_eq!(response.bytes().await.unwrap().as_ref(), payload.as_slice());
    fixture.server.shutdown().await;
}

#[tokio::test]
async fn a_dead_h3_path_falls_back_to_tcp() {
    // A TLS server without a QUIC listener; the origin is nevertheless seeded as h3 capable.
    let (tls, _) = Tls::self_signed(&["localhost"]).unwrap();
    let mut config = ServeConfig::secure("127.0.0.1:0".parse().unwrap(), tls);
    config.h3 = false;
    let server = serve(router(), config).await.unwrap();
    let port = server.tcp_addr().port();
    let url = format!("https://localhost:{port}/ping");
    let origin = ay_http::altsvc::Origin::of_https(&url.parse().unwrap()).unwrap();

    let client = Client::builder()
        .danger_accept_invalid_certs(true)
        .resolve("localhost", format!("127.0.0.1:{port}").parse().unwrap())
        .connect_timeout(Duration::from_secs(1))
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    client.alt_svc().insert(origin.clone(), Duration::from_secs(60));
    assert!(client.alt_svc().h3_usable(&origin));

    let response = client.get(&url).send().await.unwrap();
    assert_eq!(response.version(), Version::HTTP_2, "fell back to TCP");
    assert_eq!(response.text().await.unwrap(), "pong");
    assert!(!client.alt_svc().h3_usable(&origin), "the origin is backed off after the failed QUIC attempt");
    server.shutdown().await;
}

#[tokio::test]
async fn cleartext_serves_h1_and_h2c() {
    let server = serve(router(), ServeConfig::plain("127.0.0.1:0".parse().unwrap())).await.unwrap();
    assert!(server.quic_addr().is_none());
    let url = format!("http://127.0.0.1:{}/ping", server.tcp_addr().port());
    let h1 = Client::builder().http3(false).build().unwrap().get(&url).send().await.unwrap();
    assert_eq!(h1.version(), Version::HTTP_11);
    assert!(h1.headers().get("alt-svc").is_none(), "no Alt-Svc without h3");
    let h2c = Client::builder().with(|b| b.http2_prior_knowledge()).http3(false).build().unwrap().get(&url).send().await.unwrap();
    assert_eq!(h2c.version(), Version::HTTP_2);
    server.shutdown().await;
}

#[tokio::test]
async fn h3_without_tls_is_refused() {
    let mut config = ServeConfig::plain("127.0.0.1:0".parse().unwrap());
    config.h3 = true;
    assert!(serve(router(), config).await.is_err());
}

#[tokio::test]
async fn one_h3_connection_serves_many_sequential_requests() {
    let fixture = secure_server().await;
    let client = client(&fixture);
    let url = format!("https://localhost:{}/ping", fixture.port);
    client.get(&url).send().await.unwrap();
    for i in 0..3_000 {
        let response = client.get(&url).send().await.unwrap_or_else(|e| panic!("request {i}: {e:?}"));
        assert_eq!(response.version(), Version::HTTP_3, "request {i}");
        assert_eq!(response.text().await.unwrap(), "pong");
    }
    fixture.server.shutdown().await;
}
