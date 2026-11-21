//! Drives the real engine against a local HTTP server. CEF starts once per process, so one test runs every case.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::time::Duration;

use browser_chromium::{
    Backend, CaptureOptions, Chromium, Config, Error, EvaluateOptions, Page, ScreenshotOptions,
};

const PAGE: &str = "<!doctype html><title>Example Domain</title><body style=\"margin:0;background:#00ff00\">\
<script>window.answer = 6 * 7; window.later = new Promise(r => setTimeout(() => r('done'), 50));</script></body>";

/// Gemini-shaped and article-shaped pages whose content is rendered by script after load: custom elements for the
/// turns; doubled captions, a credit line and a related-cards feed that must not count as content.
const CHAT: &str = "<!doctype html><title>Shared chat</title><html lang=\"en\"><body><main id=\"m\"></main><script>\
setTimeout(() => { document.getElementById('m').innerHTML = \
'<share-turn-viewer><user-query><span class=\"cdk-visually-hidden\">You said</span><p>What is 6 x 7?</p></user-query>\
<response-container><message-content><p>It is 42.</p><p>Always.</p></message-content></response-container></share-turn-viewer>\
<a href=\"https://example.com/x\">Example</a>'; }, 300);\
</script></body></html>";

const ARTICLE: &str = "<!doctype html><title>Article</title><body><main id=\"m\"></main><script>\
setTimeout(() => { document.getElementById('m').innerHTML = \
'<h1>Heading</h1><p>Body paragraph.</p><figcaption>Caption.</figcaption><figcaption>Caption.</figcaption>\
<p>Source Somebody</p><div class=\"Related_feed\"><h2>Related</h2><p>Card text</p></div>'; }, 300);\
</script></body></html>";

fn serve() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let mut buf = [0u8; 2048];
            let read = stream.read(&mut buf).unwrap_or(0);
            let request = &buf[..read];
            let page = if request.starts_with(b"GET /chat") {
                CHAT
            } else if request.starts_with(b"GET /article") {
                ARTICLE
            } else {
                PAGE
            };
            let head = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/html\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                page.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(page.as_bytes());
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

    // Capture waits for the script-rendered content, then extracts turns, content and links.
    let capture = chromium
        .capture(
            &format!("{url}chat"),
            &CaptureOptions {
                quiet: Duration::from_millis(600),
                screenshot: true,
                ..CaptureOptions::default()
            },
        )
        .unwrap();
    assert_eq!(capture.title, "Shared chat");
    assert_eq!(capture.lang, "en");
    assert_eq!(capture.settled_by, "stable");
    let turns: Vec<_> = capture
        .turns
        .iter()
        .map(|t| (t.role.as_str(), t.text.as_str()))
        .collect();
    assert_eq!(
        turns,
        [
            ("user", "What is 6 x 7?"),
            ("model", "It is 42.\n\nAlways.")
        ]
    );
    assert!(
        capture
            .links
            .iter()
            .any(|l| l.href == "https://example.com/x" && l.text == "Example")
    );
    assert!(
        capture
            .screenshot_png
            .as_deref()
            .is_some_and(|p| p.starts_with(b"\x89PNG"))
    );
    let article = chromium
        .capture(&format!("{url}article"), &CaptureOptions::default())
        .unwrap();
    assert_eq!(article.text, "Heading\n\nBody paragraph.\n\nCaption.");
    assert!(article.turns.is_empty());

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
