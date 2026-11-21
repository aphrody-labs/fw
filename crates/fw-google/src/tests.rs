//! Offline tests: every service runs against recorded-shape JSON fixtures through an in-memory
//! transport. They check the request each operation builds (method, path, query, auth header, body)
//! and the typed parsing of the response.

use crate::drive::DriveListQuery;
use crate::mock::{Mock, Recorded, Reply};
use crate::youtube::{ChannelSelector, PlaylistSelector, VideoPatch};
use crate::{Credentials, DraftSpec, Error, EventsQuery, Google, WriteEnabled, docs_plain_text};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap_or_else(|e| panic!("fixture {name}: {e}"))
}

fn client(mock: &Mock) -> Google<Mock> {
    Google::with_connector(mock.clone(), Credentials::token("test-token"))
}

fn only(mock: &Mock) -> Recorded {
    let r = mock.requests();
    assert_eq!(
        r.len(),
        1,
        "expected one request, got {:?}",
        r.iter()
            .map(|r| format!("{} {}", r.method, r.path))
            .collect::<Vec<_>>()
    );
    r.into_iter().next().unwrap()
}

#[tokio::test]
async fn drive_list_builds_the_query_and_parses_files() {
    let mock = Mock::new(|_| Reply::ok(&fixture("drive_files.json")));
    let q = DriveListQuery {
        q: Some("name contains 'report' and trashed = false".into()),
        page_size: Some(5),
        page_token: Some("PT".into()),
        order_by: Some("modifiedTime desc".into()),
    };
    let list = client(&mock).drive_list(&q).await.unwrap();
    let r = only(&mock);
    assert_eq!(
        (r.method.as_str(), r.path.as_str()),
        ("GET", "/drive/v3/files")
    );
    assert_eq!(
        r.param("q"),
        Some("name contains 'report' and trashed = false")
    );
    assert_eq!(r.param("pageSize"), Some("5"));
    assert_eq!(r.param("pageToken"), Some("PT"));
    assert_eq!(r.param("orderBy"), Some("modifiedTime desc"));
    assert!(r.param("fields").unwrap().contains("files(id,name"));
    assert_eq!(r.param("supportsAllDrives"), Some("true"));
    assert_eq!(r.bearer(), Some("test-token"));
    let files = list.files.unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].name.as_deref(), Some("Quarterly report"));
    assert_eq!(files[1].size, Some(48213));
    assert_eq!(list.next_page_token.as_deref(), Some("NEXT123"));
}

#[tokio::test]
async fn drive_export_requests_the_mime_type_and_returns_the_bytes() {
    let mock = Mock::new(|_| Reply::ok("exported,csv\n1,2\n"));
    let bytes = client(&mock)
        .drive_export("FILE1", "text/csv")
        .await
        .unwrap();
    let r = only(&mock);
    assert_eq!(
        (r.method.as_str(), r.path.as_str()),
        ("GET", "/drive/v3/files/FILE1/export")
    );
    assert_eq!(r.param("mimeType"), Some("text/csv"));
    assert_eq!(bytes, b"exported,csv\n1,2\n");
}

#[tokio::test]
async fn sheets_get_reads_a_range() {
    let mock = Mock::new(|_| Reply::ok(&fixture("sheets_values.json")));
    let v = client(&mock)
        .sheets_values_get("SID", "Sheet1!A1:C2")
        .await
        .unwrap();
    let r = only(&mock);
    assert_eq!(
        (r.method.as_str(), r.path.as_str()),
        ("GET", "/v4/spreadsheets/SID/values/Sheet1!A1:C2")
    );
    assert_eq!(r.bearer(), Some("test-token"));
    let rows = v.values.unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1][0], serde_json::json!("apple"));
}

#[tokio::test]
async fn sheets_update_puts_the_values_with_the_input_option() {
    let mock = Mock::new(|_| Reply::ok(&fixture("sheets_update.json")));
    let values = vec![
        vec![serde_json::json!("x"), serde_json::json!(1)],
        vec![serde_json::json!("=A1"), serde_json::json!(2.5)],
    ];
    let resp = client(&mock)
        .sheets_values_update("SID", "Sheet1!A1:B2", values.clone(), false)
        .await
        .unwrap();
    let r = only(&mock);
    assert_eq!(
        (r.method.as_str(), r.path.as_str()),
        ("PUT", "/v4/spreadsheets/SID/values/Sheet1!A1:B2")
    );
    assert_eq!(r.param("valueInputOption"), Some("USER_ENTERED"));
    let body = r.json();
    assert_eq!(body["range"], "Sheet1!A1:B2");
    assert_eq!(body["majorDimension"], "ROWS");
    assert_eq!(body["values"], serde_json::json!(values));
    assert_eq!(resp.updated_cells, Some(4));

    let mock = Mock::new(|_| Reply::ok(&fixture("sheets_update.json")));
    client(&mock)
        .sheets_values_update("SID", "A1", vec![], true)
        .await
        .unwrap();
    assert_eq!(only(&mock).param("valueInputOption"), Some("RAW"));
}

