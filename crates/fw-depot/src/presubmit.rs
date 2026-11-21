//! `presubmit`: two ways to check a change.
//!
//! * `canned`: the checks every Chromium `PRESUBMIT.py` starts from (`presubmit_canned_checks.py`), ported to Rust and run on the lines a change
//!   adds: no tabs, no trailing whitespace, no CR, one final newline, long lines. No Python involved.
//! * `python`: the real `presubmit_support.py` of a depot_tools checkout, run under `uv` (or `python3`) on the change, for the project's own
//!   `PRESUBMIT.py` files, whose `input_api` is Python and cannot be ported as a whole.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{Error, Result};
use crate::git;

/// A file of a change with the lines it adds.
#[derive(Debug, Clone)]
pub struct Affected {
    pub path: String,
    pub contents: Option<Vec<u8>>,
    /// `(line number, text)` of every added line.
    pub added: Vec<(usize, String)>,
}

/// `git diff -U0 <base>` of the working tree: the files and added lines of a change.
pub fn change(repo: &Path, base: &str) -> Result<Vec<Affected>> {
    let diff = git::run(repo, &["diff", "-U0", "--no-color", "--no-ext-diff", "--no-renames", base, "--"])?;
    let mut files: Vec<Affected> = Vec::new();
    let mut line_number = 0usize;
    for line in diff.lines() {
        if let Some(path) = line.strip_prefix("+++ b/") {
            files.push(Affected {
                path: path.to_owned(),
                contents: std::fs::read(repo.join(path)).ok(),
                added: Vec::new(),
            });
        } else if line.starts_with("+++ ") {
            files.push(Affected { path: String::new(), contents: None, added: Vec::new() });
        } else if let Some(header) = line.strip_prefix("@@ ") {
            let plus = header.split_whitespace().find(|part| part.starts_with('+')).unwrap_or("+0");
            let start = plus[1..].split(',').next().unwrap_or("0");
            line_number = start.parse().unwrap_or(0);
        } else if let Some(added) = line.strip_prefix('+')
            && !line.starts_with("+++")
        {
            if let Some(file) = files.last_mut() {
                file.added.push((line_number, added.to_owned()));
            }
            line_number += 1;
        }
    }
    files.retain(|file| !file.path.is_empty());
    Ok(files)
}

fn extension(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.').map_or("", |(_, extension)| extension)
}

