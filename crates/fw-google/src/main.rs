//! `fw-google [--access-token T] [--auth auto|gcloud|adc] <service> <op> [args]`, run by `fw google`.
//!
//! Prints JSON (plain text for `docs text`, raw bytes for `drive export`) on stdout, errors on stderr.
//! Credentials: `--access-token` or `FW_GOOGLE_ACCESS_TOKEN`, else `gcloud auth print-access-token`,
//! else Application Default Credentials. Tokens are never printed.

use fw_google::youtube::{ChannelSelector, PlaylistSelector, VideoPatch};
use fw_google::{
    Credentials, DraftSpec, DriveListQuery, EventsQuery, Google, Source, WriteEnabled,
};
use std::process::ExitCode;

const USAGE: &str = "usage: fw google [--access-token TOKEN] [--auth auto|gcloud|adc] <service> <op> [args]

  drive list [--query Q] [--page-size N] [--page-token T] [--order-by O]
  drive export <file-id> --mime MIME [-o FILE]
  sheets get <spreadsheet-id> <range>
  sheets update <spreadsheet-id> <range> --values JSON [--raw]      JSON is an array of rows
  gmail list [--query Q] [--max N]
  gmail draft --to A[,B] [--cc A] --subject S (--body TEXT | --body-file FILE)   creates a draft, never sends
  calendar events [--calendar ID] [--from now|RFC3339] [--max N]
  docs text <document-id>
  youtube channels (--mine | --id ID[,ID])
  youtube videos --id ID[,ID]
  youtube playlists (--mine | --channel ID | --id ID[,ID]) [--max N]
  youtube playlist-items <playlist-id> [--max N]
  youtube update-video <video-id> [--title T] [--description D] [--tags a,b] [--category-id N] --apply

Credentials: --access-token (or FW_GOOGLE_ACCESS_TOKEN), else `gcloud auth print-access-token`, else Application Default Credentials.
Writes: only `sheets update`, `gmail draft` (draft only) and `youtube update-video` (needs --apply).";

struct Args(Vec<String>);

impl Args {
    fn flag(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .position(|a| a == name)
            .and_then(|i| self.0.get(i + 1))
            .map(String::as_str)
    }
    fn has(&self, name: &str) -> bool {
        self.0.iter().any(|a| a == name)
    }
    fn num<T: std::str::FromStr>(&self, name: &str) -> Result<Option<T>, String> {
        self.flag(name)
            .map(|v| {
                v.parse::<T>()
                    .map_err(|_| format!("{name} expects a number, got `{v}`"))
            })
            .transpose()
    }
    /// Positional arguments: everything that is not a flag or a flag value.
    fn positional(&self, value_flags: &[&str]) -> Vec<&str> {
        let mut out = Vec::new();
        let mut skip = false;
        for a in &self.0 {
            if skip {
                skip = false;
            } else if value_flags.contains(&a.as_str()) {
                skip = true;
            } else if !a.starts_with('-') || a == "-" {
                out.push(a.as_str());
            }
        }
        out
    }
}

fn csv(v: &str) -> Vec<String> {
    v.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Drops `null` members: the generated types serialize every unset field as `null`.
fn prune_nulls(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::Object(m) => {
            m.retain(|_, x| !x.is_null());
            m.values_mut().for_each(prune_nulls);
        }
        serde_json::Value::Array(a) => a.iter_mut().for_each(prune_nulls),
        _ => {}
    }
}

fn json<T: serde::Serialize>(v: &T) -> Result<String, String> {
    let mut value = serde_json::to_value(v).map_err(|e| e.to_string())?;
    prune_nulls(&mut value);
    serde_json::to_string_pretty(&value).map_err(|e| e.to_string())
}

fn need<'a>(pos: &[&'a str], i: usize, what: &str) -> Result<&'a str, String> {
    pos.get(i)
        .copied()
        .ok_or_else(|| format!("missing <{what}>\n{USAGE}"))
}

