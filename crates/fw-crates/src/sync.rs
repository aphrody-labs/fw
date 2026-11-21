//! Dump ingestion: one streaming pass over `db-dump.tar.gz`, in archive order, never stored on disk.
//!
//! The archive lists `crate_downloads`, `crates`, the keyword/category/owner joins, `versions`, `default_versions`,
//! `dependencies` and `version_downloads` in that order, which is exactly the order the joins need: each table only
//! refers to tables that came before it. CSV columns are looked up by header name (the dump orders them alphabetically).

use crate::advisories::{self, Advisory};
use crate::awesome;
use crate::model::{Advisories, Category, Meta, Record, SCORE_VERSION};
use crate::rank::{self, Inputs, Norm};
use crate::store;
use crate::time::{date_of, parse_ts};
use semver::Version;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};

pub const DUMP_URL: &str = "https://static.crates.io/db-dump.tar.gz";
const ADVISORY_COMMIT_API: &str = "https://api.github.com/repos/rustsec/advisory-db/commits/main";
const ADVISORY_TARBALL: &str = "https://codeload.github.com/rustsec/advisory-db/tar.gz/";
/// Crates are kept in the ranked data when they had any recent download or any dependent.
const DAY: i64 = 86_400;

pub struct SyncOptions {
    /// URL or path of `db-dump.tar.gz`.
    pub dump: String,
    /// `None`: the latest rustsec/advisory-db. `Some("none")`: skip. Otherwise a URL or path of an advisory-db `tar.gz`.
    pub advisories: Option<String>,
    pub out: PathBuf,
    pub publish: Option<PathBuf>,
    pub index_top: usize,
}

impl Default for SyncOptions {
    fn default() -> Self {
        Self { dump: DUMP_URL.to_owned(), advisories: None, out: store::home(), publish: None, index_top: 3000 }
    }
}

#[derive(Debug)]
pub struct SyncReport {
    pub meta: Meta,
    pub lists: usize,
    pub out: PathBuf,
    pub published: Option<PathBuf>,
}

struct Best {
    class: u8,
    version: Version,
    license: String,
    msrv: String,
    edition: String,
}

#[derive(Default)]
struct Acc {
    name: String,
    description: String,
    repository: String,
    created: i64,
    downloads: u64,
    recent: u64,
    dependents: u32,
    owners: u32,
    versions: u32,
    releases_365d: u32,
    last_release: i64,
    categories: Vec<u32>,
    keywords: Vec<u32>,
    best: Option<Best>,
    yanked_default: bool,
}

#[derive(Default)]
pub struct Ingest {
    now: i64,
    meta: Meta,
    categories: HashMap<u32, Category>,
    keywords: HashMap<u32, String>,
    crate_downloads: HashMap<u32, u64>,
    crates: HashMap<u32, Acc>,
    version_crate: Vec<u32>,
    default_versions: HashSet<u32>,
    dep_seen: HashSet<u64>,
}

struct Row<'a> {
    rec: &'a csv::ByteRecord,
    idx: &'a [usize],
}

impl Row<'_> {
    fn bytes(&self, i: usize) -> &[u8] {
        self.rec.get(self.idx[i]).unwrap_or(b"")
    }
    fn text(&self, i: usize) -> String {
        String::from_utf8_lossy(self.bytes(i)).into_owned()
    }
    fn str(&self, i: usize) -> &str {
        std::str::from_utf8(self.bytes(i)).unwrap_or("")
    }
    fn u64(&self, i: usize) -> u64 {
        self.str(i).parse().unwrap_or(0)
    }
    fn u32(&self, i: usize) -> u32 {
        self.str(i).parse().unwrap_or(0)
    }
    fn flag(&self, i: usize) -> bool {
        self.bytes(i) == b"t"
    }
}

fn each_row(r: &mut dyn Read, file: &str, cols: &[&str], mut f: impl FnMut(&Row)) -> Result<(), String> {
    let mut rdr = csv::ReaderBuilder::new().has_headers(true).flexible(true).from_reader(r);
    let headers = rdr.byte_headers().map_err(|e| format!("{file}: {e}"))?.clone();
    let mut idx = Vec::with_capacity(cols.len());
    for c in cols {
        let pos = headers.iter().position(|h| h == c.as_bytes()).ok_or_else(|| format!("{file}: column `{c}` not found"))?;
        idx.push(pos);
    }
    let mut rec = csv::ByteRecord::new();
    while rdr.read_byte_record(&mut rec).map_err(|e| format!("{file}: {e}"))? {
        f(&Row { rec: &rec, idx: &idx });
    }
    Ok(())
}

