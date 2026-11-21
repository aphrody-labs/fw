//! Source registry: what each corpus is, its licence and how it is synced.

use crate::http::Client;
use crate::sitemap::Fetch;
use crate::store::Store;
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::path::PathBuf;

pub struct Opts {
    pub root: PathBuf,
    /// Cap on documents per source (smoke runs); disables pruning.
    pub limit: Option<usize>,
    pub threads: usize,
    /// Minimum milliseconds between two requests to one host.
    pub rate_ms: u64,
    /// Ignore stored revisions.
    pub force: bool,
    /// Sitemap sources only: fetch just the pages whose URL contains this text (prioritise a section; disables pruning).
    pub only: Option<String>,
}

pub struct Report {
    pub source: String,
    pub listed: usize,
    pub fetched: usize,
    pub unchanged: usize,
    pub failed: usize,
    pub blocked_by_robots: usize,
    pub pruned: usize,
    pub bytes: usize,
    pub errors: Vec<String>,
}

impl Report {
    pub fn new(source: &str) -> Self {
        Self { source: source.to_owned(), listed: 0, fetched: 0, unchanged: 0, failed: 0, blocked_by_robots: 0, pruned: 0, bytes: 0, errors: Vec::new() }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "source": self.source, "listed": self.listed, "fetched": self.fetched, "unchanged": self.unchanged,
            "failed": self.failed, "blockedByRobots": self.blocked_by_robots, "pruned": self.pruned,
            "bytes": self.bytes, "errors": self.errors,
        })
    }
}

pub struct Sitemap {
    pub id: &'static str,
    pub sitemap: &'static str,
    pub base: &'static str,
    pub filter: fn(&str) -> bool,
    pub fetch: Fetch,
    pub license: &'static str,
}

pub struct Walk {
    pub repo: &'static str,
    pub root: &'static str,
    /// Only `README.md` files (blink) instead of every markdown file.
    pub readme_only: bool,
    /// Directory levels below `root` to descend.
    pub depth: usize,
}

pub struct Gitiles {
    pub id: &'static str,
    pub host: &'static str,
    /// Index every repo (name, description, HEAD, last commit date, README).
    pub inventory: bool,
    pub walks: &'static [Walk],
    pub license: &'static str,
}

#[derive(Clone, Copy)]
pub enum UrlMap {
    /// `https://github.com/<repo>/blob/<commit>/<path>`.
    Blob,
    /// mdBook source under `src/` published at `<base>/<path>.html`.
    MdBook(&'static str),
    /// `docs/topics/x.md` published at `https://kotlinlang.org/docs/x.html`.
    Kotlin,
    /// `content/a/b/index.md` published at `https://docs.github.com/en/a/b`.
    GithubDocs,
}

pub struct Repo {
    pub repo: &'static str,
    /// Only files under this path prefix.
    pub prefix: &'static str,
    pub map: UrlMap,
    pub license: &'static str,
}

pub struct Github {
    pub id: &'static str,
    pub repos: &'static [Repo],
}

pub enum Kind {
    Sitemap(Sitemap),
    Gitiles(Gitiles),
    Github(Github),
    Orgs(&'static [&'static str]),
    RustStd,
}

pub struct Source {
    pub id: &'static str,
    pub title: &'static str,
    pub kind: Kind,
}

const GOOGLE: &str = "(c) Google; local search only, distilled skills must cite and paraphrase";
const CC_BY_25: &str = "CC BY 2.5 (Apache-2.0 samples), Google";
const APACHE: &str = "Apache-2.0";
const MIT_APACHE: &str = "MIT OR Apache-2.0";
const LLVM_LICENSE: &str = "Apache-2.0 WITH LLVM-exception";

fn android_dev(url: &str) -> bool {
    const KEEP: &[&str] = &[
        "/about/", "/develop/", "/guide/", "/training/", "/design/", "/games/", "/studio/", "/jetpack/", "/build/", "/tools/",
        "/ndk/", "/privacy-and-security/", "/health-and-fitness/", "/ai/", "/identity/", "/google/", "/topic/", "/media/",
    ];
    let path = url.split_once("://").and_then(|(_, rest)| rest.split_once('/')).map_or("", |(_, path)| path);
    let path = format!("/{path}");
    KEEP.iter().any(|prefix| path.starts_with(prefix))
}

