//! HTML page to (title, markdown): the main/article region only, boilerplate tags dropped.

use anyhow::{Result, anyhow};

const SKIP: &[&str] = &["script", "style", "svg", "noscript", "nav", "footer", "header", "aside", "button", "template", "iframe", "form"];

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
        blank = if line.is_empty() { blank + 1 } else { 0 };
        if blank <= 1 {
            out.push_str(line);
            out.push('\n');
        }
    }
    Ok((page_title, out.trim().to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_region_only() {
        let html = "<html><head><title>T</title></head><body><nav>menu</nav><main><h1>Buttons</h1><p>Use <b>filled</b> buttons.</p><script>x()</script></main><footer>f</footer></body></html>";
        let (title, markdown) = to_markdown(html).unwrap();
        assert_eq!(title, "Buttons");
        assert!(markdown.contains("Use **filled** buttons."));
        assert!(!markdown.contains("menu") && !markdown.contains("x()"));
    }
}