impl Ingest {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one archive member; `path` is relative to the dump's dated directory (`metadata.json`, `data/crates.csv`).
    pub fn entry(&mut self, path: &str, r: &mut dyn Read) -> Result<(), String> {
        match path {
            "metadata.json" => {
                let mut s = String::new();
                r.read_to_string(&mut s).map_err(|e| e.to_string())?;
                let v: serde_json::Value = serde_json::from_str(&s).map_err(|e| format!("metadata.json: {e}"))?;
                let ts = v["timestamp"].as_str().unwrap_or_default();
                self.now = parse_ts(ts).ok_or_else(|| format!("metadata.json: bad timestamp `{ts}`"))?;
                self.meta.dump_timestamp = ts.to_owned();
                self.meta.dump_date = date_of(self.now);
                self.meta.crates_io_commit = v["crates_io_commit"].as_str().unwrap_or_default().to_owned();
            }
            "data/metadata.csv" => each_row(r, path, &["total_downloads"], |row| self.meta.total_downloads = row.u64(0))?,
            "data/categories.csv" => each_row(r, path, &["id", "slug", "category", "description", "crates_cnt"], |row| {
                self.categories.insert(row.u32(0), Category { slug: row.text(1), name: row.text(2), description: row.text(3), crates: row.u32(4) });
            })?,
            "data/keywords.csv" => each_row(r, path, &["id", "keyword"], |row| {
                self.keywords.insert(row.u32(0), row.text(1));
            })?,
            "data/crate_downloads.csv" => each_row(r, path, &["crate_id", "downloads"], |row| {
                self.crate_downloads.insert(row.u32(0), row.u64(1));
            })?,
            "data/crates.csv" => self.need_clock(path).and_then(|()| self.crates_csv(r, path))?,
            "data/crates_categories.csv" => each_row(r, path, &["crate_id", "category_id"], |row| {
                if let Some(a) = self.crates.get_mut(&row.u32(0)) {
                    a.categories.push(row.u32(1));
                }
            })?,
            "data/crates_keywords.csv" => each_row(r, path, &["crate_id", "keyword_id"], |row| {
                if let Some(a) = self.crates.get_mut(&row.u32(0)) {
                    a.keywords.push(row.u32(1));
                }
            })?,
            "data/crate_owners.csv" => each_row(r, path, &["crate_id"], |row| {
                if let Some(a) = self.crates.get_mut(&row.u32(0)) {
                    a.owners += 1;
                }
            })?,
            "data/versions.csv" => self.need_clock(path).and_then(|()| self.versions_csv(r, path))?,
            "data/default_versions.csv" => each_row(r, path, &["version_id"], |row| {
                self.default_versions.insert(row.u32(0));
            })?,
            "data/dependencies.csv" => each_row(r, path, &["crate_id", "kind", "version_id"], |row| {
                // kind: 0 normal, 1 build, 2 dev. Only default versions count, once per (version, dependency).
                let (dep, vid) = (row.u32(0), row.u32(2));
                if row.bytes(1) != b"2"
                    && self.default_versions.contains(&vid)
                    && self.dep_seen.insert((u64::from(vid) << 32) | u64::from(dep))
                    && let Some(a) = self.crates.get_mut(&dep)
                {
                    a.dependents += 1;
                }
            })?,
            "data/version_downloads.csv" => {
                let cutoff = date_of(self.now - 90 * DAY);
                each_row(r, path, &["date", "downloads", "version_id"], |row| {
                    if row.str(0) < cutoff.as_str() {
                        return;
                    }
                    let Some(&cid) = self.version_crate.get(row.u32(2) as usize) else { return };
                    if let Some(a) = self.crates.get_mut(&cid) {
                        a.recent += row.u64(1);
                    }
                })?;
            }
            _ => {}
        }
        Ok(())
    }

    fn need_clock(&self, path: &str) -> Result<(), String> {
        if self.now == 0 { Err(format!("{path} came before metadata.json; the archive is not a crates.io dump")) } else { Ok(()) }
    }

    fn crates_csv(&mut self, r: &mut dyn Read, path: &str) -> Result<(), String> {
        let downloads = std::mem::take(&mut self.crate_downloads);
        let crates = &mut self.crates;
        each_row(r, path, &["id", "name", "description", "repository", "created_at"], |row| {
            let id = row.u32(0);
            crates.insert(
                id,
                Acc {
                    name: row.text(1),
                    description: row.text(2),
                    repository: row.text(3),
                    created: parse_ts(row.str(4)).unwrap_or(0),
                    downloads: downloads.get(&id).copied().unwrap_or(0),
                    ..Acc::default()
                },
            );
        })
    }

