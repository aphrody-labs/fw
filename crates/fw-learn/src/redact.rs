//! Scrubs text before it enters the store or a published rule: tracked files must not carry personal or machine data.

const EDGE: &[char] = &['(', ')', '[', ']', '{', '}', '<', '>', ',', ';', '"', '\'', '`'];
const TOKEN_PREFIXES: &[&str] = &["ghp_", "gho_", "ghs_", "ghu_", "github_pat_", "sk-", "sk_live", "xoxb-", "xoxp-", "xoxa-", "AIza"];
const HOME_MARKERS: &[&str] = &["c:\\users\\", "\\users\\", "/users/", "/home/"];

pub fn redact(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for (line_index, line) in text.split('\n').enumerate() {
        if line_index > 0 {
            out.push('\n');
        }
        for (word_index, word) in line.split(' ').enumerate() {
            if word_index > 0 {
                out.push(' ');
            }
            out.push_str(&redact_word(word));
        }
    }
    out
}

fn redact_word(word: &str) -> String {
    let start = word.len() - word.trim_start_matches(EDGE).len();
    let end = word.trim_end_matches(EDGE).len().max(start);
    let (lead, core, tail) = (&word[..start], &word[start..end], &word[end..]);
    if core.is_empty() {
        return word.to_owned();
    }
    format!("{lead}{}{tail}", redact_core(core))
}

fn redact_core(core: &str) -> String {
    if is_email(core) {
        return "<email>".to_owned();
    }
    if is_token(core) {
        return "<token>".to_owned();
    }
    if let Some(home) = home_relative(core) {
        return home;
    }
    if is_ipv4(core) {
        return "<ip>".to_owned();
    }
    if is_secret(core) {
        return "<secret>".to_owned();
    }
    core.to_owned()
}

fn is_email(core: &str) -> bool {
    core.split_once('@').is_some_and(|(local, domain)| !local.is_empty() && !local.contains('/') && domain.contains('.') && !domain.starts_with('.'))
}

fn is_token(core: &str) -> bool {
    if TOKEN_PREFIXES.iter().any(|prefix| core.starts_with(prefix)) && core.len() >= 12 {
        return true;
    }
    core.len() == 20 && core.starts_with("AKIA") && core.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

fn home_relative(core: &str) -> Option<String> {
    let lower = core.to_ascii_lowercase();
    for marker in HOME_MARKERS {
        if let Some(at) = lower.find(marker) {
            let after = &core[at + marker.len()..];
            let rest = after.find(['/', '\\']).map_or("", |separator| &after[separator..]);
            return Some(format!("~{rest}"));
        }
    }
    None
}

fn is_ipv4(core: &str) -> bool {
    let host = core.split(':').next().unwrap_or(core);
    let parts: Vec<&str> = host.split('.').collect();
    parts.len() == 4 && parts.iter().all(|part| !part.is_empty() && part.len() <= 3 && part.parse::<u8>().is_ok())
}

fn is_secret(core: &str) -> bool {
    core.len() >= 40
        && core.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+' | '=' | '/'))
        && core.chars().any(|c| c.is_ascii_digit())
        && core.chars().any(|c| c.is_ascii_alphabetic())
        && !core.chars().all(|c| c.is_ascii_hexdigit())
        && !core.contains('/')
}

#[cfg(test)]
mod tests {
    use super::redact;

    #[test]
    fn removes_personal_and_machine_data() {
        assert_eq!(redact("mail dev@example.com now"), "mail <email> now");
        assert_eq!(redact("see C:\\Users\\someone\\src\\fw"), "see ~\\src\\fw");
        assert_eq!(redact("at /home/builder/src/fw."), "at ~/src/fw.");
        assert_eq!(redact("host 10.1.2.3:22 up"), "host <ip> up");
        assert_eq!(redact("key ghp_abcdefghijklmnopqrstuvwxyz0123456789"), "key <token>");
    }

    #[test]
    fn keeps_shas_scopes_and_versions() {
        let text = "fix 3b02f21f03a1b2c3d4e5f60718293a4b5c6d7e8f in @aphrody/core 2026.11.0 v1.2.3.4x";
        assert_eq!(redact(text), text);
    }
}
