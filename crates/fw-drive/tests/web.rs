use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use fw_drive::web::{App, Auth, Verified, parse_range, router};
use fw_drive::{Drive, User};
use serde_json::Value;
use std::sync::Arc;
use tower::ServiceExt;

fn app(auth: Auth) -> (tempfile::TempDir, axum::Router) {
    let dir = tempfile::tempdir().unwrap();
    let drive = Drive::open(dir.path(), None).unwrap();
    (dir, router(Arc::new(App { drive, auth })))
}

fn owner() -> Auth {
    Auth::Owner(User::new("me@example.com").unwrap())
}

async fn call(
    r: &axum::Router,
    req: Request<Body>,
) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let res = r.clone().oneshot(req).await.unwrap();
    let (parts, body) = res.into_parts();
    (
        parts.status,
        parts.headers,
        to_bytes(body, usize::MAX).await.unwrap().to_vec(),
    )
}

fn json(b: &[u8]) -> Value {
    serde_json::from_slice(b).unwrap()
}

#[tokio::test]
async fn upload_list_download_with_range_and_etag() {
    let (_d, r) = app(owner());
    let (s, _, b) = call(
        &r,
        Request::post("/api/v1/files?name=hello.txt")
            .body(Body::from("hello world"))
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    let file = json(&b)["file"].clone();
    assert_eq!(file["mime"], "text/plain");
    let id = file["id"].as_str().unwrap().to_owned();

    let (_, _, b) = call(
        &r,
        Request::get("/api/v1/files").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(json(&b)["files"][0]["name"], "hello.txt");

    let (s, h, b) = call(
        &r,
        Request::get(format!("/api/v1/files/{id}/content"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!((s, b.as_slice()), (StatusCode::OK, &b"hello world"[..]));
    assert!(
        h[header::CONTENT_SECURITY_POLICY]
            .to_str()
            .unwrap()
            .starts_with("sandbox")
    );
    let etag = h[header::ETAG].to_str().unwrap().to_owned();

    let (s, h, b) = call(
        &r,
        Request::get(format!("/files/{id}"))
            .header(header::RANGE, "bytes=6-")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(
        (s, b.as_slice()),
        (StatusCode::PARTIAL_CONTENT, &b"world"[..])
    );
    assert_eq!(h[header::CONTENT_RANGE], "bytes 6-10/11");

    let (s, ..) = call(
        &r,
        Request::get(format!("/files/{id}"))
            .header(header::IF_NONE_MATCH, &etag)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_MODIFIED);
    let (s, ..) = call(
        &r,
        Request::get(format!("/files/{id}"))
            .header(header::RANGE, "bytes=99-")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::RANGE_NOT_SATISFIABLE);
}

#[tokio::test]
async fn pages_render_and_forms_act() {
    let (_d, r) = app(owner());
    let (s, h, _) = call(&r, Request::get("/").body(Body::empty()).unwrap()).await;
    assert!(s.is_redirection(), "{s}");
    let folder = h[header::LOCATION].to_str().unwrap().to_owned();
    let id = folder.trim_start_matches("/folders/").to_owned();

    let form = |path: String, body: &str| {
        Request::post(path)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(body.to_owned()))
            .unwrap()
    };
    let (s, ..) = call(
        &r,
        form(format!("/folders/{id}/new-folder"), "name=Projects"),
    )
    .await;
    assert!(s.is_redirection());
    let (s, _, b) = call(&r, Request::get(&folder).body(Body::empty()).unwrap()).await;
    let html = String::from_utf8(b).unwrap();
    assert_eq!(s, StatusCode::OK);
    assert!(html.contains("Projects"));
    assert!(html.contains(r#"aria-current="page""#));
    assert!(html.contains("<h1>My Drive</h1>"));

    let (_, _, b) = call(
        &r,
        Request::get("/api/v1/files").body(Body::empty()).unwrap(),
    )
    .await;
    let projects = json(&b)["files"][0]["id"].as_str().unwrap().to_owned();
    let (s, h, _) = call(
        &r,
        form(
            format!("/files/{projects}/action"),
            "op=trash&back=%2Ftrash",
        ),
    )
    .await;
    assert_eq!(
        (s, h[header::LOCATION].to_str().unwrap()),
        (StatusCode::SEE_OTHER, "/trash")
    );
    let (_, _, b) = call(&r, Request::get("/trash").body(Body::empty()).unwrap()).await;
    assert!(String::from_utf8(b).unwrap().contains("Empty trash"));

    let (_, h, _) = call(
        &r,
        form(
            format!("/files/{projects}/action"),
            "op=restore&back=https%3A%2F%2Fevil.example",
        ),
    )
    .await;
    assert_eq!(h[header::LOCATION], "/", "off-site redirects are dropped");

    let (s, h, b) = call(&r, Request::get("/drive.css").body(Body::empty()).unwrap()).await;
    assert_eq!(s, StatusCode::OK);
    assert!(
        h[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("text/css")
    );
    let css = String::from_utf8(b).unwrap();
    assert!(
        css.contains("--md-sys-color-primary:"),
        "brand tokens come from app.css"
    );
}

#[tokio::test]
async fn cross_site_writes_are_refused() {
    let (_d, r) = app(owner());
    let req = Request::post("/api/v1/folders")
        .header(header::HOST, "drive.example.com")
        .header(header::ORIGIN, "https://evil.example")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"name":"x"}"#))
        .unwrap();
    assert_eq!(call(&r, req).await.0, StatusCode::FORBIDDEN);
    let same = Request::post("/api/v1/folders")
        .header(header::HOST, "drive.example.com")
        .header(header::ORIGIN, "https://drive.example.com")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"name":"x"}"#))
        .unwrap();
    assert_eq!(call(&r, same).await.0, StatusCode::CREATED);
}

#[tokio::test]
async fn access_identity_for_people_and_service_tokens() {
    let (_d, r) = app(Auth::Access);
    assert_eq!(
        call(&r, Request::get("/api/v1/me").body(Body::empty()).unwrap())
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let (s, _, b) = call(
        &r,
        Request::get("/api/v1/me")
            .header("cf-access-authenticated-user-email", "Person@Example.com")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(
        (s, json(&b)["user"].as_str()),
        (StatusCode::OK, Some("person@example.com"))
    );
    // header.payload.signature with payload {"common_name":"abc.access"}
    let jwt = "e30.eyJjb21tb25fbmFtZSI6ImFiYy5hY2Nlc3MifQ.sig";
    let (_, _, b) = call(
        &r,
        Request::get("/api/v1/me")
            .header("cf-access-jwt-assertion", jwt)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(json(&b)["user"], "abc.access@service.access");
}

#[tokio::test]
async fn sharing_over_the_api() {
    let (_d, r) = app(Auth::Access);
    let me = |req: axum::http::request::Builder| {
        req.header("cf-access-authenticated-user-email", "me@example.com")
    };
    let you = |req: axum::http::request::Builder| {
        req.header("cf-access-authenticated-user-email", "you@example.com")
    };
    let (_, _, b) = call(
        &r,
        me(Request::post("/api/v1/files?name=s.txt"))
            .body(Body::from("shared"))
            .unwrap(),
    )
    .await;
    let id = json(&b)["file"]["id"].as_str().unwrap().to_owned();
    assert_eq!(
        call(
            &r,
            you(Request::get(format!("/api/v1/files/{id}")))
                .body(Body::empty())
                .unwrap()
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let share = me(Request::put(format!("/api/v1/files/{id}/shares")))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"principal":"you@example.com","role":"reader"}"#,
        ))
        .unwrap();
    assert_eq!(call(&r, share).await.0, StatusCode::OK);
    let (s, _, b) = call(
        &r,
        you(Request::get(format!("/api/v1/files/{id}")))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(
        (s, json(&b)["role"].as_str()),
        (StatusCode::OK, Some("reader"))
    );
    let (_, _, b) = call(
        &r,
        you(Request::get("/api/v1/files?view=shared"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(json(&b)["files"].as_array().unwrap().len(), 1);
}

#[test]
fn ranges() {
    assert_eq!(parse_range("bytes=0-4", 10), Some((0, 4)));
    assert_eq!(parse_range("bytes=5-", 10), Some((5, 9)));
    assert_eq!(parse_range("bytes=-3", 10), Some((7, 9)));
    assert_eq!(parse_range("bytes=8-100", 10), Some((8, 9)));
    assert_eq!(parse_range("bytes=10-", 10), None);
    assert_eq!(parse_range("bytes=0-1,4-5", 10), None);
    assert_eq!(parse_range("items=0-1", 10), None);
}

#[tokio::test]
async fn verified_identity_comes_from_the_host_not_from_headers() {
    let (_d, r) = app(Auth::Verified);
    // Identity headers a client could forge mean nothing in this mode.
    let (s, ..) = call(
        &r,
        Request::get("/api/v1/me")
            .header("cf-access-authenticated-user-email", "mallory@example.com")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    let mut req = Request::get("/api/v1/me").body(Body::empty()).unwrap();
    req.extensions_mut()
        .insert(Verified(User::new("Me@Example.com").unwrap()));
    let (s, _, b) = call(&r, req).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(json(&b)["user"], "me@example.com");
}
