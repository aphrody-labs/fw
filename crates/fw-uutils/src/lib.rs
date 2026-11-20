//! `fw-uutils` — 100% pure-Rust, zero-C-dependency multicall userland modeled on
//! `github.com/orgs/uutils` (`coreutils`, `findutils`, `grep`, `diffutils`, `sed`, `util-linux`, `procps`),
//! Alpine Linux `edge/community/uutils-coreutils`, and `uutils.org/playground` (Wasm32).

#![forbid(unsafe_code)]

pub mod fast_cli;
pub mod gnu_cli;
pub mod hardened;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SUPPORTED_APPLETS: &[&str] = &[
    "echo", "cat", "wc", "head", "tail", "sha256sum", "uname", "pwd", "env",
    "ls", "grep", "diff", "sed", "find", "hostname", "true", "false",
    "sort", "uniq", "cut", "tr", "nl", "rev", "seq", "basename", "dirname",
    "realpath", "stat", "mkdir", "touch", "cp", "mv", "rm", "base64", "which",
    "du", "tokei", "jq", "bat", "sh",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppletOutput {
    pub applet: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Execute a pure-Rust uutils applet (safe for native CLI, Alpine/NixOS multicall, and Wasm32 playground).
pub fn run_applet(applet: &str, args: &[&str], stdin: &str) -> AppletOutput {
    match applet {
        "echo" => AppletOutput {
            applet: "echo".into(),
            exit_code: 0,
            stdout: format!("{}\n", args.join(" ")),
            stderr: String::new(),
        },
        "true" => AppletOutput {
            applet: "true".into(),
            exit_code: 0,
            stdout: String::new(),
            stderr: String::new(),
        },
        "false" => AppletOutput {
            applet: "false".into(),
            exit_code: 1,
            stdout: String::new(),
            stderr: String::new(),
        },
        "uname" => {
            let text = if args.contains(&"-a") {
                format!(
                    "FwOS-Asterinas 4.0.0 {} {} pure-rust-uutils\n",
                    std::env::consts::OS,
                    std::env::consts::ARCH
                )
            } else {
                "FwOS-Asterinas\n".to_string()
            };
            AppletOutput {
                applet: "uname".into(),
                exit_code: 0,
                stdout: text,
                stderr: String::new(),
            }
        }
        "hostname" => AppletOutput {
            applet: "hostname".into(),
            exit_code: 0,
            stdout: "fw-os\n".into(),
            stderr: String::new(),
        },
        "cat" => {
            if let Some(path) = args.first() {
                match std::fs::read_to_string(path) {
                    Ok(content) => AppletOutput {
                        applet: "cat".into(),
                        exit_code: 0,
                        stdout: content,
                        stderr: String::new(),
                    },
                    Err(err) => AppletOutput {
                        applet: "cat".into(),
                        exit_code: 1,
                        stdout: String::new(),
                        stderr: format!("cat: {path}: {err}\n"),
                    },
                }
            } else {
                AppletOutput {
                    applet: "cat".into(),
                    exit_code: 0,
                    stdout: stdin.to_string(),
                    stderr: String::new(),
                }
            }
        }
        "wc" => {
            let lines = stdin.lines().count();
            let words = stdin.split_whitespace().count();
            let bytes = stdin.len();
            AppletOutput {
                applet: "wc".into(),
                exit_code: 0,
                stdout: format!("{lines} {words} {bytes}\n"),
                stderr: String::new(),
            }
        }
        "head" => {
            let n: usize = args
                .windows(2)
                .find(|w| w[0] == "-n")
                .and_then(|w| w[1].parse().ok())
                .unwrap_or(10);
            let out: Vec<&str> = stdin.lines().take(n).collect();
            AppletOutput {
                applet: "head".into(),
                exit_code: 0,
                stdout: if out.is_empty() { String::new() } else { format!("{}\n", out.join("\n")) },
                stderr: String::new(),
            }
        }
        "tail" => {
            let n: usize = args
                .windows(2)
                .find(|w| w[0] == "-n")
                .and_then(|w| w[1].parse().ok())
                .unwrap_or(10);
            let all: Vec<&str> = stdin.lines().collect();
            let start = all.len().saturating_sub(n);
            let slice = &all[start..];
            AppletOutput {
                applet: "tail".into(),
                exit_code: 0,
                stdout: if slice.is_empty() { String::new() } else { format!("{}\n", slice.join("\n")) },
                stderr: String::new(),
            }
        }
        "sha256sum" => {
            const HEX: &[u8; 16] = b"0123456789abcdef";
            let raw = Sha256::digest(stdin.as_bytes());
            let mut digest = String::with_capacity(64);
            for b in raw {
                digest.push(HEX[(b >> 4) as usize] as char);
                digest.push(HEX[(b & 0x0f) as usize] as char);
            }
            AppletOutput {
                applet: "sha256sum".into(),
                exit_code: 0,
                stdout: format!("{digest}  -\n"),
                stderr: String::new(),
            }
        }
        "grep" => {
            let pat = args.first().copied().unwrap_or("");
            let matched: Vec<&str> = stdin.lines().filter(|l| l.contains(pat)).collect();
            AppletOutput {
                applet: "grep".into(),
                exit_code: if matched.is_empty() { 1 } else { 0 },
                stdout: if matched.is_empty() { String::new() } else { format!("{}\n", matched.join("\n")) },
                stderr: String::new(),
            }
        }
        "sed" => {
            // Supports basic `s/from/to/g` substitution
            let expr = args.first().copied().unwrap_or("");
            let out = if let Some(rest) = expr.strip_prefix("s/") {
                let parts: Vec<&str> = rest.split('/').collect();
                if parts.len() >= 2 {
                    stdin.replace(parts[0], parts[1])
                } else {
                    stdin.to_string()
                }
            } else {
                stdin.to_string()
            };
            AppletOutput {
                applet: "sed".into(),
                exit_code: 0,
                stdout: out,
                stderr: String::new(),
            }
        }
        "diff" => {
            let left = args.first().copied().unwrap_or("");
            let right = args.get(1).copied().unwrap_or("");
            let same = left == right;
            AppletOutput {
                applet: "diff".into(),
                exit_code: if same { 0 } else { 1 },
                stdout: if same { String::new() } else { format!("-{left}\n+{right}\n") },
                stderr: String::new(),
            }
        }
        "pwd" => {
            let cwd = std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| "/".into());
            AppletOutput {
                applet: "pwd".into(),
                exit_code: 0,
                stdout: format!("{cwd}\n"),
                stderr: String::new(),
            }
        }
        "ls" => {
            let dir = args.first().copied().unwrap_or(".");
            match std::fs::read_dir(dir) {
                Ok(entries) => {
                    let mut names: Vec<String> = entries
                        .flatten()
                        .map(|e| e.file_name().to_string_lossy().to_string())
                        .collect();
                    names.sort();
                    AppletOutput {
                        applet: "ls".into(),
                        exit_code: 0,
                        stdout: format!("{}\n", names.join("\n")),
                        stderr: String::new(),
                    }
                }
                Err(err) => AppletOutput {
                    applet: "ls".into(),
                    exit_code: 1,
                    stdout: String::new(),
                    stderr: format!("ls: {dir}: {err}\n"),
                },
            }
        }
        other => {
            if let Some(res) = gnu_cli::run_extended_applet(other, args, stdin) {
                return res;
            }
            AppletOutput {
                applet: applet.into(),
                exit_code: 127,
                stdout: String::new(),
                stderr: format!("uutils: unknown applet '{applet}'\n"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coreutils_grep_sed_sha256_and_uname_execute_pure_rust() {
        assert_eq!(run_applet("echo", &["fw", "uutils"], "").stdout, "fw uutils\n");
        assert_eq!(run_applet("wc", &[], "a b\nc d e\n").stdout, "2 5 10\n");
        assert_eq!(run_applet("grep", &["rust"], "alpine\npure rust\n").stdout, "pure rust\n");
        assert_eq!(run_applet("sed", &["s/gnu/uutils/g"], "gnu coreutils").stdout, "uutils coreutils");
        let h = run_applet("sha256sum", &[], "fw");
        assert_eq!(h.exit_code, 0);
        assert_eq!(h.stdout.split_whitespace().next().unwrap().len(), 64);
        assert!(run_applet("uname", &["-a"], "").stdout.contains("FwOS-Asterinas"));
    }
}