/// `InputApi.FilterSourceFile` with the default `files_to_check` and `files_to_skip` of presubmit_support.py.
pub fn is_source_file(path: &str) -> bool {
    const CHECK: &[&str] = &[
        "c", "cc", "cpp", "h", "m", "mm", "inl", "asm", "hxx", "hpp", "s", "S", "js", "ts", "py", "star", "sh", "rb", "pl", "pm", "java", "mk",
        "am", "css", "mojom", "fidl", "rs",
    ];
    let normalized = path.replace('\\', "/");
    let segments: Vec<&str> = normalized.split('/').collect();
    let name = segments.last().copied().unwrap_or_default();
    let in_directory = |wanted: &str| segments[..segments.len().saturating_sub(1)].contains(&wanted);
    let checked = CHECK.contains(&extension(path)) && name.contains('.')
        || normalized.contains("/res/") && normalized.ends_with(".xml");
    if !checked {
        return false;
    }
    let third_party_skipped = segments
        .windows(2)
        .enumerate()
        .any(|(index, pair)| pair[0] == "third_party" && !matches!(pair[1], "WebKit" | "blink") && index + 1 < segments.len());
    let skipped = normalized.starts_with("testing_support/google_appengine/")
        || in_directory("experimental")
        || third_party_skipped
        || in_directory("Debug")
        || in_directory("Release")
        || in_directory("xcodebuild")
        || in_directory("out")
        || in_directory(".git")
        || in_directory(".svn")
        || normalized.ends_with(".diff")
        || normalized.ends_with(".patch")
        || (name.len() >= 2 && name.bytes().all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_'));
    !skipped
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub check: &'static str,
    pub message: &'static str,
    /// `path` or `path:line`.
    pub location: String,
}

fn files(changed: &[Affected]) -> impl Iterator<Item = &Affected> {
    changed.iter().filter(|file| is_source_file(&file.path))
}

pub fn check_no_tabs(changed: &[Affected]) -> Vec<Finding> {
    let mut out = Vec::new();
    for file in files(changed) {
        let basename = file.path.rsplit('/').next().unwrap_or_default();
        if matches!(basename, "Makefile" | "makefile") || basename.ends_with(".mk") {
            continue;
        }
        for (number, text) in &file.added {
            if text.contains('\t') {
                out.push(Finding {
                    check: "CheckChangeHasNoTabs",
                    message: "Found a tab character in:",
                    location: format!("{}:{number}", file.path),
                });
            }
        }
    }
    out
}

pub fn check_no_stray_whitespace(changed: &[Affected]) -> Vec<Finding> {
    let mut out = Vec::new();
    for file in files(changed) {
        for (number, text) in &file.added {
            if text.trim_end() != text {
                out.push(Finding {
                    check: "CheckChangeHasNoStrayWhitespace",
                    message: "Found line ending with white spaces in:",
                    location: format!("{}:{number}", file.path),
                });
            }
        }
    }
    out
}

pub fn check_no_cr_and_one_eol(changed: &[Affected]) -> Vec<Finding> {
    let mut out = Vec::new();
    for file in files(changed) {
        let Some(contents) = &file.contents else {
            continue;
        };
        if contents.contains(&b'\r') {
            out.push(Finding {
                check: "CheckChangeHasNoCR",
                message: "Found a CR character in these files:",
                location: file.path.clone(),
            });
        }
        let length = contents.len();
        if length > 1 && (contents[length - 1] != b'\n' || contents[length - 2] == b'\n') {
            out.push(Finding {
                check: "CheckChangeHasOnlyOneEol",
                message: "These files should end in one (and only one) newline character:",
                location: file.path.clone(),
            });
        }
    }
    out
}

fn long_symbol(line: &str, symbol_length: usize) -> bool {
    // `.*[A-Za-z][A-Za-z_0-9]{N,}.*`: a letter followed by at least N word characters.
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_' {
            let start = index;
            while index < bytes.len() && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_') {
                index += 1;
            }
            let run = &bytes[start..index];
            if let Some(first_letter) = run.iter().position(u8::is_ascii_alphabetic)
                && run.len() - first_letter > symbol_length
            {
                return true;
            }
        } else {
            index += 1;
        }
    }
    false
}

fn line_is_short(extension: &str, line: &str, maxlen: usize) -> bool {
    let limit = match extension {
        "java" => 100,
        "mk" => 200,
        _ => maxlen,
    };
    let length = line.chars().count();
    if length <= limit {
        return true;
    }
    let trimmed = line.trim_start();
    let then_change = "LINT.ThenChange(";
    let exceptions: &[&str] = match extension {
        "c" | "cc" => &["#define", "#endif", "#if", "#include", "#pragma"],
        "html" => &["<g ", "<link ", "<path "],
        "java" => &["import ", "package "],
        "js" => &["GEN('#include", "import "],
        "ts" => &["import ", "export {", "export type {"],
        "h" | "m" | "mm" => &["#define", "#endif", "#if", "#import", "#include", "#pragma"],
        "py" => &["import", "from"],
        "star" => &[""],
        "rs" => return true,
        _ => &[],
    };
    let comment = if extension == "py" { "# " } else { "// " };
    if exceptions.iter().any(|prefix| trimmed.starts_with(prefix))
        || (matches!(extension, "c" | "cc" | "h" | "m" | "mm" | "java" | "js" | "ts" | "mojom") || extension == "py")
            && trimmed.starts_with(&format!("{comment}{then_change}"))
    {
        return true;
    }
    if ["file://", "http://", "https://"].iter().any(|url| line.contains(url)) {
        return true;
    }
    if line.contains("presubmit: ignore-long-line") {
        return true;
    }
    if length * 2 > limit * 3 {
        return false;
    }
    if line.contains("url(") && extension == "css" {
        return true;
    }
    if line.contains("<include") && matches!(extension, "css" | "html" | "js") {
        return true;
    }
    long_symbol(line, limit * 2 / 3)
}

pub fn check_long_lines(changed: &[Affected], maxlen: usize) -> Vec<Finding> {
    let mut out = Vec::new();
    for file in files(changed) {
        let extension = extension(&file.path);
        for (number, text) in &file.added {
            if !line_is_short(extension, text, maxlen) {
                out.push(Finding {
                    check: "CheckLongLines",
                    message: "Found lines longer than the limit in:",
                    location: format!("{}:{number}", file.path),
                });
            }
        }
    }
    out
}

