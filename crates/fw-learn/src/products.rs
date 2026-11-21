//! A repository at one revision: its tree, its products (directories carrying `product.toml` or `ay.toml`) and its units
//! (directories carrying a Cargo.toml, package.json or pyproject.toml). Everything is read from git objects, so a dirty or
//! stale working tree does not change what is learned.

use anyhow::{Context, Result, bail};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const PRODUCT_MANIFESTS: &[&str] = &["product.toml", "ay.toml"];
pub const UNIT_MANIFESTS: &[&str] = &["Cargo.toml", "package.json", "pyproject.toml"];

pub struct Repo {
    pub path: PathBuf,
    pub name: String,
    pub slug: Option<String>,
    pub rev: String,
    pub head: String,
}

pub struct TreeEntry {
    pub sha: String,
    pub path: String,
}

#[derive(Clone, Debug, Default)]
pub struct Product {
    pub id: String,
    pub display: String,
    pub description: String,
    /// Directory of the product inside the repository; empty for the repository root.
    pub dir: String,
    pub docs_url: String,
}

pub fn git(path: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git").arg("-C").arg(path).args(args).output().with_context(|| format!("run git {}", args.join(" ")))?;
    if !output.status.success() {
        bail!("git {}: {}", args.join(" "), String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim_end().to_owned())
}

impl Repo {
    pub fn open(path: &Path, rev: Option<&str>) -> Result<Self> {
        let top = PathBuf::from(git(path, &["rev-parse", "--show-toplevel"])?);
        let name = top.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| "repo".to_owned());
        let slug = git(&top, &["remote", "get-url", "origin"]).ok().and_then(|url| parse_slug(&url));
        let rev = match rev {
            Some(rev) => rev.to_owned(),
            None if git(&top, &["rev-parse", "--verify", "-q", "origin/main"]).is_ok() => "origin/main".to_owned(),
            None => "HEAD".to_owned(),
        };
        let head = git(&top, &["rev-parse", &rev])?;
        Ok(Self { path: top, name, slug, rev, head })
    }

    pub fn short_head(&self) -> &str {
        &self.head[..self.head.len().min(10)]
    }

    pub fn tree(&self) -> Result<Vec<TreeEntry>> {
        let output = Command::new("git").arg("-C").arg(&self.path).args(["ls-tree", "-r", "-z", &self.rev]).output().context("git ls-tree")?;
        if !output.status.success() {
            bail!("git ls-tree {}: {}", self.rev, String::from_utf8_lossy(&output.stderr).trim());
        }
        Ok(output
            .stdout
            .split(|byte| *byte == 0)
            .filter_map(|record| {
                let text = String::from_utf8_lossy(record);
                let (meta, path) = text.split_once('\t')?;
                let mut fields = meta.split(' ');
                let (_mode, kind, sha) = (fields.next()?, fields.next()?, fields.next()?);
                (kind == "blob").then(|| TreeEntry { sha: sha.to_owned(), path: path.to_owned() })
            })
            .collect())
    }

    /// Reads blobs through one `git cat-file --batch` process; a blob above `limit` bytes comes back empty.
    pub fn blobs(&self, shas: &[&str], limit: usize) -> Result<Vec<Vec<u8>>> {
        if shas.is_empty() {
            return Ok(Vec::new());
        }
        let mut child = Command::new("git")
            .arg("-C")
            .arg(&self.path)
            .args(["cat-file", "--batch"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("spawn git cat-file")?;
        let mut stdin = child.stdin.take().context("cat-file stdin")?;
        let wanted: Vec<String> = shas.iter().map(|sha| format!("{sha}\n")).collect();
        let writer = std::thread::spawn(move || {
            for line in wanted {
                if stdin.write_all(line.as_bytes()).is_err() {
                    break;
                }
            }
        });
        let mut reader = BufReader::new(child.stdout.take().context("cat-file stdout")?);
        let mut blobs = Vec::with_capacity(shas.len());
        for _ in shas {
            let mut header = String::new();
            reader.read_line(&mut header).context("read cat-file header")?;
            let size: usize = header.split_whitespace().nth(2).and_then(|size| size.parse().ok()).unwrap_or(0);
            let mut content = vec![0u8; size];
            reader.read_exact(&mut content).context("read cat-file blob")?;
            let mut newline = [0u8; 1];
            reader.read_exact(&mut newline).context("read cat-file separator")?;
            blobs.push(if size > limit { Vec::new() } else { content });
        }
        writer.join().map_err(|_| anyhow::anyhow!("cat-file writer panicked"))?;
        child.wait().context("wait for cat-file")?;
        Ok(blobs)
    }
}

pub fn parse_slug(url: &str) -> Option<String> {
    let tail = url.split("github.com").nth(1)?.trim_start_matches([':', '/']);
    let slug = tail.trim_end_matches(".git").trim_end_matches('/');
    (slug.matches('/').count() == 1).then(|| slug.to_owned())
}

pub fn skipped(path: &str) -> bool {
    path.split('/').any(|segment| matches!(segment, "node_modules" | "target" | "third_party" | "vendor" | "vendored" | ".git" | "dist" | "fixtures" | "testdata" | "snapshots" | "test_fixture" | "test_fixtures"))
}

pub fn discover_products(repo: &Repo, tree: &[TreeEntry]) -> Result<Vec<Product>> {
    let manifests: Vec<&TreeEntry> = tree
        .iter()
        .filter(|entry| {
            let (dir, file) = entry.path.rsplit_once('/').unwrap_or(("", &entry.path));
            PRODUCT_MANIFESTS.contains(&file) && dir.matches('/').count() == 0 && !skipped(dir)
        })
        .collect();
    let blobs = repo.blobs(&manifests.iter().map(|entry| entry.sha.as_str()).collect::<Vec<_>>(), 1 << 20)?;
    let mut by_dir: BTreeMap<String, Product> = BTreeMap::new();
    for (entry, blob) in manifests.iter().zip(blobs) {
        let dir = entry.path.rsplit_once('/').map_or("", |(dir, _)| dir).to_owned();
        let Ok(table) = String::from_utf8_lossy(&blob).parse::<toml::Table>() else {
            continue;
        };
        let product = product_from(&table, &dir, &repo.name);
        by_dir.entry(dir).or_insert(product);
    }
    if by_dir.is_empty() {
        by_dir.insert(String::new(), Product { id: repo.name.clone(), display: repo.name.clone(), ..Product::default() });
    }
    Ok(by_dir.into_values().collect())
}

fn text<'a>(table: &'a toml::Table, key: &str) -> Option<&'a str> {
    table.get(key).and_then(toml::Value::as_str).filter(|value| !value.is_empty())
}

