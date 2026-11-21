//! Render a JavaScript SPA with headless Chrome and return the final DOM.

use anyhow::{Context, Result, bail};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub fn chrome_binary() -> String {
    for name in ["google-chrome", "chromium", "chromium-browser", "chrome"] {
        if Command::new(name).arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|status| status.success()) {
            return name.to_owned();
        }
    }
    "google-chrome".to_owned()
}

pub fn dump_dom(binary: &str, url: &str, virtual_time_ms: u32) -> Result<String> {
    let profile = std::env::temp_dir().join(format!("fw-corpus-chrome-{}-{:x}", std::process::id(), url.len() ^ (crate::now_unix() as usize)));
    let mut child = Command::new(binary)
        .args(["--headless=new", "--no-sandbox", "--disable-gpu", "--disable-dev-shm-usage", "--hide-scrollbars", "--mute-audio"])
        .arg(format!("--user-data-dir={}", profile.display()))
        .arg(format!("--virtual-time-budget={virtual_time_ms}"))
        .arg(format!("--user-agent={}", crate::http::USER_AGENT))
        .args(["--dump-dom", url])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("spawn {binary}"))?;
    let mut stdout = child.stdout.take().context("chrome stdout")?;
    let reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        std::io::Read::read_to_end(&mut stdout, &mut buffer).map(|_| buffer)
    });
    let deadline = Instant::now() + Duration::from_secs(120);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() > deadline {
            child.kill().ok();
            child.wait().ok();
            std::fs::remove_dir_all(&profile).ok();
            bail!("{url}: chrome timed out");
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let buffer = reader.join().map_err(|_| anyhow::anyhow!("chrome reader panicked"))??;
    std::fs::remove_dir_all(&profile).ok();
    if !status.success() {
        bail!("{url}: chrome exit {status}");
    }
    Ok(String::from_utf8_lossy(&buffer).into_owned())
}
