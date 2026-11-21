use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::io::Write;
use std::process::{Command, Stdio};

pub const WEBSTORE_UPDATE_URL: &str = "https://clients2.google.com/service/update2/crx";

#[derive(Debug, Clone, Deserialize)]
pub struct Apply {
    pub nodes: Vec<String>,
    pub linux_dirs: Vec<String>,
    pub linux_file: String,
    pub windows_key: String,
    #[serde(default)]
    pub windows_reset: bool,
    pub windows_out: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Extension {
    pub name: String,
    pub id: String,
    pub source: String,
    #[serde(default)]
    pub update_url: String,
    pub state: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SiteSearch {
    pub name: String,
    pub shortcut: String,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BrowserConfig {
    pub apply: Apply,
    pub policies: toml::Table,
    #[serde(default, rename = "extension")]
    pub extensions: Vec<Extension>,
    #[serde(default, rename = "search")]
    pub searches: Vec<SiteSearch>,
}

impl BrowserConfig {
    pub fn parse(text: &str, brand_seed: &str) -> Result<Self> {
        let config: BrowserConfig = toml::from_str(&text.replace("${brand.seed}", brand_seed)).context("parse chrome.toml")?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        for extension in &self.extensions {
            match extension.state.as_str() {
                "pending" => {}
                "active" => {
                    if extension.id.len() != 32 || !extension.id.bytes().all(|byte| byte.is_ascii_lowercase()) {
                        bail!("extension {}: id must be 32 lowercase letters", extension.name);
                    }
                    match extension.source.as_str() {
                        "webstore" => {}
                        "packaged" if !extension.update_url.is_empty() => {}
                        other => bail!("extension {}: source {other} needs an update_url (or use webstore)", extension.name),
                    }
                }
                other => bail!("extension {}: state must be active or pending, got {other}", extension.name),
            }
        }
        for search in &self.searches {
            if !search.url.contains("{searchTerms}") {
                bail!("search {}: url must contain {{searchTerms}}", search.name);
            }
        }
        Ok(())
    }

    pub fn policy_map(&self) -> Result<Map<String, Value>> {
        let mut policies: Map<String, Value> = Map::new();
        for (name, value) in &self.policies {
            policies.insert(name.clone(), serde_json::to_value(value)?);
        }
        let mut settings = Map::new();
        for extension in self.extensions.iter().filter(|extension| extension.state == "active") {
            let update_url = if extension.source == "webstore" { WEBSTORE_UPDATE_URL } else { extension.update_url.as_str() };
            settings.insert(
                extension.id.clone(),
                json!({ "installation_mode": "force_installed", "update_url": update_url, "toolbar_pin": "force_pinned" }),
            );
        }
        if !settings.is_empty() {
            policies.insert("ExtensionSettings".to_string(), Value::Object(settings));
        }
        if !self.searches.is_empty() {
            let entries: Vec<Value> = self
                .searches
                .iter()
                .map(|search| json!({ "name": search.name, "shortcut": search.shortcut, "url": search.url }))
                .collect();
            policies.insert("SiteSearchSettings".to_string(), Value::Array(entries));
        }
        Ok(policies)
    }

    pub fn linux_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(&Value::Object(self.policy_map()?))? + "\n")
    }

    pub fn windows_reg(&self) -> Result<String> {
        let root = format!("HKEY_LOCAL_MACHINE\\{}", self.apply.windows_key);
        let mut out = String::from("Windows Registry Editor Version 5.00\r\n\r\n");
        if self.apply.windows_reset {
            out.push_str(&format!("[-{root}]\r\n\r\n"));
        }
        out.push_str(&format!("[{root}]\r\n"));
        let mut subkeys = String::new();
        for (name, value) in self.policy_map()? {
            match &value {
                Value::Bool(flag) => out.push_str(&format!("\"{name}\"=dword:{:08x}\r\n", u32::from(*flag))),
                Value::Number(number) => {
                    let integer = number.as_u64().and_then(|n| u32::try_from(n).ok()).ok_or_else(|| anyhow!("{name}: registry integers are unsigned 32-bit"))?;
                    out.push_str(&format!("\"{name}\"=dword:{integer:08x}\r\n"));
                }
                Value::String(text) => out.push_str(&format!("\"{name}\"=\"{}\"\r\n", reg_escape(text))),
                Value::Array(items) if items.iter().all(|item| item.is_string() || item.is_number()) => {
                    subkeys.push_str(&format!("\r\n[{root}\\{name}]\r\n"));
                    for (index, item) in items.iter().enumerate() {
                        let text = item.as_str().map(str::to_string).unwrap_or_else(|| item.to_string());
                        subkeys.push_str(&format!("\"{}\"=\"{}\"\r\n", index + 1, reg_escape(&text)));
                    }
                }
                Value::Array(_) | Value::Object(_) => out.push_str(&format!("\"{name}\"=\"{}\"\r\n", reg_escape(&serde_json::to_string(&value)?))),
                Value::Null => bail!("{name}: null has no registry form"),
            }
        }
        out.push_str(&subkeys);
        Ok(out)
    }

    pub fn linux_paths(&self) -> Vec<String> {
        self.apply.linux_dirs.iter().map(|directory| format!("{}/{}", directory.trim_end_matches('/'), self.apply.linux_file)).collect()
    }
}

fn reg_escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

#[derive(Debug, Clone, Deserialize)]
pub struct Host {
    pub ssh: String,
    #[serde(default)]
    pub aliases: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct BuildCluster {
    hosts: std::collections::BTreeMap<String, Host>,
}

pub fn ssh_target(build_toml: &str, node: &str) -> Result<String> {
    let cluster: BuildCluster = toml::from_str(build_toml).context("parse build.toml")?;
    let host = cluster.hosts.get(node).ok_or_else(|| anyhow!("node {node} is not in build.toml [hosts]"))?;
    let resolved = Command::new("ssh").args(["-G", &host.ssh]).output().ok().map(|output| String::from_utf8_lossy(&output.stdout).into_owned()).unwrap_or_default();
    let configured_hostname = resolved.lines().find_map(|line| line.strip_prefix("hostname "));
    match (configured_hostname, host.aliases.first()) {
        (Some(hostname), _) if hostname != host.ssh => Ok(host.ssh.clone()),
        (_, Some(alias)) => Ok(alias.clone()),
        _ => Ok(host.ssh.clone()),
    }
}

pub fn ssh(target: &str, script: &str, stdin: Option<&str>) -> Result<String> {
    let mut child = Command::new("ssh")
        .args(["-o", "BatchMode=yes", target, script])
        .stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("spawn ssh {target}"))?;
    if let (Some(content), Some(mut pipe)) = (stdin, child.stdin.take()) {
        pipe.write_all(content.as_bytes())?;
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        bail!("ssh {target}: {}", String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn install_script(config: &BrowserConfig) -> String {
    let writes: Vec<String> = config
        .apply
        .linux_dirs
        .iter()
        .zip(config.linux_paths())
        .map(|(directory, path)| format!("sudo install -d -m 755 '{}' && sudo cp \"$staged\" '{path}' && sudo chmod 644 '{path}'", directory.trim_end_matches('/')))
        .collect();
    format!("staged=$(mktemp) && cat > \"$staged\" && {} ; status=$?; rm -f \"$staged\"; exit $status", writes.join(" && "))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r##"
[apply]
nodes = ["n1"]
linux_dirs = ["/etc/opt/chrome/policies/managed/", "/etc/chromium/policies/managed"]
linux_file = "aphrody.json"
windows_key = 'SOFTWARE\Policies\Google\Chrome'
windows_reset = true
windows_out = "dist/chrome/a.reg"

[policies]
HardwareAccelerationModeEnabled = true
NetworkPredictionOptions = 0
HomepageLocation = "https://aphrody.com/"
BrowserThemeColor = "${brand.seed}"
RestoreOnStartupURLs = ["https://a.example/", "https://b.example/"]

[[extension]]
name = "Bitwarden"
id = "nngceckbapebfimnlniiiahkandclblb"
source = "webstore"
state = "active"

[[extension]]
name = "fw"
id = ""
source = "packaged"
state = "pending"

[[search]]
name = "crates.io"
shortcut = "crates"
url = "https://crates.io/search?q={searchTerms}"
"##;

    fn sample() -> BrowserConfig {
        BrowserConfig::parse(SAMPLE, "#e2dbad").expect("sample parses")
    }

    #[test]
    fn linux_json_carries_policies_extensions_and_searches() {
        let value: Value = serde_json::from_str(&sample().linux_json().expect("render")).expect("json");
        assert_eq!(value["HardwareAccelerationModeEnabled"], json!(true));
        assert_eq!(value["NetworkPredictionOptions"], json!(0));
        assert_eq!(value["BrowserThemeColor"], json!("#e2dbad"));
        let settings = &value["ExtensionSettings"]["nngceckbapebfimnlniiiahkandclblb"];
        assert_eq!(settings["installation_mode"], json!("force_installed"));
        assert_eq!(settings["update_url"], json!(WEBSTORE_UPDATE_URL));
        assert_eq!(value["ExtensionSettings"].as_object().map(Map::len), Some(1), "pending extension is not installed");
        assert_eq!(value["SiteSearchSettings"][0]["shortcut"], json!("crates"));
    }

    #[test]
    fn windows_reg_uses_dword_string_subkey_and_json_forms() {
        let reg = sample().windows_reg().expect("render");
        assert!(reg.starts_with("Windows Registry Editor Version 5.00\r\n\r\n[-HKEY_LOCAL_MACHINE\\SOFTWARE\\Policies\\Google\\Chrome]"));
        assert!(reg.contains("\"HardwareAccelerationModeEnabled\"=dword:00000001\r\n"));
        assert!(reg.contains("\"NetworkPredictionOptions\"=dword:00000000\r\n"));
        assert!(reg.contains("\"HomepageLocation\"=\"https://aphrody.com/\"\r\n"));
        assert!(reg.contains("[HKEY_LOCAL_MACHINE\\SOFTWARE\\Policies\\Google\\Chrome\\RestoreOnStartupURLs]\r\n\"1\"=\"https://a.example/\"\r\n\"2\"=\"https://b.example/\"\r\n"));
        assert!(reg.contains("\"ExtensionSettings\"=\"{\\\"nngceckbapebfimnlniiiahkandclblb\\\":{"));
    }

    #[test]
    fn active_extension_needs_a_valid_id_and_pending_entries_may_be_empty() {
        let broken = SAMPLE.replace("nngceckbapebfimnlniiiahkandclblb", "short");
        assert!(BrowserConfig::parse(&broken, "#000000").is_err());
        let no_terms = SAMPLE.replace("{searchTerms}", "x");
        assert!(BrowserConfig::parse(&no_terms, "#000000").is_err());
    }

    #[test]
    fn install_script_creates_the_directory_and_writes_the_file() {
        let script = install_script(&sample());
        assert!(script.contains("install -d -m 755 '/etc/opt/chrome/policies/managed'"));
        assert!(script.contains("cp \"$staged\" '/etc/chromium/policies/managed/aphrody.json'"));
    }
}
