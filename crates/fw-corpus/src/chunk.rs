//! Markdown to section chunks: split at headings (h1 to h4) outside code fences, then at paragraph
//! boundaries once a section exceeds the size cap.

pub struct Chunk {
    pub heading: String,
    pub anchor: String,
    pub body: String,
}

pub fn slug(heading: &str) -> String {
    let mut slug = String::new();
    for character in heading.chars() {
        if character.is_alphanumeric() {
            slug.extend(character.to_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    slug.trim_end_matches('-').to_owned()
}

/// Title from YAML front matter (`title:`) or the Kotlin `[//]: # (title: ...)` header.
pub fn front_title(markdown: &str) -> Option<String> {
    for line in markdown.lines().take(40) {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("[//]: # (title:") {
            return Some(rest.trim_end_matches(')').trim().to_owned());
        }
        if let Some(rest) = line.strip_prefix("title:") {
            return Some(rest.trim().trim_matches(['"', '\'']).to_owned());
        }
    }
    None
}

fn strip_front_matter(markdown: &str) -> &str {
    let Some(rest) = markdown.strip_prefix("---\n").or_else(|| markdown.strip_prefix("---\r\n")) else {
        return markdown;
    };
    match rest.find("\n---") {
        Some(end) => rest[end + 4..].trim_start_matches(['\r', '\n']),
        None => markdown,
    }
}

pub fn markdown(title: &str, markdown: &str, max_chars: usize) -> Vec<Chunk> {
    let text = strip_front_matter(markdown);
    let mut chunks = Vec::new();
    let mut section = String::new();
    let mut heading = title.to_owned();
    let mut anchor = String::new();
    let mut fenced = false;

    let mut flush = |heading: &str, anchor: &str, section: &mut String| {
        let body = section.trim();
        if body.len() >= 12 {
            chunks.push(Chunk { heading: heading.to_owned(), anchor: anchor.to_owned(), body: body.to_owned() });
        }
        section.clear();
    };

    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
        }
        let level = trimmed.chars().take_while(|character| *character == '#').count();
        let is_heading = !fenced && (1..=4).contains(&level) && trimmed[level..].starts_with(' ');
        if is_heading {
            flush(&heading, &anchor, &mut section);
            let name = trimmed[level..].trim().trim_end_matches('#').trim();
            anchor = slug(name);
            heading = if title.is_empty() || name == title { name.to_owned() } else { format!("{title} > {name}") };
            continue;
        }
        if !fenced && section.len() >= max_chars && line.trim().is_empty() {
            flush(&heading, &anchor, &mut section);
            continue;
        }
        section.push_str(line);
        section.push('\n');
    }
    flush(&heading, &anchor, &mut section);
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_headings_not_fences() {
        let md = "# A\n\nintro text here\n\n```sh\n# not a heading\n```\n\n## B c\n\nbody of b section\n";
        let chunks = markdown("A", md, 3000);
        assert_eq!(chunks.len(), 2);
        assert!(chunks[0].body.contains("# not a heading"));
        assert_eq!(chunks[1].heading, "A > B c");
        assert_eq!(chunks[1].anchor, "b-c");
    }

    #[test]
    fn kotlin_title() {
        assert_eq!(front_title("[//]: # (title: Hello world)\n\nx").as_deref(), Some("Hello world"));
    }
}