    fn versions_csv(&mut self, r: &mut dyn Read, path: &str) -> Result<(), String> {
        let now = self.now;
        let crates = &mut self.crates;
        let version_crate = &mut self.version_crate;
        each_row(r, path, &["id", "crate_id", "num", "yanked", "created_at", "license", "rust_version", "edition"], |row| {
            let (vid, cid) = (row.u32(0) as usize, row.u32(1));
            if version_crate.len() <= vid {
                version_crate.resize(vid + 1, 0);
            }
            version_crate[vid] = cid;
            let Some(a) = crates.get_mut(&cid) else { return };
            let created = parse_ts(row.str(4)).unwrap_or(0);
            let yanked = row.flag(3);
            a.versions += 1;
            if !yanked {
                a.last_release = a.last_release.max(created);
                if created >= now - 365 * DAY {
                    a.releases_365d += 1;
                }
            }
            let Ok(version) = Version::parse(row.str(2)) else { return };
            let class = if yanked { 0 } else if version.pre.is_empty() { 2 } else { 1 };
            if a.best.as_ref().is_none_or(|b| (class, &version) > (b.class, &b.version)) {
                a.yanked_default = yanked;
                a.best = Some(Best { class, version, license: row.text(5), msrv: row.text(6), edition: row.text(7) });
            }
        })
    }

    /// Joins everything into ranked records, best first.
    pub fn finish(mut self, advisories: &HashMap<String, Vec<Advisory>>) -> (Meta, Vec<Category>, Vec<Record>) {
        let now = self.now;
        let crates_total = self.crates.len();
        let cats = std::mem::take(&mut self.categories);
        let keywords = std::mem::take(&mut self.keywords);
        let mut accs: Vec<Acc> = self.crates.into_values().filter(|a| a.best.is_some() && (a.recent > 0 || a.dependents > 0)).collect();
        accs.sort_by(|a, b| a.name.cmp(&b.name));
        let norm = Norm {
            max_recent_downloads: accs.iter().map(|a| a.recent).max().unwrap_or(0),
            max_dependents: accs.iter().map(|a| a.dependents).max().unwrap_or(0),
        };
        let mut records: Vec<Record> = accs
            .into_iter()
            .map(|a| {
                let best = a.best.unwrap_or_else(|| unreachable!("filtered on best"));
                let mut adv = Advisories::default();
                for x in advisories.get(&a.name).into_iter().flatten().filter(|x| x.affects(&best.version)) {
                    if x.informational.is_some() { adv.informational.push(x.id.clone()) } else { adv.open.push(x.id.clone()) }
                }
                adv.open.sort();
                adv.informational.sort();
                let last_release = if a.last_release > 0 { a.last_release } else { a.created };
                let mut categories: Vec<String> = a.categories.iter().filter_map(|id| cats.get(id).map(|c| c.slug.clone())).collect();
                categories.sort();
                let mut kw: Vec<String> = a.keywords.iter().filter_map(|id| keywords.get(id).cloned()).collect();
                kw.sort();
                let inputs = Inputs {
                    recent_downloads: a.recent,
                    dependents: a.dependents,
                    days_since_release: (now - last_release) as f64 / DAY as f64,
                    releases_365d: a.releases_365d,
                    age_days: (now - a.created) as f64 / DAY as f64,
                    owners: a.owners,
                    has_repository: !a.repository.is_empty(),
                    has_msrv: !best.msrv.is_empty(),
                    license: &best.license,
                    open_advisories: adv.open.len(),
                    informational_advisories: adv.informational.len(),
                };
                let (score, components) = rank::score(&inputs, norm);
                Record {
                    name: a.name,
                    version: best.version.to_string(),
                    description: a.description,
                    repository: a.repository,
                    created: date_of(a.created),
                    last_release: date_of(last_release),
                    downloads: a.downloads,
                    recent_downloads: a.recent,
                    dependents: a.dependents,
                    owners: a.owners,
                    versions: a.versions,
                    releases_365d: a.releases_365d,
                    msrv: best.msrv,
                    edition: best.edition,
                    license: best.license,
                    yanked: a.yanked_default,
                    categories,
                    keywords: kw,
                    advisories: adv,
                    score,
                    components,
                }
            })
            .collect();
        records.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.name.cmp(&b.name)));
        let mut categories: Vec<Category> = cats.into_values().collect();
        categories.sort_by(|a, b| a.slug.cmp(&b.slug));
        let mut meta = self.meta;
        meta.score_version = SCORE_VERSION;
        meta.crates_total = u32::try_from(crates_total).unwrap_or(u32::MAX);
        meta.crates_ranked = u32::try_from(records.len()).unwrap_or(u32::MAX);
        meta.categories = u32::try_from(categories.len()).unwrap_or(u32::MAX);
        (meta, categories, records)
    }
}