async fn run(mut raw: Vec<String>) -> Result<Option<String>, String> {
    // global flags come first
    let mut token = std::env::var("FW_GOOGLE_ACCESS_TOKEN").ok();
    let mut source = Source::Auto;
    while let Some(first) = raw.first().cloned() {
        match first.as_str() {
            "--access-token" if raw.len() > 1 => {
                token = Some(raw[1].clone());
                raw.drain(..2);
            }
            "--auth" if raw.len() > 1 => {
                source = Source::parse(&raw[1])
                    .ok_or_else(|| format!("--auth: `{}` is not auto, gcloud or adc", raw[1]))?;
                raw.drain(..2);
            }
            _ => break,
        }
    }
    let [service, op, rest @ ..] = raw.as_slice() else {
        return Err(USAGE.to_string());
    };
    let a = Args(rest.to_vec());
    let g = Google::new(Credentials::with_source(token, source));
    let e = |e: fw_google::Error| e.to_string();
    match (service.as_str(), op.as_str()) {
        ("drive", "list") => {
            let q = DriveListQuery {
                q: a.flag("--query").map(Into::into),
                page_size: a.num("--page-size")?,
                page_token: a.flag("--page-token").map(Into::into),
                order_by: a.flag("--order-by").map(Into::into),
            };
            json(&g.drive_list(&q).await.map_err(e)?).map(Some)
        }
        ("drive", "export") => {
            let pos = a.positional(&["--mime", "-o"]);
            let id = need(&pos, 0, "file-id")?;
            let mime = a.flag("--mime").ok_or(
                "drive export needs --mime (for example text/plain, application/pdf, text/csv)",
            )?;
            let bytes = g.drive_export(id, mime).await.map_err(e)?;
            match a.flag("-o") {
                Some(path) => {
                    std::fs::write(path, &bytes).map_err(|err| format!("{path}: {err}"))?;
                    Ok(Some(format!(
                        "{{\"file\":{path:?},\"bytes\":{}}}",
                        bytes.len()
                    )))
                }
                None => {
                    use std::io::Write;
                    std::io::stdout()
                        .write_all(&bytes)
                        .map_err(|err| err.to_string())?;
                    Ok(None)
                }
            }
        }
        ("sheets", "get") => {
            let pos = a.positional(&[]);
            json(
                &g.sheets_values_get(need(&pos, 0, "spreadsheet-id")?, need(&pos, 1, "range")?)
                    .await
                    .map_err(e)?,
            )
            .map(Some)
        }
        ("sheets", "update") => {
            let pos = a.positional(&["--values"]);
            let raw_values = a
                .flag("--values")
                .ok_or("sheets update needs --values '[[\"a\",1],[\"b\",2]]'")?;
            let values: Vec<Vec<serde_json::Value>> = serde_json::from_str(raw_values)
                .map_err(|err| format!("--values must be a JSON array of rows: {err}"))?;
            json(
                &g.sheets_values_update(
                    need(&pos, 0, "spreadsheet-id")?,
                    need(&pos, 1, "range")?,
                    values,
                    a.has("--raw"),
                )
                .await
                .map_err(e)?,
            )
            .map(Some)
        }
        ("gmail", "list") => json(
            &g.gmail_list(a.flag("--query"), a.num("--max")?.unwrap_or(10))
                .await
                .map_err(e)?,
        )
        .map(Some),
        ("gmail", "draft") => {
            let body = match (a.flag("--body"), a.flag("--body-file")) {
                (Some(b), None) => b.to_string(),
                (None, Some(f)) => {
                    std::fs::read_to_string(f).map_err(|err| format!("{f}: {err}"))?
                }
                _ => {
                    return Err(
                        "gmail draft needs exactly one of --body or --body-file".to_string()
                    );
                }
            };
            let spec = DraftSpec {
                to: csv(a.flag("--to").ok_or("gmail draft needs --to")?),
                cc: a.flag("--cc").map(csv).unwrap_or_default(),
                subject: a.flag("--subject").unwrap_or_default().to_string(),
                body,
            };
            json(&g.gmail_draft_create(&spec).await.map_err(e)?).map(Some)
        }
        ("calendar", "events") => {
            let time_min = match a.flag("--from") {
                None => None,
                Some("now") => Some(chrono::Utc::now()),
                Some(t) => Some(
                    chrono::DateTime::parse_from_rfc3339(t)
                        .map_err(|err| format!("--from: `{t}` is not RFC 3339 ({err})"))?
                        .to_utc(),
                ),
            };
            let q = EventsQuery {
                calendar_id: a.flag("--calendar").unwrap_or("primary").to_string(),
                time_min,
                max_results: a.num("--max")?.unwrap_or(10),
            };
            json(&g.calendar_events(&q).await.map_err(e)?).map(Some)
        }
        ("docs", "text") => {
            let pos = a.positional(&[]);
            g.docs_text(need(&pos, 0, "document-id")?)
                .await
                .map(Some)
                .map_err(e)
        }
        ("youtube", "channels") => {
            let sel = match (a.has("--mine"), a.flag("--id")) {
                (true, None) => ChannelSelector::Mine,
                (false, Some(ids)) => ChannelSelector::Ids(csv(ids)),
                _ => return Err("youtube channels needs --mine or --id ID[,ID]".to_string()),
            };
            json(&g.youtube_channels(&sel).await.map_err(e)?).map(Some)
        }
        ("youtube", "videos") => json(
            &g.youtube_videos(&csv(a
                .flag("--id")
                .ok_or("youtube videos needs --id ID[,ID]")?))
                .await
                .map_err(e)?,
        )
        .map(Some),
        ("youtube", "playlists") => {
            let sel =
                match (a.has("--mine"), a.flag("--channel"), a.flag("--id")) {
                    (true, None, None) => PlaylistSelector::Mine,
                    (false, Some(c), None) => PlaylistSelector::Channel(c.to_string()),
                    (false, None, Some(ids)) => PlaylistSelector::Ids(csv(ids)),
                    _ => return Err(
                        "youtube playlists needs exactly one of --mine, --channel ID, --id ID[,ID]"
                            .to_string(),
                    ),
                };
            json(
                &g.youtube_playlists(&sel, a.num("--max")?.unwrap_or(25))
                    .await
                    .map_err(e)?,
            )
            .map(Some)
        }
        ("youtube", "playlist-items") => {
            let pos = a.positional(&["--max"]);
            json(
                &g.youtube_playlist_items(
                    need(&pos, 0, "playlist-id")?,
                    a.num("--max")?.unwrap_or(25),
                )
                .await
                .map_err(e)?,
            )
            .map(Some)
        }
        ("youtube", "update-video") => {
            let pos = a.positional(&["--title", "--description", "--tags", "--category-id"]);
            let write =
                WriteEnabled::from_flag(a.has("--apply"), "youtube update-video").map_err(e)?;
            let patch = VideoPatch {
                title: a.flag("--title").map(Into::into),
                description: a.flag("--description").map(Into::into),
                tags: a.flag("--tags").map(csv),
                category_id: a.flag("--category-id").map(Into::into),
            };
            json(
                &g.youtube_video_update(write, need(&pos, 0, "video-id")?, &patch)
                    .await
                    .map_err(e)?,
            )
            .map(Some)
        }
        _ => Err(format!("unknown operation `{service} {op}`\n{USAGE}")),
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    match run(args).await {
        Ok(Some(out)) => {
            println!("{out}");
            ExitCode::SUCCESS
        }
        Ok(None) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("fw-google: {e}");
            ExitCode::from(if e.starts_with("usage:") { 2 } else { 1 })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_output_drops_nulls_but_keeps_falsy_values() {
        let v = serde_json::json!({"a": null, "b": [1, null, {"c": null, "d": false, "e": 0}], "f": ""});
        let out: serde_json::Value = serde_json::from_str(&json(&v).unwrap()).unwrap();
        assert_eq!(
            out,
            serde_json::json!({"b": [1, null, {"d": false, "e": 0}], "f": ""})
        );
    }

    #[test]
    fn positional_skips_flags_and_their_values() {
        let a = Args(
            ["FILE", "--mime", "text/csv", "-o", "out.csv"]
                .map(String::from)
                .to_vec(),
        );
        assert_eq!(a.positional(&["--mime", "-o"]), ["FILE"]);
        assert_eq!(a.flag("--mime"), Some("text/csv"));
        assert!(!a.has("--apply"));
        assert_eq!(csv("a, b,,c"), ["a", "b", "c"]);
    }

    #[tokio::test]
    async fn youtube_update_without_apply_is_refused_before_any_network_use() {
        let err = run([
            "--access-token",
            "t",
            "youtube",
            "update-video",
            "VID",
            "--title",
            "x",
        ]
        .map(String::from)
        .to_vec())
        .await
        .unwrap_err();
        assert!(err.contains("--apply"), "{err}");
    }

    #[tokio::test]
    async fn unknown_operation_prints_usage() {
        let err = run(["drive", "nope"].map(String::from).to_vec())
            .await
            .unwrap_err();
        assert!(
            err.contains("unknown operation") && err.contains("usage:"),
            "{err}"
        );
    }
}