#[tokio::test]
async fn gmail_list_fetches_metadata_for_each_message() {
    let mock = Mock::new(|r| match r.path.as_str() {
        "/gmail/v1/users/me/messages" => Reply::ok(&fixture("gmail_list.json")),
        "/gmail/v1/users/me/messages/18f0a1" => Reply::ok(&fixture("gmail_msg_18f0a1.json")),
        "/gmail/v1/users/me/messages/18f0a2" => Reply::ok(&fixture("gmail_msg_18f0a2.json")),
        other => Reply::status(404, &format!("{{\"unexpected\":\"{other}\"}}")),
    });
    let rows = client(&mock)
        .gmail_list(Some("is:unread"), 2)
        .await
        .unwrap();
    let reqs = mock.requests();
    assert_eq!(reqs.len(), 3);
    assert_eq!(reqs[0].param("q"), Some("is:unread"));
    assert_eq!(reqs[0].param("maxResults"), Some("2"));
    assert_eq!(reqs[1].param("format"), Some("metadata"));
    assert_eq!(
        reqs[1].params("metadataHeaders"),
        ["Subject", "From", "Date"]
    );
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].subject.as_deref(), Some("Re: planning"));
    assert_eq!(rows[0].from.as_deref(), Some("Ada <ada@example.com>"));
    assert_eq!(rows[0].snippet.as_deref(), Some("Meeting moved to 15:00"));
    // header names match case-insensitively; missing headers stay None
    assert_eq!(rows[1].from.as_deref(), Some("bot@example.com"));
    assert_eq!(rows[1].subject, None);
}

#[tokio::test]
async fn gmail_draft_uploads_a_message_and_never_sends() {
    let mock = Mock::new(|_| Reply::ok(&fixture("gmail_draft.json")));
    let spec = DraftSpec {
        to: vec!["to@example.com".into()],
        cc: vec!["cc@example.com".into()],
        subject: "Hello".into(),
        body: "Body line".into(),
    };
    let draft = client(&mock).gmail_draft_create(&spec).await.unwrap();
    let reqs = mock.requests();
    assert_eq!(reqs.len(), 1);
    let r = &reqs[0];
    assert_eq!(r.method, "POST");
    assert_eq!(r.path, "/upload/gmail/v1/users/me/drafts");
    assert_eq!(r.param("uploadType"), Some("multipart"));
    let body = String::from_utf8_lossy(&r.body).to_string();
    assert!(body.contains("message/rfc822"), "{body}");
    assert!(body.contains("To: to@example.com"), "{body}");
    assert!(body.contains("Cc: cc@example.com"), "{body}");
    assert!(body.contains("Subject: Hello"), "{body}");
    assert!(
        reqs.iter().all(|r| !r.path.contains("send")),
        "a draft must never hit a send endpoint"
    );
    assert_eq!(draft.id.as_deref(), Some("r-123456789"));
}

#[test]
fn rfc822_draft_rejects_header_injection_and_encodes_utf8() {
    use crate::rfc822_draft;
    let ok = DraftSpec {
        to: vec!["a@example.com".into()],
        subject: "Café".into(),
        body: "été".into(),
        ..Default::default()
    };
    let text = String::from_utf8(rfc822_draft(&ok).unwrap()).unwrap();
    assert!(text.contains("Subject: =?UTF-8?B?Q2Fmw6k=?=\r\n"), "{text}");
    assert!(text.contains("Content-Transfer-Encoding: base64"));
    assert!(text.ends_with("w6l0w6k=\r\n"), "{text}");

    let bad_subject = DraftSpec {
        subject: "x\r\nBcc: evil@example.com".into(),
        ..ok.clone()
    };
    assert!(matches!(rfc822_draft(&bad_subject), Err(Error::Invalid(_))));
    let bad_to = DraftSpec {
        to: vec!["a@example.com\nBcc: e@example.com".into()],
        ..ok.clone()
    };
    assert!(matches!(rfc822_draft(&bad_to), Err(Error::Invalid(_))));
    let no_to = DraftSpec {
        to: vec![],
        ..ok.clone()
    };
    assert!(matches!(rfc822_draft(&no_to), Err(Error::Invalid(_))));
    let not_mail = DraftSpec {
        to: vec!["nope".into()],
        ..ok
    };
    assert!(matches!(rfc822_draft(&not_mail), Err(Error::Invalid(_))));
}

