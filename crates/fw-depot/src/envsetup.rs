//! `build/envsetup.sh` and `lunch` without the shell and without Soong: the variables a lunch sets, computed from the arguments and from the files
//! of the tree. What `lunch` gets by running `soong_ui --dumpvars-mode` (the full variable set of the build) is out of reach without a built
//! Soong; what the shell script itself derives is complete: the top of the tree, `TARGET_PRODUCT`, `TARGET_RELEASE`, `TARGET_BUILD_VARIANT`,
//! `TARGET_BUILD_TYPE`, `ANDROID_BUILD_TOP`, `BUILD_ENV_SEQUENCE_NUMBER`, the out directories and the `PATH` entries that follow from them.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

const TOP_FILE: &str = "build/make/core/envsetup.mk";

/// `gettop`: the nearest ancestor of `start` (or `$TOP`) that holds `build/make/core/envsetup.mk`.
pub fn gettop(start: &Path, top_env: Option<&str>) -> Option<PathBuf> {
    if let Some(top) = top_env.filter(|top| Path::new(top).join(TOP_FILE).is_file()) {
        return std::fs::canonicalize(top).ok();
    }
    start
        .ancestors()
        .find(|directory| directory.join(TOP_FILE).is_file())
        .map(Path::to_path_buf)
}

/// `getoutdir`: `$OUT_DIR`, else `$OUT_DIR_COMMON_BASE/<top name>`, else `out`; relative to the top.
pub fn getoutdir(top: &Path, out_dir: Option<&str>, common_base: Option<&str>) -> PathBuf {
    let chosen = match (out_dir.filter(|value| !value.is_empty()), common_base.filter(|value| !value.is_empty())) {
        (Some(dir), _) => dir.to_owned(),
        (None, Some(base)) => format!(
            "{}/{}",
            base,
            top.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
        ),
        (None, None) => "out".to_owned(),
    };
    if Path::new(&chosen).is_absolute() {
        PathBuf::from(chosen)
    } else {
        top.join(chosen)
    }
}

/// `product release variant` as `lunch` parses them: `lunch product [release [variant]]` or the legacy `lunch product-release-variant`.
pub fn parse_lunch(arguments: &[String]) -> Result<(String, String, String)> {
    match arguments {
        [] => Err(Error::message("No target specified. See lunch --help")),
        more if more.len() > 3 => Err(Error::message("Too many parameters given. See lunch --help")),
        [single] if single.contains('-') => {
            let mut parts = single.splitn(3, '-');
            let (product, release, variant) = (
                parts.next().unwrap_or_default(),
                parts.next().unwrap_or_default(),
                parts.next().unwrap_or_default(),
            );
            if product.is_empty() || release.is_empty() || variant.is_empty() {
                return Err(Error::message(format!(
                    "Invalid lunch combo: {single}\nValid combos must be of the form <product>-<release>-<variant> when using\nthe legacy format.  Run 'lunch --help' for usage."
                )));
            }
            Ok((product.to_owned(), release.to_owned(), variant.to_owned()))
        }
        [product, rest @ ..] => Ok((
            product.clone(),
            rest.first().cloned().unwrap_or_else(|| "trunk_staging".to_owned()),
            rest.get(1).cloned().unwrap_or_else(|| "eng".to_owned()),
        )),
    }
}

/// The host tag of prebuilts: `linux-x86` or `darwin-x86`.
pub fn host_prebuilt_tag() -> &'static str {
    if cfg!(target_os = "macos") { "darwin-x86" } else { "linux-x86" }
}

/// `BUILD_ENV_SEQUENCE_NUMBER` as the tree's own `envsetup.sh` sets it.
fn sequence_number(top: &Path) -> Option<String> {
    let text = std::fs::read_to_string(top.join("build/envsetup.sh"))
        .or_else(|_| std::fs::read_to_string(top.join("build/make/envsetup.sh")))
        .ok()?;
    text.lines()
        .find_map(|line| line.trim().strip_prefix("export BUILD_ENV_SEQUENCE_NUMBER="))
        .map(|value| value.trim().to_owned())
}

/// `PRODUCT_DEVICE` of the product's makefile, found by name under the product directories.
pub fn product_device(top: &Path, product: &str) -> Option<String> {
    let roots = ["build/make/target/product", "device", "vendor"];
    for root in roots {
        let base = top.join(root);
        let mut stack = vec![base];
        while let Some(directory) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&directory) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if path.file_name().is_some_and(|name| name != ".git") {
                        stack.push(path);
                    }
                    continue;
                }
                if path.file_name().is_some_and(|name| name.to_string_lossy() == format!("{product}.mk"))
                    && let Ok(text) = std::fs::read_to_string(&path)
                    && let Some(device) = text.lines().find_map(|line| {
                        let line = line.trim();
                        let rest = line.strip_prefix("PRODUCT_DEVICE")?;
                        let value = rest.trim_start().strip_prefix(":=").or_else(|| rest.trim_start().strip_prefix('='))?;
                        Some(value.trim().to_owned())
                    })
                {
                    return Some(device);
                }
            }
        }
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lunch {
    pub vars: Vec<(String, String)>,
    /// Directories `lunch` puts at the front of `PATH`, in order.
    pub path: Vec<String>,
    /// What a full `lunch` computes with Soong and this could not.
    pub unresolved: Vec<String>,
}

