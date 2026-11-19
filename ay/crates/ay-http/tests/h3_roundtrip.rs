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

#[tokio::test]
async fn every_new_h3_connection_answers_its_first_request() {
    let fixture = secure_server().await;
    let url = format!("https://localhost:{}/ping", fixture.port);
    let origin = ay_http::altsvc::Origin::of_https(&url.parse().unwrap()).unwrap();
    for i in 0..200 {
        let client = client(&fixture);
        client.alt_svc().insert(origin.clone(), Duration::from_secs(60));
        let response = client.get(&url).send().await.unwrap_or_else(|e| panic!("connection {i}: {e:?}"));
        assert_eq!(response.version(), Version::HTTP_3, "connection {i}");
        assert_eq!(response.text().await.unwrap_or_else(|e| panic!("body {i}: {e:?}")), "pong");
    }
    fixture.server.shutdown().await;
}

/// A QUIC endpoint on `port` that speaks h3 but resets every request stream before answering (a server that stops reading and
/// writing with quinn's default code 0), with the certificate of `pem`/`key_pem`.
fn broken_h3_listener(port: u16, cert_pem: &str, key_pem: &str) -> quinn::Endpoint {
    use rustls::pki_types::pem::PemObject;
    let chain = rustls::pki_types::CertificateDer::pem_slice_iter(cert_pem.as_bytes()).collect::<Result<Vec<_>, _>>().unwrap();
    let key = rustls::pki_types::PrivateKeyDer::from_pem_slice(key_pem.as_bytes()).unwrap();
    let provider = std::sync::Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let mut config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(chain, key)
        .unwrap();
    config.alpn_protocols = vec![b"h3".to_vec()];
    let quic = quinn::crypto::rustls::QuicServerConfig::try_from(std::sync::Arc::new(config)).unwrap();
    let endpoint = quinn::Endpoint::server(quinn::ServerConfig::with_crypto(std::sync::Arc::new(quic)), format!("127.0.0.1:{port}").parse().unwrap()).unwrap();
    let accepting = endpoint.clone();
    tokio::spawn(async move {
        while let Some(incoming) = accepting.accept().await {
            tokio::spawn(async move {
                let Ok(connection) = incoming.await else { return };
                let Ok(mut h3) = h3::server::Connection::<_, bytes::Bytes>::new(h3_quinn::Connection::new(connection)).await else { return };
                while let Ok(Some(resolver)) = h3.accept().await {
                    // Resolve the request, then drop the stream: reset with code 0, no response.
                    tokio::spawn(async move {
                        let _ = resolver.resolve_request().await;
                    });
                }
            });
        }
    });
    endpoint
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_reset_h3_stream_is_replayed_over_tcp_and_the_origin_stays_on_tcp() {
    // The same certificate for TCP and QUIC.
    let rcgen::CertifiedKey { cert, signing_key } = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).unwrap();
    let (cert_pem, key_pem) = (cert.pem(), signing_key.serialize_pem());
    let tcp_tls = Tls::from_pem(cert_pem.as_bytes(), key_pem.as_bytes()).unwrap();
    let mut config = ServeConfig::secure("127.0.0.1:0".parse().unwrap(), tcp_tls);
    config.h3 = false;
    let server = serve(router(), config).await.unwrap();
    let port = server.tcp_addr().port();
    let _endpoint = broken_h3_listener(port, &cert_pem, &key_pem);

    let root = ay_http::reqwest::Certificate::from_pem(cert_pem.as_bytes()).unwrap();
    let client = Client::builder()
        .add_root_certificate(root)
        .resolve("localhost", format!("127.0.0.1:{port}").parse().unwrap())
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    let url = format!("https://localhost:{port}/ping");
    let origin = ay_http::altsvc::Origin::of_https(&url.parse().unwrap()).unwrap();
    client.alt_svc().insert(origin.clone(), Duration::from_secs(60));
    assert!(client.alt_svc().h3_usable(&origin));

    let response = client.get(&url).send().await.expect("the reset is absorbed by the replay over TCP");
    assert_eq!(response.version(), Version::HTTP_2, "answered over TCP");
    assert_eq!(response.text().await.unwrap(), "pong");
    assert!(!client.alt_svc().h3_usable(&origin), "the origin is marked h3-broken for the Alt-Svc freshness");

    // A POST is not replayed after a reset: nothing here proves the server did not act on it.
    client.alt_svc().insert(origin.clone(), Duration::from_secs(60));
    assert!(client.alt_svc().h3_usable(&origin));
    let post = client.post(format!("https://localhost:{port}/echo")).body("x").send().await;
    assert!(post.is_err(), "a non-idempotent request is not replayed");
    server.shutdown().await;
}

fn early_router() -> Router {
    Router::new()
        .route("/early", get(|request: Request| async move { if request.headers().contains_key("early-data") { "1" } else { "0" } }))
        .route("/ping", get(|| async { "pong" }))
}

/// Resumes the TLS session of a first h3 connection on a second one against a server that accepts early data, and sends a GET.
/// reqwest enables early data in its TLS config but its h3 connector completes the handshake before it sends (it never calls
/// quinn's `into_0rtt`), so today the GET travels as ordinary 1-RTT data; the server reports which one it saw and both are accepted.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_resumed_h3_connection_serves_a_get_with_early_data_enabled() {
    let (tls, pem) = Tls::self_signed(&["localhost"]).unwrap();
    let mut config = ServeConfig::secure("127.0.0.1:0".parse().unwrap(), tls);
    config.early_data = true;
    let server = serve(early_router(), config).await.unwrap();
    let port = server.tcp_addr().port();

    let cert = ay_http::reqwest::Certificate::from_pem(pem.as_bytes()).unwrap();
    // A pool that forgets idle connections quickly, so the second request opens a new connection that can resume the first session.
    let client = Client::builder()
        .add_root_certificate(cert)
        .resolve("localhost", format!("127.0.0.1:{port}").parse().unwrap())
        .pool_idle_timeout(Duration::from_millis(50))
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    let url = format!("https://localhost:{port}/early");
    client.get(&url).send().await.unwrap();
    let first = client.get(&url).send().await.unwrap();
    assert_eq!(first.version(), Version::HTTP_3);
    assert_eq!(first.text().await.unwrap(), "0", "a fresh connection has no early data");

    tokio::time::sleep(Duration::from_millis(300)).await;
    let resumed = client.get(&url).send().await.expect("the resumed connection answers");
    assert_eq!(resumed.version(), Version::HTTP_3, "the second connection is h3 too");
    let saw = resumed.text().await.unwrap();
    assert!(saw == "0" || saw == "1", "the server reports whether the request was early data: {saw}");
    eprintln!("server saw the resumed GET as early data: {}", saw == "1");
    server.shutdown().await;
}