/// Every canned check, in the order Chromium's `PRESUBMIT.py` runs them.
pub fn run_canned(changed: &[Affected], maxlen: usize) -> Vec<Finding> {
    let mut out = check_no_cr_and_one_eol(changed);
    out.extend(check_no_tabs(changed));
    out.extend(check_no_stray_whitespace(changed));
    out.extend(check_long_lines(changed, maxlen));
    out
}

/// Runs the real `presubmit_support.py` on the change in `repo`. `depot_tools` is a checkout of depot_tools.
pub fn run_python(repo: &Path, depot_tools: &Path, upstream: &str, files: &[String]) -> Result<i32> {
    let script = depot_tools.join("presubmit_support.py");
    if !script.is_file() {
        return Err(Error::message(format!("{}: no presubmit_support.py", depot_tools.display())));
    }
    let (program, mut prefix): (String, Vec<String>) = if which("uv") {
        ("uv".to_owned(), vec!["run".into(), "--no-project".into(), "python".into()])
    } else {
        ("python3".to_owned(), Vec::new())
    };
    prefix.push(script.display().to_string());
    prefix.extend(["--root".into(), repo.display().to_string(), "--upstream".into(), upstream.to_owned(), "--upload".into()]);
    if !files.is_empty() {
        prefix.push("--files".into());
        prefix.extend(files.iter().cloned());
    }
    let status = Command::new(&program)
        .args(&prefix)
        .current_dir(repo)
        .env("DEPOT_TOOLS_UPDATE", "0")
        .status()
        .map_err(|source| Error::io(Path::new(&program), source))?;
    Ok(status.code().unwrap_or(1))
}

fn which(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|directory| directory.join(program).is_file() || directory.join(format!("{program}.exe")).is_file())
    })
}

/// The depot_tools checkout to use: `--depot-tools`, `$DEPOT_TOOLS`, `<repo>/third_party/depot_tools`.
pub fn find_depot_tools(repo: &Path, explicit: Option<&str>) -> Option<PathBuf> {
    explicit
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("DEPOT_TOOLS").map(PathBuf::from))
        .or_else(|| Some(repo.join("third_party/depot_tools")))
        .filter(|path| path.join("presubmit_support.py").is_file())
}

/// Writes the findings the way presubmit prints them: a heading and the locations under it.
pub fn render(findings: &[Finding]) -> String {
    let mut out = String::new();
    let mut heading = "";
    for finding in findings {
        if finding.message != heading {
            heading = finding.message;
            out.push_str(&format!("{heading}\n"));
        }
        out.push_str(&format!("  {}\n", finding.location));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, added: &[&str], contents: Option<&str>) -> Affected {
        Affected {
            path: path.to_owned(),
            contents: contents.map(|text| text.as_bytes().to_vec()),
            added: added.iter().enumerate().map(|(index, text)| (index + 1, (*text).to_owned())).collect(),
        }
    }

    #[test]
    fn the_source_filter_skips_third_party_and_caps_files() {
        assert!(is_source_file("base/a.cc"));
        assert!(!is_source_file("third_party/foo/a.cc"));
        assert!(is_source_file("third_party/blink/a.cc"));
        assert!(!is_source_file("README"));
        assert!(!is_source_file("out/Debug/a.py"));
        assert!(!is_source_file("a.txt"));
    }

    #[test]
    fn canned_checks_find_what_they_name() {
        let changed = [file("a.cc", &["ok", "\tbad", "trail  "], Some("ok\n\tbad\ntrail  \n\n"))];
        let found = run_canned(&changed, 80);
        let by = |check: &str| found.iter().filter(|finding| finding.check == check).count();
        assert_eq!(by("CheckChangeHasNoTabs"), 1);
        assert_eq!(by("CheckChangeHasNoStrayWhitespace"), 1);
        assert_eq!(by("CheckChangeHasOnlyOneEol"), 1);
        assert_eq!(by("CheckLongLines"), 0);
    }

    #[test]
    fn long_lines_follow_the_exceptions() {
        let long = "x".repeat(200);
        assert!(!line_is_short("cc", &format!("int {long};"), 80));
        assert!(line_is_short("cc", &format!("#include \"{long}\""), 80));
        assert!(line_is_short("cc", &format!("// see https://x/{long}"), 80));
        assert!(line_is_short("rs", &format!("let {long} = 1;"), 80));
        let spaced = "word ".repeat(20);
        assert!(!line_is_short("cc", &spaced, 80));
    }
}