fn any(_: &str) -> bool {
    true
}

pub fn all() -> Vec<Source> {
    vec![
        Source {
            id: "design-google",
            title: "design.google",
            kind: Kind::Sitemap(Sitemap { id: "design-google", sitemap: "https://design.google/sitemap.xml", base: "https://design.google/", filter: any, fetch: Fetch::Html, license: GOOGLE }),
        },
        Source {
            id: "m3",
            title: "Material Design 3 (m3.material.io, rendered in headless Chrome)",
            kind: Kind::Sitemap(Sitemap { id: "m3", sitemap: "https://m3.material.io/sitemap.xml", base: "https://m3.material.io/", filter: any, fetch: Fetch::Chrome, license: GOOGLE }),
        },
        Source {
            id: "android-dev",
            title: "developer.android.com guides (API reference pages excluded)",
            kind: Kind::Sitemap(Sitemap { id: "android-dev", sitemap: "https://developer.android.com/sitemap.xml", base: "https://developer.android.com/", filter: android_dev, fetch: Fetch::MarkdownText, license: CC_BY_25 }),
        },
        Source {
            id: "android-source",
            title: "source.android.com",
            kind: Kind::Sitemap(Sitemap { id: "android-source", sitemap: "https://source.android.com/sitemap.xml", base: "https://source.android.com/", filter: any, fetch: Fetch::MarkdownText, license: CC_BY_25 }),
        },
        Source {
            id: "chromium-repos",
            title: "chromium.googlesource.com repo inventory and READMEs",
            kind: Kind::Gitiles(Gitiles { id: "chromium-repos", host: "chromium.googlesource.com", inventory: true, walks: &[], license: "BSD-3-Clause" }),
        },
        Source {
            id: "chromium-docs",
            title: "chromium/src docs, styleguide and Blink READMEs",
            kind: Kind::Gitiles(Gitiles {
                id: "chromium-docs",
                host: "chromium.googlesource.com",
                inventory: false,
                walks: &[
                    Walk { repo: "chromium/src", root: "docs", readme_only: false, depth: 8 },
                    Walk { repo: "chromium/src", root: "styleguide", readme_only: false, depth: 4 },
                    Walk { repo: "chromium/src", root: "third_party/blink/renderer", readme_only: true, depth: 7 },
                    Walk { repo: "chromium/src", root: "third_party/blink/public", readme_only: true, depth: 5 },
                    Walk { repo: "chromium/src", root: "content/public", readme_only: true, depth: 5 },
                ],
                license: "BSD-3-Clause",
            }),
        },
        Source {
            id: "aosp-repos",
            title: "android.googlesource.com repo inventory and READMEs",
            kind: Kind::Gitiles(Gitiles { id: "aosp-repos", host: "android.googlesource.com", inventory: true, walks: &[], license: "Apache-2.0" }),
        },
        Source {
            id: "aosp-docs",
            title: "AOSP docs trees (platform/developers/docs, build, soong, art, tools)",
            kind: Kind::Gitiles(Gitiles {
                id: "aosp-docs",
                host: "android.googlesource.com",
                inventory: false,
                walks: &[
                    Walk { repo: "platform/developers/docs", root: "", readme_only: false, depth: 6 },
                    Walk { repo: "platform/docs/source.android.com", root: "", readme_only: false, depth: 1 },
                    Walk { repo: "platform/build", root: "", readme_only: false, depth: 1 },
                    Walk { repo: "platform/build/soong", root: "docs", readme_only: false, depth: 8 },
                    Walk { repo: "platform/art", root: "docs", readme_only: false, depth: 8 },
                ],
                license: "Apache-2.0",
            }),
        },
        Source {
            id: "github-docs",
            title: "docs.github.com (github/docs content)",
            kind: Kind::Github(Github { id: "github-docs", repos: &[Repo { repo: "github/docs", prefix: "content/", map: UrlMap::GithubDocs, license: "CC-BY-4.0" }] }),
        },
        Source { id: "github-orgs", title: "aphrody-labs and aphrody-dev repo inventories (gh api)", kind: Kind::Orgs(&["aphrody-labs", "aphrody-dev"]) },
        Source {
            id: "kotlin",
            title: "kotlinlang.org docs (JetBrains/kotlin-web-site)",
            kind: Kind::Github(Github { id: "kotlin", repos: &[Repo { repo: "JetBrains/kotlin-web-site", prefix: "docs/topics/", map: UrlMap::Kotlin, license: APACHE }] }),
        },
        Source {
            id: "compose-multiplatform",
            title: "Compose Multiplatform, Kotlin Multiplatform docs, Compose Hot Reload",
            kind: Kind::Github(Github {
                id: "compose-multiplatform",
                repos: &[
                    Repo { repo: "JetBrains/compose-multiplatform", prefix: "", map: UrlMap::Blob, license: APACHE },
                    Repo { repo: "JetBrains/kotlin-multiplatform-dev-docs", prefix: "topics/", map: UrlMap::Blob, license: APACHE },
                    Repo { repo: "JetBrains/compose-hot-reload", prefix: "", map: UrlMap::Blob, license: APACHE },
                ],
            }),
        },
        Source {
            id: "rust",
            title: "Rust book, reference, rustc book (platform support), rustdoc book, unstable book, rustc dev guide, nomicon, edition guide",
            kind: Kind::Github(Github {
                id: "rust",
                repos: &[
                    Repo { repo: "rust-lang/book", prefix: "src/", map: UrlMap::MdBook("https://doc.rust-lang.org/book"), license: MIT_APACHE },
                    Repo { repo: "rust-lang/reference", prefix: "src/", map: UrlMap::MdBook("https://doc.rust-lang.org/reference"), license: MIT_APACHE },
                    Repo { repo: "rust-lang/rustc-dev-guide", prefix: "src/", map: UrlMap::MdBook("https://rustc-dev-guide.rust-lang.org"), license: MIT_APACHE },
                    Repo { repo: "rust-lang/nomicon", prefix: "src/", map: UrlMap::MdBook("https://doc.rust-lang.org/nomicon"), license: MIT_APACHE },
                    Repo { repo: "rust-lang/edition-guide", prefix: "src/", map: UrlMap::MdBook("https://doc.rust-lang.org/edition-guide"), license: MIT_APACHE },
                    Repo { repo: "rust-lang/rust", prefix: "src/doc/rustc/src/", map: UrlMap::MdBook("https://doc.rust-lang.org/rustc"), license: MIT_APACHE },
                    Repo { repo: "rust-lang/rust", prefix: "src/doc/rustdoc/src/", map: UrlMap::MdBook("https://doc.rust-lang.org/rustdoc"), license: MIT_APACHE },
                    Repo { repo: "rust-lang/rust", prefix: "src/doc/unstable-book/src/", map: UrlMap::MdBook("https://doc.rust-lang.org/nightly/unstable-book"), license: MIT_APACHE },
                    Repo { repo: "rust-lang/rust", prefix: "src/doc/style-guide/src/", map: UrlMap::MdBook("https://doc.rust-lang.org/nightly/style-guide"), license: MIT_APACHE },
                ],
            }),
        },
        Source {
            id: "cargo",
            title: "The Cargo Book (rust-lang/cargo doc/book/src)",
            kind: Kind::Github(Github { id: "cargo", repos: &[Repo { repo: "rust-lang/cargo", prefix: "doc/book/src/", map: UrlMap::MdBook("https://doc.rust-lang.org/cargo"), license: MIT_APACHE }] }),
        },
        Source {
            id: "rust-process",
            title: "Rust Forge, accepted RFCs and project goals",
            kind: Kind::Github(Github {
                id: "rust-process",
                repos: &[
                    Repo { repo: "rust-lang/rust-forge", prefix: "src/", map: UrlMap::MdBook("https://forge.rust-lang.org"), license: "MIT OR Apache-2.0" },
                    Repo { repo: "rust-lang/rfcs", prefix: "text/", map: UrlMap::MdBook("https://rust-lang.github.io/rfcs"), license: "MIT OR Apache-2.0" },
                    Repo { repo: "rust-lang/goals", prefix: "src/", map: UrlMap::MdBook("https://rust-lang.github.io/rust-project-goals"), license: "MIT" },
                ],
            }),
        },
        Source {
            id: "rust-cross",
            title: "Cross-compilation: cross, cargo-zigbuild, cargo-xwin, xwin",
            kind: Kind::Github(Github {
                id: "rust-cross",
                repos: &[
                    Repo { repo: "cross-rs/cross", prefix: "", map: UrlMap::Blob, license: "MIT OR Apache-2.0" },
                    Repo { repo: "rust-cross/cargo-zigbuild", prefix: "", map: UrlMap::Blob, license: "MIT" },
                    Repo { repo: "rust-cross/cargo-xwin", prefix: "", map: UrlMap::Blob, license: "MIT" },
                    Repo { repo: "Jake-Shadle/xwin", prefix: "", map: UrlMap::Blob, license: "MIT OR Apache-2.0" },
                ],
            }),
        },
        Source {
            id: "llvm",
            title: "LLVM project docs: LLVM, Clang, lld, MLIR",
            kind: Kind::Github(Github {
                id: "llvm",
                repos: &[
                    Repo { repo: "llvm/llvm-project", prefix: "llvm/docs/", map: UrlMap::MdBook("https://llvm.org/docs"), license: LLVM_LICENSE },
                    Repo { repo: "llvm/llvm-project", prefix: "clang/docs/", map: UrlMap::MdBook("https://clang.llvm.org/docs"), license: LLVM_LICENSE },
                    Repo { repo: "llvm/llvm-project", prefix: "lld/docs/", map: UrlMap::MdBook("https://lld.llvm.org"), license: LLVM_LICENSE },
                    Repo { repo: "llvm/llvm-project", prefix: "mlir/docs/", map: UrlMap::Blob, license: LLVM_LICENSE },
                ],
            }),
        },
        Source { id: "rust-std", title: "Rust std, core and alloc item docs from rustdoc JSON (nightly)", kind: Kind::RustStd },
    ]
}