impl Lunch {
    /// `export NAME='value'` lines, with `PATH` last.
    pub fn export_script(&self, current_path: &str) -> String {
        let mut out = String::new();
        for (name, value) in &self.vars {
            out.push_str(&format!("export {name}='{}'\n", value.replace('\'', "'\\''")));
        }
        if !self.path.is_empty() {
            out.push_str(&format!("export PATH='{}:{current_path}'\n", self.path.join(":")));
        }
        out
    }
}

/// The environment of `lunch <arguments>` for the tree at `top`.
pub fn lunch(arguments: &[String], top: &Path, out_dir: Option<&str>, common_base: Option<&str>) -> Result<Lunch> {
    let (product, release, variant) = parse_lunch(arguments)?;
    if !matches!(variant.as_str(), "user" | "userdebug" | "eng") {
        return Err(Error::message(format!(
            "variant must be one of user, userdebug, eng; got {variant}"
        )));
    }
    let out = getoutdir(top, out_dir, common_base);
    let tag = host_prebuilt_tag();
    let host_out = out.join("host").join(tag);
    let mut unresolved = Vec::new();
    let mut vars = vec![
        ("TARGET_PRODUCT".to_owned(), product.clone()),
        ("TARGET_BUILD_VARIANT".to_owned(), variant),
        ("TARGET_RELEASE".to_owned(), release),
        ("TARGET_BUILD_TYPE".to_owned(), "release".to_owned()),
        ("TARGET_BUILD_APPS".to_owned(), String::new()),
        ("ANDROID_BUILD_TOP".to_owned(), top.display().to_string()),
    ];
    match sequence_number(top) {
        Some(number) => vars.push(("BUILD_ENV_SEQUENCE_NUMBER".to_owned(), number)),
        None => unresolved.push("BUILD_ENV_SEQUENCE_NUMBER (build/envsetup.sh not in the tree)".to_owned()),
    }
    match product_device(top, &product) {
        Some(device) => {
            let product_out = out.join("target").join("product").join(&device);
            vars.push(("TARGET_DEVICE".to_owned(), device));
            vars.push(("ANDROID_PRODUCT_OUT".to_owned(), product_out.display().to_string()));
            vars.push(("OUT".to_owned(), product_out.display().to_string()));
        }
        None => unresolved.push(format!(
            "TARGET_DEVICE, ANDROID_PRODUCT_OUT, OUT (no {product}.mk with PRODUCT_DEVICE under build/make/target/product, device or vendor)"
        )),
    }
    vars.push(("ANDROID_HOST_OUT".to_owned(), host_out.display().to_string()));
    vars.push((
        "ANDROID_HOST_OUT_TESTCASES".to_owned(),
        host_out.join("testcases").display().to_string(),
    ));
    let path = vec![
        top.join("build/soong/bin").display().to_string(),
        host_out.join("bin").display().to_string(),
        top.join("prebuilts/asuite/acloud").join(tag).display().to_string(),
        top.join("prebuilts/asuite/aidegen").join(tag).display().to_string(),
        top.join("prebuilts/asuite/atest").join(tag).display().to_string(),
    ];
    unresolved.push("ANDROID_JAVA_HOME, JAVA_HOME, ANDROID_JAVA_TOOLCHAIN, ASAN_SYMBOLIZER_PATH, the clang and Java PATH entries, PRODUCT_* (Soong dumpvars)".to_owned());
    Ok(Lunch { vars, path, unresolved })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_owned()).collect()
    }

    #[test]
    fn lunch_arguments_follow_envsetup_sh() {
        assert_eq!(
            parse_lunch(&args(&["aosp_arm64-trunk_staging-userdebug"])).unwrap(),
            ("aosp_arm64".into(), "trunk_staging".into(), "userdebug".into())
        );
        assert_eq!(
            parse_lunch(&args(&["aosp_arm64"])).unwrap(),
            ("aosp_arm64".into(), "trunk_staging".into(), "eng".into())
        );
        assert_eq!(
            parse_lunch(&args(&["aosp_arm64", "next", "user"])).unwrap(),
            ("aosp_arm64".into(), "next".into(), "user".into())
        );
        assert!(parse_lunch(&args(&["aosp_arm64-eng"])).is_err());
        assert!(parse_lunch(&args(&[])).is_err());
        assert!(parse_lunch(&args(&["a", "b", "c", "d"])).is_err());
    }

    #[test]
    fn out_directory_follows_getoutdir() {
        let top = Path::new("/src/aosp");
        assert_eq!(getoutdir(top, None, None), PathBuf::from("/src/aosp/out"));
        assert_eq!(getoutdir(top, Some("/o"), None), PathBuf::from("/o"));
        assert_eq!(getoutdir(top, None, Some("/o")), PathBuf::from("/o/aosp"));
        assert_eq!(getoutdir(top, Some("rel"), None), PathBuf::from("/src/aosp/rel"));
    }
}