fn product_from(table: &toml::Table, dir: &str, repo_name: &str) -> Product {
    let section = table.get("product").and_then(toml::Value::as_table);
    let from_section = |key: &str| section.and_then(|section| text(section, key));
    let id = if dir.is_empty() {
        text(table, "name").or_else(|| from_section("slug")).or_else(|| from_section("repo")).unwrap_or(repo_name).to_owned()
    } else {
        dir.rsplit('/').next().unwrap_or(dir).to_owned()
    };
    let docs_url = table
        .get("urls")
        .and_then(toml::Value::as_table)
        .and_then(|urls| text(urls, "docs"))
        .or_else(|| text(table, "homepage"))
        .unwrap_or_default()
        .to_owned();
    Product {
        display: text(table, "display_name").or_else(|| from_section("display")).or_else(|| from_section("name")).unwrap_or(&id).to_owned(),
        description: from_section("description").or_else(|| text(table, "description")).unwrap_or_default().to_owned(),
        dir: dir.to_owned(),
        docs_url,
        id,
    }
}

/// Product that owns a path: the deepest product directory that prefixes it, else the root product.
pub fn product_of<'a>(products: &'a [Product], path: &str) -> &'a Product {
    products
        .iter()
        .filter(|product| !product.dir.is_empty() && path.starts_with(&format!("{}/", product.dir)))
        .max_by_key(|product| product.dir.len())
        .or_else(|| products.iter().find(|product| product.dir.is_empty()))
        .unwrap_or(&products[0])
}