#[tokio::test]
async fn calendar_events_are_expanded_and_ordered() {
    let mock = Mock::new(|_| Reply::ok(&fixture("calendar_events.json")));
    let t = chrono::DateTime::parse_from_rfc3339("2026-11-21T00:00:00Z")
        .unwrap()
        .to_utc();
    let q = EventsQuery {
        calendar_id: "team@example.com".into(),
        time_min: Some(t),
        max_results: 3,
    };
    let events = client(&mock).calendar_events(&q).await.unwrap();
    let r = only(&mock);
    assert_eq!(
        (r.method.as_str(), r.path.as_str()),
        ("GET", "/calendar/v3/calendars/team@example.com/events")
    );
    assert_eq!(r.param("singleEvents"), Some("true"));
    assert_eq!(r.param("orderBy"), Some("startTime"));
    assert_eq!(r.param("maxResults"), Some("3"));
    assert_eq!(r.param("timeMin"), Some("2026-11-21T00:00:00.000Z"));
    let items = events.items.unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].summary.as_deref(), Some("Standup"));
    assert!(items[1].start.as_ref().unwrap().date.is_some());
}

#[tokio::test]
async fn docs_text_flattens_paragraphs_and_tables() {
    let mock = Mock::new(|_| Reply::ok(&fixture("docs_document.json")));
    let text = client(&mock).docs_text("DID").await.unwrap();
    let r = only(&mock);
    assert_eq!(
        (r.method.as_str(), r.path.as_str()),
        ("GET", "/v1/documents/DID")
    );
    assert_eq!(text, "Title\nHello world\na1\tb1\na2\tb2\nDone\n");
    let doc: crate::docs_api::api::Document =
        serde_json::from_str(&fixture("docs_document.json")).unwrap();
    assert_eq!(docs_plain_text(&doc), text);
    assert_eq!(docs_plain_text(&Default::default()), "");
}

#[tokio::test]
async fn youtube_lists_build_their_filters() {
    let mock = Mock::new(|_| Reply::ok(&fixture("youtube_channels.json")));
    let ch = client(&mock)
        .youtube_channels(&ChannelSelector::Mine)
        .await
        .unwrap();
    let r = only(&mock);
    assert_eq!(
        (r.method.as_str(), r.path.as_str()),
        ("GET", "/youtube/v3/channels")
    );
    assert_eq!(r.param("mine"), Some("true"));
    assert_eq!(
        r.params("part"),
        ["snippet", "contentDetails", "statistics"]
    );
    let c = &ch.items.unwrap()[0];
    assert_eq!(
        c.snippet.as_ref().unwrap().title.as_deref(),
        Some("Example channel")
    );
    assert_eq!(
        c.content_details
            .as_ref()
            .unwrap()
            .related_playlists
            .as_ref()
            .unwrap()
            .uploads
            .as_deref(),
        Some("UUxxxxxxxxxxxxxxxxxxxxxx")
    );

    let mock = Mock::new(|_| Reply::ok(&fixture("youtube_channels.json")));
    client(&mock)
        .youtube_channels(&ChannelSelector::Ids(vec!["UC1".into(), "UC2".into()]))
        .await
        .unwrap();
    assert_eq!(only(&mock).params("id"), ["UC1", "UC2"]);

    let mock = Mock::new(|_| Reply::ok(&fixture("youtube_video.json")));
    let v = client(&mock)
        .youtube_videos(&["dQw4w9WgXcQ".into()])
        .await
        .unwrap();
    let r = only(&mock);
    assert_eq!(r.path, "/youtube/v3/videos");
    assert_eq!(r.params("id"), ["dQw4w9WgXcQ"]);
    assert_eq!(
        v.items.unwrap()[0]
            .snippet
            .as_ref()
            .unwrap()
            .title
            .as_deref(),
        Some("Old title")
    );
    assert!(matches!(
        client(&mock).youtube_videos(&[]).await,
        Err(Error::Invalid(_))
    ));

    let mock = Mock::new(|_| Reply::ok(&fixture("youtube_playlists.json")));
    let p = client(&mock)
        .youtube_playlists(&PlaylistSelector::Channel("UC9".into()), 7)
        .await
        .unwrap();
    let r = only(&mock);
    assert_eq!(r.path, "/youtube/v3/playlists");
    assert_eq!(
        (r.param("channelId"), r.param("maxResults")),
        (Some("UC9"), Some("7"))
    );
    assert_eq!(
        p.items.unwrap()[0]
            .content_details
            .as_ref()
            .unwrap()
            .item_count,
        Some(12)
    );

    let mock = Mock::new(|_| Reply::ok("{\"items\":[]}"));
    client(&mock)
        .youtube_playlist_items("PL1", 3)
        .await
        .unwrap();
    let r = only(&mock);
    assert_eq!(
        (r.path.as_str(), r.param("playlistId")),
        ("/youtube/v3/playlistItems", Some("PL1"))
    );
}

