//! Offline end-to-end test: a tiny crates.io dump and advisory-db built in memory go through `sync`, the published
//! snapshot is read back, and the queries, the stack generator and the CLI run against it.

#![cfg(feature = "sync")]

use flate2::{Compression, write::GzEncoder};
use fw_crates::sync::{SyncOptions, sync};
use fw_crates::{query, stack, store};
use serde_json::Value;
use std::path::Path;

fn quote(v: &str) -> String {
    if v.contains([',', '"', '\n']) { format!("\"{}\"", v.replace('"', "\"\"")) } else { v.to_owned() }
}

/// A CSV with the dump's own header and the given rows; unspecified columns are empty.
fn csv(header: &str, rows: &[Vec<(&str, &str)>]) -> String {
    let cols: Vec<&str> = header.split(',').collect();
    let mut out = format!("{header}\n");
    for r in rows {
        let line: Vec<String> = cols.iter().map(|c| quote(r.iter().find(|(k, _)| k == c).map_or("", |(_, v)| v))).collect();
        out.push_str(&line.join(","));
        out.push('\n');
    }
    out
}

fn tar_gz(files: &[(String, String)]) -> Vec<u8> {
    let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::fast()));
    for (path, content) in files {
        let mut h = tar::Header::new_gnu();
        h.set_size(content.len() as u64);
        h.set_mode(0o600);
        h.set_cksum();
        builder.append_data(&mut h, path, content.as_bytes()).unwrap();
    }
    builder.into_inner().unwrap().finish().unwrap()
}

const CRATES_H: &str = "created_at,description,documentation,homepage,id,max_features,max_upload_size,name,readme,repository,trustpub_only,updated_at";
const VERSIONS_H: &str = "bin_names,categories,crate_id,crate_size,created_at,description,documentation,downloads,edition,features,has_lib,homepage,id,keywords,license,links,num,num_no_build,published_by,repository,rust_version,tar_sha256,updated_at,yanked,zip_json_sha256,zip_sha256";

