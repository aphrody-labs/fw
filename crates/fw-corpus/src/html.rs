//! HTML page to (title, markdown): the main/article region only, boilerplate tags dropped.

use anyhow::{Result, anyhow};

const SKIP: &[&str] = &["script", "style", "svg", "noscript", "nav", "footer", "header", "aside", "button", "template", "iframe", "form", "mat-icon"];

/// Attribute values above this size are framework state (m3.material.io ships its whole token set as one 26 MB attribute), not content.
const MAX_ATTRIBUTE: usize = 20_000;

/// Lines the Material and DevSite page chrome leaves behind after conversion.
const CHROME_LINES: &[&str] = &["link", "Copy link", "Link copied", "Copy linkLink copied"];

/// Drop attribute values larger than `MAX_ATTRIBUTE` so the converter never sees them.
pub fn strip_huge_attributes(html: &str) -> String {
    let mut out = String::with_capacity(html.len().min(1 << 20));
    let mut rest = html;
    while let Some(at) = rest.find("=\"") {
        let value_start = at + 2;
        let Some(length) = rest[value_start..].find('"') else { break };
        if length > MAX_ATTRIBUTE {
            out.push_str(&rest[..at]);
            out.push_str("=\"\"");
        } else {
            out.push_str(&rest[..value_start + length + 1]);
        }
        rest = &rest[value_start + length + 1..];
    }
    out.push_str(rest);
    out
}

fn region<'a>(html: &'a str, tag: &str) -> Option<&'a str> {
    let open = html.find(&format!("<{tag}")).filter(|at| {
        html[at + tag.len() + 1..].starts_with([' ', '>', '\n', '\t'])
    })?;
    let close = html.rfind(&format!("</{tag}>")).filter(|close| *close > open)?;
    Some(&html[open..close + tag.len() + 3])
}

pub fn title(html: &str) -> String {
    let inner = |tag: &str| {
        let start = html.find(&format!("<{tag}"))?;
        let from = html[start..].find('>')? + start + 1;
        let end = html[from..].find(&format!("</{tag}>"))? + from;
        Some(crate::unescape(strip_tags(&html[from..end]).trim()))
    };
    inner("h1").filter(|title| !title.is_empty()).or_else(|| inner("title")).unwrap_or_default()
}

fn strip_tags(fragment: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for character in fragment.chars() {
        match character {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => out.push(character),
            _ => {}
        }
    }
    out
}

pub fn to_markdown(html: &str) -> Result<(String, String)> {
    let page_title = title(html);
    let body = region(html, "main").or_else(|| region(html, "article")).or_else(|| region(html, "body")).unwrap_or(html);
    let converter = htmd::HtmlToMarkdown::builder().skip_tags(SKIP.to_vec()).build();
    let markdown = converter.convert(body).map_err(|error| anyhow!("html to markdown: {error}"))?;
    let mut out = String::with_capacity(markdown.len());
    let mut blank = 0;
    for line in markdown.lines() {
        let line = line.trim_end();
        let line = if CHROME_LINES.contains(&line.trim()) { "" } else { line };
        blank = if line.is_empty() { blank + 1 } else { 0 };
        if blank <= 1 {
            out.push_str(line);
            out.push('\n');
        }
    }
    Ok((page_title, out.trim().replace("&#10;", " ").to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_chrome_and_huge_attributes_are_dropped() {
        let huge = "x".repeat(MAX_ATTRIBUTE + 1);
        let html = format!("<main><h1>Buttons</h1><div data-x=\"{huge}\"><mat-icon>info</mat-icon><div>link</div><p>Copy link</p><p>Real text</p></div></main>");
        let stripped = strip_huge_attributes(&html);
        assert!(stripped.len() < html.len() / 2);
        let (_, markdown) = to_markdown(&stripped).unwrap();
        assert!(markdown.contains("Real text"));
        assert!(!markdown.contains("Copy link") && !markdown.contains("info"));
        assert!(!markdown.lines().any(|line| line.trim() == "link"));
    }

    #[test]
    fn main_region_only() {
        let html = "<html><head><title>T</title></head><body><nav>menu</nav><main><h1>Buttons</h1><p>Use <b>filled</b> buttons.</p><script>x()</script></main><footer>f</footer></body></html>";
        let (title, markdown) = to_markdown(html).unwrap();
        assert_eq!(title, "Buttons");
        assert!(markdown.contains("Use **filled** buttons."));
        assert!(!markdown.contains("menu") && !markdown.contains("x()"));
    }
}
