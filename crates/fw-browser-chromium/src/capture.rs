//! Capture a JS-rendered page as structured text: wait for it to settle, run one extraction script
//! (`capture.js`) and post-process its raw result in Rust (the pure part, unit-tested without an engine).
//!
//! The script only gathers: content blocks of `<main>` in document order, links, the Gemini conversation turns
//! (`user-query` and `model-response` or, on share pages, `response-container`) and the page text. De-duplication (design.google repeats every
//! caption), the credit-line filter ("Source ..." lines under images), turn ordering and the fallbacks live in
//! [`Capture::from_raw`].

use std::collections::HashSet;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

const SCRIPT: &str = include_str!("capture.js");

#[derive(Clone, Debug)]
pub struct CaptureOptions {
    /// Bound for the load and the settle together.
    pub timeout: Duration,
    /// The page is settled once the DOM and its resource requests have been quiet this long.
    pub quiet: Duration,
    /// Also paint a PNG of the viewport.
    pub screenshot: bool,
    pub width: u32,
    pub height: u32,
}

impl Default for CaptureOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            quiet: Duration::from_millis(1500),
            screenshot: false,
            width: 1280,
            height: 720,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Heading {
    pub level: u8,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Link {
    pub href: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Turn {
    /// `"user"` or `"model"`.
    pub role: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Capture {
    pub url: String,
    pub title: String,
    pub lang: String,
    /// Main content in order: the turns when the page has a conversation, else headings and paragraphs, else the
    /// page text. Blank-line separated.
    pub text: String,
    pub headings: Vec<Heading>,
    pub links: Vec<Link>,
    pub turns: Vec<Turn>,
    /// What ended the wait: `stable` (quiet for [`CaptureOptions::quiet`]) or `timeout`.
    pub settled_by: String,
    /// The viewport as PNG; never serialized (the CLI writes it to a file).
    #[serde(skip)]
    pub screenshot_png: Option<Vec<u8>>,
}

/// The expression to evaluate: the script applied to the quiet window and the settle bound.
pub(crate) fn expression(options: &CaptureOptions, settle_bound: Duration) -> String {
    format!(
        "({SCRIPT})({}, {})",
        options.quiet.as_millis(),
        settle_bound.as_millis()
    )
}

/// Collapse runs of whitespace (newlines included) to single spaces.
fn squash(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Keep line breaks, trim every line and drop blank runs.
fn tidy(text: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for line in text.lines() {
        let line = squash(line);
        if line.is_empty() && out.last().is_none_or(String::is_empty) {
            continue;
        }
        out.push(line);
    }
    while out.last().is_some_and(String::is_empty) {
        out.pop();
    }
    out.join("\n")
}

/// An image credit line: `Source`, then a separator or a name, short, and not a sentence.
fn is_credit(text: &str) -> bool {
    let Some(rest) = text.strip_prefix("Source") else {
        return false;
    };
    let starts_word = rest.is_empty() || rest.starts_with(|c: char| !c.is_alphanumeric());
    starts_word && text.chars().count() <= 200 && !text.ends_with('.')
}

fn heading_level(tag: &str) -> Option<u8> {
    let digit = tag.strip_prefix('h')?;
    digit.parse().ok().filter(|n| (1..=6).contains(n))
}

fn string(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(squash)
        .unwrap_or_default()
}

fn items<'a>(value: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

impl Capture {
    /// Build a capture from the raw script result.
    pub fn from_raw(raw: &Value) -> Capture {
        let mut seen = HashSet::new();
        let mut headings = Vec::new();
        let mut paragraphs = Vec::new();
        for block in items(raw, "blocks") {
            let text = string(block, "text");
            if text.is_empty() || is_credit(&text) || !seen.insert(text.clone()) {
                continue;
            }
            let tag = block.get("tag").and_then(Value::as_str).unwrap_or("p");
            if let Some(level) = heading_level(tag) {
                headings.push(Heading {
                    level,
                    text: text.clone(),
                });
            }
            paragraphs.push(text);
        }

        let mut ordered: Vec<(i64, usize, &Value)> = items(raw, "turns")
            .enumerate()
            .map(|(i, t)| {
                (
                    t.get("pos").and_then(Value::as_i64).unwrap_or(i as i64),
                    i,
                    t,
                )
            })
            .collect();
        ordered.sort_by_key(|(pos, i, _)| (*pos, *i));
        let mut turns: Vec<Turn> = Vec::new();
        for (_, _, turn) in ordered {
            let role = match turn.get("role").and_then(Value::as_str) {
                Some(role @ ("user" | "model")) => role,
                _ => continue,
            };
            let text = tidy(turn.get("text").and_then(Value::as_str).unwrap_or(""));
            if text.is_empty()
                || turns
                    .last()
                    .is_some_and(|t| t.role == role && t.text == text)
            {
                continue;
            }
            turns.push(Turn {
                role: role.into(),
                text,
            });
        }

        let mut links = Vec::new();
        let mut seen_links = HashSet::new();
        for link in items(raw, "links") {
            let href = string(link, "href");
            let usable = href.starts_with("http://") || href.starts_with("https://");
            if usable && seen_links.insert(href.clone()) {
                links.push(Link {
                    href,
                    text: string(link, "text"),
                });
            }
        }

        let text = if !turns.is_empty() {
            turns
                .iter()
                .map(|t| format!("{}: {}", t.role, t.text))
                .collect::<Vec<_>>()
                .join("\n\n")
        } else if !paragraphs.is_empty() {
            paragraphs.join("\n\n")
        } else {
            let mut lines = Vec::new();
            for line in raw
                .get("body")
                .and_then(Value::as_str)
                .unwrap_or("")
                .lines()
            {
                let line = squash(line);
                if !line.is_empty() && !is_credit(&line) && seen.insert(line.clone()) {
                    lines.push(line);
                }
            }
            lines.join("\n")
        };

        Capture {
            url: string(raw, "url"),
            title: string(raw, "title"),
            lang: string(raw, "lang"),
            text,
            headings,
            links,
            turns,
            settled_by: string(raw, "settled_by"),
            screenshot_png: None,
        }
    }

    /// Paragraphs of [`Capture::text`] (blank-line separated blocks).
    pub fn paragraph_count(&self) -> usize {
        self.text.split("\n\n").filter(|p| !p.is_empty()).count()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn captions_that_appear_twice_are_kept_once_and_credits_are_dropped() {
        let raw = json!({
            "url": "https://design.google/library/x", "title": " The next  interface ", "lang": "en",
            "blocks": [
                {"tag": "h1", "text": "The next interface"},
                {"tag": "p", "text": "When the first motorized vehicles appeared,\n people walked in front."},
                {"tag": "figcaption", "text": "A red flag man."},
                {"tag": "p", "text": "Source Library of Congress"},
                {"tag": "figcaption", "text": "A red flag man."},
                {"tag": "p", "text": "Source: Getty Images"},
                {"tag": "h2", "text": "Why it matters"},
                {"tag": "p", "text": "Source code is not a credit, it is a sentence about code."},
                {"tag": "p", "text": "   "},
            ],
            "body": "Related\ncards only",
        });
        let capture = Capture::from_raw(&raw);
        assert_eq!(capture.title, "The next interface");
        assert_eq!(
            capture.text,
            "The next interface\n\nWhen the first motorized vehicles appeared, people walked in front.\n\nA red flag man.\n\nWhy it matters\n\nSource code is not a credit, it is a sentence about code."
        );
        assert_eq!(capture.paragraph_count(), 5);
        assert_eq!(
            capture.headings,
            [
                Heading {
                    level: 1,
                    text: "The next interface".into()
                },
                Heading {
                    level: 2,
                    text: "Why it matters".into()
                },
            ]
        );
        assert!(capture.turns.is_empty());
    }

    #[test]
    fn credit_lines_need_a_word_boundary_and_no_final_period() {
        assert!(is_credit("Source"));
        assert!(is_credit("Source Getty Images"));
        assert!(is_credit("Source: Wikimedia"));
        assert!(!is_credit("Sourcing the parts took a year"));
        assert!(!is_credit("Source material was scarce."));
        assert!(!is_credit("The Source"));
    }

    #[test]
    fn turns_follow_document_position_and_drop_noise() {
        let raw = json!({
            "turns": [
                {"pos": 3, "role": "model", "text": "Answer two"},
                {"pos": 0, "role": "user", "text": "Question one"},
                {"pos": 2, "role": "user", "text": "Question two\n\n\n  more"},
                {"pos": 1, "role": "model", "text": "Answer one"},
                {"pos": 4, "role": "model", "text": "Answer two"},
                {"pos": 5, "role": "system", "text": "ignored"},
                {"pos": 6, "role": "user", "text": " \n "},
            ],
            "blocks": [{"tag": "p", "text": "shell text"}],
        });
        let capture = Capture::from_raw(&raw);
        let order: Vec<_> = capture
            .turns
            .iter()
            .map(|t| (t.role.as_str(), t.text.as_str()))
            .collect();
        assert_eq!(
            order,
            [
                ("user", "Question one"),
                ("model", "Answer one"),
                ("user", "Question two\n\nmore"),
                ("model", "Answer two"),
            ]
        );
        assert!(
            capture
                .text
                .starts_with("user: Question one\n\nmodel: Answer one")
        );
    }

    #[test]
    fn missing_positions_keep_the_given_order() {
        let raw = json!({"turns": [
            {"role": "user", "text": "a"}, {"role": "model", "text": "b"}, {"role": "user", "text": "c"},
        ]});
        let roles: Vec<_> = Capture::from_raw(&raw)
            .turns
            .into_iter()
            .map(|t| t.text)
            .collect();
        assert_eq!(roles, ["a", "b", "c"]);
    }

    #[test]
    fn without_blocks_or_turns_the_page_text_is_the_fallback() {
        let raw = json!({"body": "Hello\n\nHello\nSource Someone\nworld  here", "links": [
            {"href": "https://a.example/x", "text": " A "},
            {"href": "https://a.example/x", "text": "again"},
            {"href": "javascript:void(0)", "text": "no"},
            {"href": "/relative", "text": "no"},
        ]});
        let capture = Capture::from_raw(&raw);
        assert_eq!(capture.text, "Hello\nworld here");
        assert_eq!(
            capture.links,
            [Link {
                href: "https://a.example/x".into(),
                text: "A".into()
            }]
        );
    }

    #[test]
    fn the_expression_passes_the_quiet_window_and_the_bound() {
        let options = CaptureOptions {
            quiet: Duration::from_millis(700),
            ..Default::default()
        };
        let expression = expression(&options, Duration::from_secs(9));
        assert!(expression.ends_with(")(700, 9000)"), "{expression}");
    }
}