fn dump() -> Vec<u8> {
    let d = "2026-11-21-020022/";
    let crate_rows: Vec<Vec<(&str, &str)>> = vec![
        vec![("id", "1"), ("name", "tokio"), ("description", "An event-driven, non-blocking I/O platform"), ("repository", "https://github.com/tokio-rs/tokio"), ("created_at", "2016-07-01 00:00:00+00"), ("readme", "# tokio\nmulti-line, \"quoted\" readme")],
        vec![("id", "2"), ("name", "axum"), ("description", "Web framework that focuses on ergonomics"), ("repository", "https://github.com/tokio-rs/axum"), ("created_at", "2019-11-01 00:00:00+00")],
        vec![("id", "3"), ("name", "poem"), ("description", "Another web framework"), ("repository", "https://github.com/poem-web/poem"), ("created_at", "2020-06-01 00:00:00+00")],
        vec![("id", "4"), ("name", "actix-web"), ("description", "Actix Web is a powerful web framework"), ("repository", "https://github.com/actix/actix-web"), ("created_at", "2017-10-01 00:00:00+00")],
        vec![("id", "5"), ("name", "serde"), ("description", "A serialization framework"), ("repository", "https://github.com/serde-rs/serde"), ("created_at", "2015-01-01 00:00:00+00")],
        vec![("id", "6"), ("name", "vuln"), ("description", "A web framework with a known advisory"), ("created_at", "2022-01-01 00:00:00+00")],
        vec![("id", "7"), ("name", "stale"), ("description", "No downloads at all"), ("created_at", "2019-01-01 00:00:00+00")],
        vec![("id", "8"), ("name", "preview"), ("description", "Only a pre-release"), ("created_at", "2026-10-01 00:00:00+00")],
    ];
    let v = |id: &str, krate: &str, num: &str, created: &str, license: &str, msrv: &str, yanked: &str| -> Vec<(String, String)> {
        vec![("id", id), ("crate_id", krate), ("num", num), ("created_at", created), ("license", license), ("rust_version", msrv), ("edition", "2021"), ("yanked", yanked)].into_iter().map(|(a, b)| (a.to_owned(), b.to_owned())).collect()
    };
    let version_rows = [
        v("10", "1", "1.40.0", "2026-06-01 00:00:00+00", "MIT", "1.70", "f"),
        v("11", "1", "1.41.0", "2026-10-01 00:00:00+00", "MIT", "1.70", "f"),
        v("12", "1", "1.42.0", "2026-11-10 00:00:00+00", "MIT", "1.71", "f"),
        v("13", "1", "1.43.0-alpha", "2026-11-15 00:00:00+00", "MIT", "1.71", "f"),
        v("20", "2", "0.8.0", "2026-10-01 00:00:00+00", "MIT", "1.75", "f"),
        v("21", "2", "0.8.1", "2026-11-12 00:00:00+00", "MIT", "1.75", "f"),
        v("22", "2", "0.9.0", "2026-11-13 00:00:00+00", "MIT", "1.80", "t"),
        v("30", "3", "3.0.0", "2026-08-01 00:00:00+00", "MIT OR Apache-2.0", "", "f"),
        v("40", "4", "4.0.0", "2025-01-01 00:00:00+00", "GPL-3.0-only", "1.60", "f"),
        v("50", "5", "1.0.0", "2026-11-01 00:00:00+00", "MIT OR Apache-2.0", "1.56", "f"),
        v("60", "6", "1.0.0", "2022-01-01 00:00:00+00", "MIT", "", "f"),
        v("70", "7", "0.1.0", "2019-01-01 00:00:00+00", "", "", "f"),
        v("80", "8", "0.1.0-rc.1", "2026-10-01 00:00:00+00", "MIT", "", "f"),
    ];
    let version_rows: Vec<Vec<(&str, &str)>> = version_rows.iter().map(|r| r.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect()).collect();
    let cat = |slug: &str, name: &str, id: &str| vec![("id", id.to_owned()), ("slug", slug.to_owned()), ("category", name.to_owned()), ("description", format!("{name} crates")), ("crates_cnt", "5".to_owned())];
    let cats: Vec<Vec<(&str, String)>> = vec![cat("web-programming::http-server", "Web programming::HTTP server", "100"), cat("asynchronous", "Asynchronous", "101")];
    let cats: Vec<Vec<(&str, &str)>> = cats.iter().map(|r| r.iter().map(|(a, b)| (*a, b.as_str())).collect()).collect();
    let dl = |vid: &str, date: &str, n: &str| vec![("version_id", vid.to_owned()), ("date", date.to_owned()), ("downloads", n.to_owned())].into_iter().collect::<Vec<_>>();
    let dls: Vec<Vec<(&str, String)>> = vec![
        dl("12", "2026-11-10", "900000"), dl("12", "2026-11-11", "100000"), dl("10", "2026-05-01", "777777"),
        dl("21", "2026-11-12", "50000"), dl("30", "2026-11-12", "4000"), dl("40", "2026-11-12", "9000"),
        dl("50", "2026-11-12", "800000"), dl("60", "2026-11-12", "1500"), dl("80", "2026-11-12", "3"),
    ];
    let dls: Vec<Vec<(&str, &str)>> = dls.iter().map(|r| r.iter().map(|(a, b)| (*a, b.as_str())).collect()).collect();

    let files = vec![
        (format!("{d}metadata.json"), "{\"timestamp\":\"2026-11-21T02:00:22.021216857Z\",\"crates_io_commit\":\"abc123\"}".to_owned()),
        (format!("{d}data/categories.csv"), csv("category,crates_cnt,created_at,description,id,path,slug", &cats)),
        (format!("{d}data/crate_downloads.csv"), "crate_id,downloads\n1,5000000\n2,900000\n5,9000000\n".to_owned()),
        (format!("{d}data/crates.csv"), csv(CRATES_H, &crate_rows)),
        (format!("{d}data/keywords.csv"), "crates_cnt,created_at,id,keyword\n3,2020-01-01 00:00:00+00,900,web-framework\n".to_owned()),
        (format!("{d}data/crates_categories.csv"), "category_id,crate_id\n100,2\n100,3\n100,4\n100,6\n101,1\n".to_owned()),
        (format!("{d}data/crates_keywords.csv"), "crate_id,keyword_id\n2,900\n3,900\n4,900\n".to_owned()),
        (format!("{d}data/crate_owners.csv"), "crate_id,created_at,created_by,owner_id,owner_kind\n1,2016-07-01 00:00:00+00,1,1,0\n1,2016-07-01 00:00:00+00,1,2,0\n2,2019-11-01 00:00:00+00,1,1,0\n".to_owned()),
        (format!("{d}data/versions.csv"), csv(VERSIONS_H, &version_rows)),
        (format!("{d}data/default_versions.csv"), "crate_id,num_versions,version_id\n1,4,12\n2,3,21\n3,1,30\n4,1,40\n5,1,50\n6,1,60\n7,1,70\n8,1,80\n".to_owned()),
        (format!("{d}data/dependencies.csv"), "crate_id,default_features,explicit_name,features,id,kind,optional,req,target,version_id\n1,t,,{},1,0,f,1,,21\n5,t,,{},2,2,f,1,,21\n5,t,,{},3,0,f,1,,60\n1,t,,{},4,0,f,1,,10\n1,t,,{},5,0,f,1,,60\n1,t,,{},6,0,f,1,,60\n".to_owned()),
        (format!("{d}data/version_downloads.csv"), csv("date,downloads,version_id", &dls)),
    ];
    tar_gz(&files)
}