/// Streams a `tar.gz` dump into an `Ingest`.
pub fn ingest_dump<R: Read>(reader: R, ing: &mut Ingest, log: &mut dyn FnMut(&str)) -> Result<(), String> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(reader));
    for entry in archive.entries().map_err(|e| format!("dump: {e}"))? {
        let mut entry = entry.map_err(|e| format!("dump: {e}"))?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let full = entry.path().map_err(|e| e.to_string())?.to_string_lossy().replace('\\', "/");
        let rel = full.split_once('/').map_or(full.as_str(), |(_, rest)| rest).to_owned();
        let wanted = matches!(
            rel.as_str(),
            "metadata.json" | "data/metadata.csv" | "data/categories.csv" | "data/keywords.csv" | "data/crate_downloads.csv" | "data/crates.csv" | "data/crates_categories.csv"
                | "data/crates_keywords.csv" | "data/crate_owners.csv" | "data/versions.csv" | "data/default_versions.csv" | "data/dependencies.csv" | "data/version_downloads.csv"
        );
        if wanted {
            log(&format!("reading {rel} ({} bytes)", entry.size()));
            ing.entry(&rel, &mut entry)?;
        }
    }
    Ok(())
}

fn open_source(src: &str) -> Result<Box<dyn Read + Send>, String> {
    if src.starts_with("http://") || src.starts_with("https://") {
        crate::http::open(src)
    } else {
        Ok(Box::new(std::io::BufReader::new(std::fs::File::open(src).map_err(|e| format!("{src}: {e}"))?)))
    }
}

/// Loads advisories: `(commit or source label, advisories)`.
fn load_advisories(spec: Option<&str>) -> Result<(String, Vec<Advisory>), String> {
    match spec {
        Some("none") => Ok((String::new(), Vec::new())),
        Some(src) => {
            let list = advisories::load_tar_gz(open_source(src)?)?;
            Ok((src.to_owned(), list))
        }
        None => {
            let sha = crate::http::get_github(ADVISORY_COMMIT_API, "application/vnd.github.sha")?.trim().to_owned();
            let list = advisories::load_tar_gz(open_source(&format!("{ADVISORY_TARBALL}{sha}"))?)?;
            Ok((sha, list))
        }
    }
}

/// Downloads (or reads) the dump and the advisory-db, ranks every crate, writes the working data to `out` and, when
/// `publish` is set, the committed snapshot with the awesome lists.
pub fn sync(opts: &SyncOptions, log: &mut dyn FnMut(&str)) -> Result<SyncReport, String> {
    let (adv_source, adv_list) = load_advisories(opts.advisories.as_deref())?;
    let advisory_count = u32::try_from(adv_list.len()).unwrap_or(u32::MAX);
    log(&format!("advisory-db: {advisory_count} advisories ({})", if adv_source.is_empty() { "skipped" } else { &adv_source }));
    let by_pkg = advisories::by_package(adv_list);

    let mut ing = Ingest::new();
    ingest_dump(open_source(&opts.dump)?, &mut ing, log)?;
    let (mut meta, categories, records) = ing.finish(&by_pkg);
    meta.advisory_db = adv_source;
    meta.advisories = advisory_count;
    meta.source = if opts.dump.starts_with("http") { opts.dump.clone() } else { "local dump".to_owned() };

    store::write_working(&opts.out, &meta, &categories, &records)?;
    log(&format!("wrote {} ranked crates to {}", records.len(), opts.out.display()));
    let mut lists = 0;
    if let Some(dir) = &opts.publish {
        lists = awesome::publish(dir, &meta, &categories, &records, opts.index_top)?;
        log(&format!("published {lists} awesome lists to {}", Path::new(dir).display()));
    }
    Ok(SyncReport { meta, lists, out: opts.out.clone(), published: opts.publish.clone() })
}