/// Maps a file path to the unit (manifest directory) that owns it. Paths of deleted units fall back to the container
/// directory (a directory with several units, such as `crates`) plus one segment, then to the first segment.
pub struct AreaIndex {
    dirs: BTreeSet<String>,
    product_dirs: Vec<String>,
    units: BTreeSet<String>,
    containers: BTreeSet<String>,
}

impl AreaIndex {
    pub fn from_tree(tree: &[TreeEntry], products: &[Product]) -> Self {
        let units: BTreeSet<String> = tree
            .iter()
            .filter_map(|entry| {
                let (dir, file) = entry.path.rsplit_once('/')?;
                (UNIT_MANIFESTS.contains(&file) && !skipped(dir)).then(|| dir.to_owned())
            })
            .collect();
        let mut children: BTreeMap<&str, usize> = BTreeMap::new();
        for unit in &units {
            if let Some((parent, _)) = unit.rsplit_once('/') {
                *children.entry(parent).or_default() += 1;
            }
        }
        let containers = children.into_iter().filter(|(_, count)| *count >= 3).map(|(parent, _)| parent.to_owned()).collect();
        let mut dirs = BTreeSet::new();
        for entry in tree {
            let segments: Vec<&str> = entry.path.split('/').collect();
            for depth in 1..segments.len().min(4) {
                dirs.insert(segments[..depth].join("/"));
            }
        }
        let product_dirs = products.iter().filter(|product| !product.dir.is_empty()).map(|product| product.dir.clone()).collect();
        Self { dirs, product_dirs, units, containers }
    }

    /// History keeps the paths a commit had when it was made, and imported histories (a subtree merge of an upstream) lack
    /// the product directory prefix; a path that is not a directory of the tree but is one below a product directory is
    /// mapped there.
    pub fn resolve(&self, path: &str) -> String {
        let segments: Vec<&str> = path.split('/').collect();
        for depth in (1..segments.len().min(3)).rev() {
            let prefix = segments[..depth].join("/");
            if self.dirs.contains(&prefix) {
                return path.to_owned();
            }
            if let Some(dir) = self.product_dirs.iter().find(|dir| self.dirs.contains(&format!("{dir}/{prefix}"))) {
                return format!("{dir}/{path}");
            }
        }
        path.to_owned()
    }

    /// True when the area is a unit or a directory of the current tree; areas of deleted code teach nothing about the product.
    pub fn is_live(&self, area: &str) -> bool {
        self.units.contains(area) || self.dirs.contains(area) || self.product_dirs.iter().any(|dir| self.dirs.contains(&format!("{dir}/{area}")))
    }

    pub fn area_of(&self, path: &str) -> String {
        let mut dir = path;
        while let Some((parent, _)) = dir.rsplit_once('/') {
            if self.units.contains(parent) {
                return parent.to_owned();
            }
            dir = parent;
        }
        let segments: Vec<&str> = path.split('/').collect();
        for depth in (1..segments.len()).rev() {
            let prefix = segments[..depth].join("/");
            if self.containers.contains(&prefix) {
                return segments[..(depth + 1).min(segments.len() - 1)].join("/");
            }
        }
        segments.first().filter(|_| segments.len() > 1).map_or_else(|| "(root)".to_owned(), |first| (*first).to_owned())
    }
}

/// Child directories of `dir` that are git repositories carrying a product, agent or naming manifest.
pub fn discover_repos(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut repos: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.join(".git").exists() && ["product.toml", "ay.toml", "naming.toml", "AGENTS.md", "CLAUDE.md"].iter().any(|manifest| path.join(manifest).is_file())
        })
        .collect();
    repos.sort();
    repos
}