fn advisories() -> Vec<u8> {
    let md = "```toml\n[advisory]\nid = \"RUSTSEC-2099-0001\"\npackage = \"vuln\"\ndate = \"2099-01-01\"\n\n[versions]\npatched = [\">=2.0.0\"]\n```\n\n# vuln is broken\n";
    let info = "```toml\n[advisory]\nid = \"RUSTSEC-2099-0002\"\npackage = \"actix-web\"\ninformational = \"unmaintained\"\n```\n";
    tar_gz(&[("advisory-db-main/crates/vuln/RUSTSEC-2099-0001.md".to_owned(), md.to_owned()), ("advisory-db-main/crates/actix-web/RUSTSEC-2099-0002.md".to_owned(), info.to_owned()), ("advisory-db-main/README.md".to_owned(), "x".to_owned())])
}

fn write(dir: &Path, name: &str, bytes: &[u8]) -> String {
    let p = dir.join(name);
    std::fs::write(&p, bytes).unwrap();
    p.to_string_lossy().into_owned()
}

fn synced(tmp: &Path) -> fw_crates::sync::SyncReport {
    let dump = write(tmp, "db-dump.tar.gz", &dump());
    let adv = write(tmp, "advisory-db.tar.gz", &advisories());
    let opts = SyncOptions { dump, advisories: Some(adv), out: tmp.join("home"), publish: Some(tmp.join("docs")), index_top: 100 };
    sync(&opts, &mut |_| {}).unwrap()
}

#[test]
fn dump_becomes_ranked_dataset() {
    let tmp = tempfile::tempdir().unwrap();
    let r = synced(tmp.path());
    assert_eq!(r.meta.dump_date, "2026-11-21");
    assert_eq!(r.meta.crates_io_commit, "abc123");
    assert_eq!(r.meta.advisories, 2);
    // stale (no downloads, no dependents) is dropped; everything else is ranked.
    assert_eq!(r.meta.crates_ranked, 7);

    let ds = store::read_dir(&tmp.path().join("home")).unwrap();
    let tokio = ds.get("tokio").unwrap();
    assert_eq!(tokio.version, "1.42.0", "default version is the highest non-yanked stable release");
    assert_eq!(tokio.recent_downloads, 1_000_000, "the 2026-05-01 row is outside the 90 day window");
    assert_eq!(tokio.downloads, 5_000_000);
    assert_eq!(tokio.dependents, 2, "axum and vuln (which lists it twice) depend on tokio; dev dependencies and non-default versions do not count");
    assert_eq!(tokio.owners, 2);
    assert_eq!(tokio.releases_365d, 4);
    assert_eq!(tokio.last_release, "2026-11-15");
    assert_eq!(ds.get("axum").unwrap().version, "0.8.1", "a yanked newer release is not the default");
    assert_eq!(ds.get("preview").unwrap().version, "0.1.0-rc.1", "a crate with only a pre-release defaults to it");
    assert_eq!(ds.get("serde").unwrap().dependents, 1);
    assert_eq!(ds.get("axum_").map(|r| r.name.as_str()), None);
    assert_eq!(ds.get("actix_web").unwrap().name, "actix-web", "name lookup ignores case and - versus _");

    let vuln = ds.get("vuln").unwrap();
    assert_eq!(vuln.advisories.open, vec!["RUSTSEC-2099-0001"]);
    assert_eq!(vuln.components.security, 0.0);
    assert_eq!(ds.get("actix-web").unwrap().advisories.informational, vec!["RUSTSEC-2099-0002"]);
    assert!(ds.get("tokio").unwrap().score > ds.get("vuln").unwrap().score);
    assert!(ds.records.windows(2).all(|w| w[0].score >= w[1].score), "records are sorted by score");

    let again = tempfile::tempdir().unwrap();
    let r2 = synced(again.path());
    let a = std::fs::read_to_string(tmp.path().join("home/ranked.jsonl")).unwrap();
    let b = std::fs::read_to_string(again.path().join("home/ranked.jsonl")).unwrap();
    assert_eq!(a, b, "the same dump always produces the same scores");
    assert_eq!(r2.lists, r.lists);
}

