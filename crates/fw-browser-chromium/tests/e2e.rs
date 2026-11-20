//! Drives the real engine against a local HTTP server. CEF starts once per process, so one test runs every case.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::time::Duration;

use browser_chromium::{Backend, Chromium, Config, Error, EvaluateOptions, Page, ScreenshotOptions};

const PAGE: &str = "<!doctype html><title>Example Domain</title><body style=\"margin:0;background:#00ff00\">\
<script>window.answer = 6 * 7; window.later = new Promise(r => setTimeout(() => r('done'), 50));</script></body>";

fn serve() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let mut buf = [0u8; 2048];
            let _ = stream.read(&mut buf);
            let head = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/html\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                PAGE.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(PAGE.as_bytes());
        }
    });
    url
}

fn unused_port_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    format!("http://{}/", listener.local_addr().unwrap())
}

#[test]
fn screenshot_and_evaluate_a_real_page() {
    let chromium = Chromium::start(Config {
        helper: Some(PathBuf::from(env!("CARGO_BIN_EXE_browser-chromium"))),
        ..Config::default()
    })
    .unwrap();
    let url = serve();
    let eval = EvaluateOptions::default();

    assert_eq!(
        chromium.evaluate(&url, "document.title", &eval).unwrap(),
        serde_json::json!("Example Domain")
    );
    assert_eq!(
        chromium.evaluate(&url, "window.answer", &eval).unwrap(),
        serde_json::json!(42)
    );
    assert_eq!(
        chromium.evaluate(&url, "window.later", &eval).unwrap(),
        serde_json::json!("done")
    );
    assert_eq!(
        chromium
            .evaluate(&url, "({a: [1, 2], b: null})", &eval)
            .unwrap(),
        serde_json::json!({"a": [1, 2], "b": null})
    );
    assert!(
        matches!(chromium.evaluate(&url, "nope.nope", &eval), Err(Error::Script(m)) if m.contains("ReferenceError"))
    );

    let out = std::env::temp_dir().join(format!("browser-chromium-e2e-{}.png", std::process::id()));
    let options = ScreenshotOptions {
        width: 320,
        height: 200,
        ..ScreenshotOptions::default()
    };
    let shot = chromium.screenshot(&url, &out, &options).unwrap();
    assert_eq!((shot.width, shot.height), (320, 200));
    let decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&out).unwrap()));
    let mut reader = decoder.read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let frame = reader.next_frame(&mut pixels).unwrap();
    assert_eq!((frame.width, frame.height), (320, 200));
    assert_eq!(
        &pixels[..4],
        &[0, 255, 0, 255],
        "the page's green background is the first pixel"
    );
    std::fs::remove_file(&out).unwrap();

    // One page, several operations: state persists, and the viewport follows each screenshot.
    let page = Backend::open(&chromium, &url).unwrap();
    page.evaluate("window.counter = 1", &eval).unwrap();
    assert_eq!(
        page.evaluate("window.counter + 1", &eval).unwrap(),
        serde_json::json!(2)
    );
    let small = ScreenshotOptions {
        width: 200,
        height: 100,
        settle: Duration::ZERO,
        ..options.clone()
    };
    let large = ScreenshotOptions {
        width: 400,
        height: 300,
        settle: Duration::ZERO,
        ..options.clone()
    };
    assert_eq!(page.screenshot(&out, &small).unwrap().width, 200);
    assert_eq!(page.screenshot(&out, &large).unwrap().height, 300);
    assert_eq!(page.screenshot(&out, &small).unwrap().height, 100);
    assert_eq!(
        page.evaluate("window.innerWidth", &eval).unwrap(),
        serde_json::json!(200)
    );
    std::fs::remove_file(&out).unwrap();
    drop(page);

    let failed = chromium.evaluate(
        &unused_port_url(),
        "1",
        &EvaluateOptions {
            timeout: Duration::from_secs(15),
            ..eval
        },
    );
    assert!(
        matches!(failed, Err(Error::Load(_))),
        "connection refused must surface as a load error, got {failed:?}"
    );
    chromium.shutdown();
}