#[tokio::test]
async fn youtube_update_needs_the_flag_and_sends_only_writable_fields() {
    assert!(matches!(
        WriteEnabled::from_flag(false, "youtube videos.update"),
        Err(Error::WriteNotEnabled(_))
    ));
    let write = WriteEnabled::from_flag(true, "youtube videos.update").unwrap();

    let mock = Mock::new(|r| match r.method.as_str() {
        "GET" => Reply::ok(&fixture("youtube_video.json")),
        _ => Reply::ok(&fixture("youtube_video_updated.json")),
    });
    let patch = VideoPatch {
        title: Some("New title".into()),
        ..Default::default()
    };
    let updated = client(&mock)
        .youtube_video_update(write, "dQw4w9WgXcQ", &patch)
        .await
        .unwrap();
    let reqs = mock.requests();
    assert_eq!(reqs.len(), 2);
    assert_eq!(
        (reqs[0].method.as_str(), reqs[0].path.as_str()),
        ("GET", "/youtube/v3/videos")
    );
    let put = &reqs[1];
    assert_eq!(
        (put.method.as_str(), put.path.as_str()),
        ("PUT", "/youtube/v3/videos")
    );
    assert_eq!(put.params("part"), ["id,snippet"]);
    let body = put.json();
    assert_eq!(body["id"], "dQw4w9WgXcQ");
    assert_eq!(body["snippet"]["title"], "New title");
    // untouched fields are carried over, read-only ones are not sent
    assert_eq!(body["snippet"]["description"], "Old description");
    assert_eq!(body["snippet"]["categoryId"], "28");
    assert_eq!(body["snippet"]["tags"], serde_json::json!(["rust", "demo"]));
    assert!(
        body["snippet"].get("thumbnails").is_none() && body["snippet"].get("publishedAt").is_none()
    );
    assert_eq!(updated.snippet.unwrap().title.as_deref(), Some("New title"));

    // an empty patch is refused before any request
    let mock = Mock::new(|_| Reply::ok("{}"));
    assert!(matches!(
        client(&mock)
            .youtube_video_update(write, "x", &VideoPatch::default())
            .await,
        Err(Error::Invalid(_))
    ));
    assert!(mock.requests().is_empty());
}

#[test]
fn apply_patch_overrides_only_given_fields() {
    let current: crate::youtube_api::api::VideoSnippet = serde_json::from_value(serde_json::json!({"title": "t", "description": "d", "tags": ["a"], "categoryId": "10", "defaultLanguage": "fr"})).unwrap();
    let patch = VideoPatch {
        description: Some("new".into()),
        tags: Some(vec![]),
        ..Default::default()
    };
    let s = crate::apply_patch(&current, &patch);
    assert_eq!(
        (
            s.title.as_deref(),
            s.description.as_deref(),
            s.category_id.as_deref(),
            s.default_language.as_deref()
        ),
        (Some("t"), Some("new"), Some("10"), Some("fr"))
    );
    assert_eq!(s.tags, Some(vec![]));
}

#[tokio::test]
async fn api_errors_carry_status_and_body_but_never_the_token() {
    let mock = Mock::new(|_| Reply::status(403, &fixture("error_403.json")));
    let err = client(&mock)
        .drive_list(&DriveListQuery::default())
        .await
        .unwrap_err();
    let shown = format!("{err} / {err:?}");
    match &err {
        Error::Http {
            service,
            status,
            body,
        } => {
            assert_eq!((*service, *status), ("drive", 403));
            assert!(
                body.contains("insufficient authentication scopes"),
                "{body}"
            );
        }
        other => panic!("expected Http error, got {other:?}"),
    }
    assert!(!shown.contains("test-token"), "{shown}");
}

#[tokio::test]
async fn unusable_credentials_become_an_auth_error_and_nothing_is_sent() {
    let mock = Mock::new(|_| Reply::ok("{}"));
    let g = Google::with_connector(
        mock.clone(),
        Credentials::adc_file("/nonexistent/fw-google/adc.json"),
    );
    let err = g.drive_list(&DriveListQuery::default()).await.unwrap_err();
    assert!(matches!(err, Error::Auth(_)), "{err:?}");
    assert!(mock.requests().is_empty());
}