#[test]
fn published_snapshot_and_awesome_lists() {
    let tmp = tempfile::tempdir().unwrap();
    let r = synced(tmp.path());
    assert_eq!(r.lists, 1, "only the HTTP server category has three eligible crates");
    let docs = tmp.path().join("docs");
    for f in ["meta.json", "categories.json", "index.jsonl", "RANKING.md", "awesome/index.json", "awesome/index.mdx", "awesome/web-programming--http-server.json", "awesome/web-programming--http-server.mdx"] {
        assert!(docs.join(f).is_file(), "{f}");
    }
    let list: Value = serde_json::from_str(&std::fs::read_to_string(docs.join("awesome/web-programming--http-server.json")).unwrap()).unwrap();
    assert_eq!(list["dump_date"], "2026-11-21");
    let names: Vec<&str> = list["entries"].as_array().unwrap().iter().map(|e| e["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["axum", "poem", "actix-web", "vuln"], "ordered by score: the GPL, unmaintained actix-web trails poem and the crate with an open advisory is last");
    let mdx = std::fs::read_to_string(docs.join("awesome/web-programming--http-server.mdx")).unwrap();
    assert!(mdx.starts_with("---\ntitle:") && mdx.contains("| 1 | [axum](https://crates.io/crates/axum) 0.8.1 |") && mdx.contains("RUSTSEC-2099-0001"));

    // The snapshot alone answers queries (no working data).
    let ds = store::read_dir(&docs).unwrap();
    let hits = query::search(&ds, "web framework", None, 10).unwrap();
    assert_eq!(hits["count"], 4);
    let in_cat = query::search(&ds, "framework", Some("http-server"), 10).unwrap();
    assert_eq!(in_cat["category"], "web-programming::http-server");
    let info = query::info(&ds, "axum", false).unwrap();
    assert_eq!(info["rank"]["categories"]["web-programming::http-server"], 1);
    let cmp = query::compare(&ds, &["axum".into(), "poem".into()], false).unwrap();
    assert_eq!(cmp["winners"]["score"], "axum");
    assert!(query::info(&ds, "does-not-exist", false).is_err());
    assert!(query::compare(&ds, &["axum".into()], false).is_err());
    let all = query::awesome(&ds, None, 25).unwrap();
    assert_eq!(all["count"], 1);
}

#[test]
fn stack_picks_from_the_ranking_and_writes_a_project() {
    let tmp = tempfile::tempdir().unwrap();
    synced(tmp.path());
    let ds = store::read_dir(&tmp.path().join("docs")).unwrap();
    let v = stack::generate(&ds, "web API with auth").unwrap();
    assert_eq!(v["profile"], "web-api");
    let web = v["roles"].as_array().unwrap().iter().find(|r| r["role"] == "web-server").unwrap();
    assert_eq!(web["crate"], "axum");
    assert_eq!(web["alternatives"].as_array().unwrap().len(), 2, "poem and actix-web are the alternatives");
    let cargo = v["files"]["Cargo.toml"].as_str().unwrap();
    assert!(cargo.contains("axum = \"0.8.1\"") && cargo.contains("tokio = { version = \"1.42.0\", features = [\"full\"] }") && cargo.contains("rust-version = \"1.75\""), "{cargo}");
    assert!(v["files"]["src/main.rs"].as_str().unwrap().contains("axum::serve"));
    let out = tmp.path().join("project");
    let files = stack::write(&v, &out).unwrap();
    assert!(files.contains(&"Cargo.toml".to_owned()) && out.join("src/main.rs").is_file());
    // An unmatched goal falls back to the default profile.
    assert_eq!(stack::generate(&ds, "something unrelated").unwrap()["profile"], "cli");
}

#[test]
fn cli_runs_against_the_working_data() {
    let tmp = tempfile::tempdir().unwrap();
    synced(tmp.path());
    let run = |args: &[&str]| {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_fw-crates")).args(args).env("FW_CRATES_HOME", tmp.path().join("home")).output().unwrap();
        assert!(out.status.success(), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8(out.stdout).unwrap()
    };
    let search: Value = serde_json::from_str(&run(&["search", "web", "framework", "--json"])).unwrap();
    assert_eq!(search["op"], "search");
    assert_eq!(search["results"][0]["name"], "axum", "equal relevance, the higher score first");
    let info = run(&["info", "tokio", "--offline"]);
    assert!(info.contains("dump 2026-11-21") && info.contains("tokio 1.42.0"), "{info}");
    assert!(run(&["compare", "axum", "poem", "--mdx", "--offline"]).starts_with("---\ntitle:"));
    assert!(run(&["awesome", "http-server"]).contains("axum"));
    let status: Value = serde_json::from_str(&run(&["status"])).unwrap();
    assert_eq!(status["meta"]["dump_date"], "2026-11-21");
    let listing = run(&["bench", "list", "--json"]);
    assert!(listing.contains("may-minihttp") && listing.contains("actix-web"));
}