pub fn find(id: &str) -> Option<Source> {
    all().into_iter().find(|source| source.id == id)
}

pub fn license(source: &Source) -> &'static str {
    match &source.kind {
        Kind::Sitemap(spec) => spec.license,
        Kind::Gitiles(spec) => spec.license,
        Kind::Github(spec) => spec.repos.first().map_or("", |repo| repo.license),
        Kind::Orgs(_) => "metadata only",
        Kind::RustStd => MIT_APACHE,
    }
}

pub fn sync(source: &Source, opts: &Opts) -> Result<Report> {
    let mut store = Store::open(&opts.root, source.id)?;
    // googlesource answers 429 well below the default rate once two crawls share the host.
    let rate_ms = if matches!(source.kind, Kind::Gitiles(_)) { opts.rate_ms.max(500) } else { opts.rate_ms };
    let client = Client::new(rate_ms);
    let started = crate::now_unix();
    let report = match &source.kind {
        Kind::Sitemap(spec) => crate::sitemap::sync(&client, &mut store, spec, opts)?,
        Kind::Gitiles(spec) => crate::gitiles::sync(&client, &mut store, spec, opts)?,
        Kind::Github(spec) => crate::github::sync_docs(&client, &mut store, spec, opts)?,
        Kind::Orgs(orgs) => crate::github::sync_orgs(&client, &mut store, source.id, orgs, opts)?,
        Kind::RustStd => crate::rustdoc::sync(&mut store, opts)?,
    };
    store.set_meta("title", source.title)?;
    store.set_meta("license", license(source))?;
    store.set_meta("last_sync", &json!({"startedAt": crate::iso(started), "finishedAt": crate::iso(crate::now_unix()), "report": report.to_json()}).to_string())?;
    if report.failed > 0 && report.fetched == 0 && report.unchanged == 0 {
        bail!("{}: every fetch failed: {:?}", source.id, report.errors);
    }
    Ok(report)
}
