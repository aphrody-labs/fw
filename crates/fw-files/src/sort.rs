//! Sorting modes of yazi's `mgr.sort_by`, with natural ordering ("file2" before "file10").

use crate::entry::Entry;
use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortBy {
    None,
    Mtime,
    Extension,
    Alphabetical,
    Natural,
    Size,
    Mime,
}

impl SortBy {
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "none" => Self::None,
            "mtime" | "modified" => Self::Mtime,
            "extension" | "ext" => Self::Extension,
            "alphabetical" | "alpha" => Self::Alphabetical,
            "natural" => Self::Natural,
            "size" => Self::Size,
            "mime" => Self::Mime,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Sorter {
    pub by: SortBy,
    pub reverse: bool,
    pub dirs_first: bool,
    /// Case-sensitive comparison of names (yazi `sort_sensitive`).
    pub sensitive: bool,
}

impl Default for Sorter {
    /// yazi defaults: natural, ascending, directories first, case-insensitive.
    fn default() -> Self {
        Self { by: SortBy::Natural, reverse: false, dirs_first: true, sensitive: false }
    }
}

impl Sorter {
    pub fn sort(&self, entries: &mut [Entry]) {
        entries.sort_by(|a, b| self.compare(a, b));
    }

    pub fn compare(&self, a: &Entry, b: &Entry) -> Ordering {
        if self.dirs_first && a.is_dir() != b.is_dir() {
            // Directories stay first whether or not the order is reversed, as in yazi.
            return if a.is_dir() { Ordering::Less } else { Ordering::Greater };
        }
        let ord = match self.by {
            SortBy::None => Ordering::Equal,
            SortBy::Mtime => a.mtime_ms.cmp(&b.mtime_ms),
            SortBy::Size => a.size.cmp(&b.size),
            SortBy::Extension => cmp_text(a.extension(), b.extension(), self.sensitive, false).then_with(|| cmp_text(&a.name, &b.name, self.sensitive, true)),
            SortBy::Mime => a.mime.as_deref().unwrap_or("").cmp(b.mime.as_deref().unwrap_or("")).then_with(|| cmp_text(&a.name, &b.name, self.sensitive, true)),
            SortBy::Alphabetical => cmp_text(&a.name, &b.name, self.sensitive, false),
            SortBy::Natural => cmp_text(&a.name, &b.name, self.sensitive, true),
        };
        if self.reverse { ord.reverse() } else { ord }
    }
}

fn cmp_text(a: &str, b: &str, sensitive: bool, natural: bool) -> Ordering {
    if natural {
        natural_cmp(a, b, sensitive)
    } else if sensitive {
        a.cmp(b)
    } else {
        a.chars().flat_map(char::to_lowercase).cmp(b.chars().flat_map(char::to_lowercase))
    }
}

/// Natural order: digit runs compare by numeric value, everything else char by char.
/// Equal values with different zero padding order the shorter padding first ("a1" < "a01").
pub fn natural_cmp(a: &str, b: &str, sensitive: bool) -> Ordering {
    let (mut ai, mut bi) = (a.char_indices().peekable(), b.char_indices().peekable());
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some((si, ac)), Some((sj, bc))) if ac.is_ascii_digit() && bc.is_ascii_digit() => {
                let ra = digit_run(a, si);
                let rb = digit_run(b, sj);
                let (ta, tb) = (ra.trim_start_matches('0'), rb.trim_start_matches('0'));
                let ord = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb)).then_with(|| ra.len().cmp(&rb.len()));
                if ord != Ordering::Equal {
                    return ord;
                }
                for _ in 0..ra.len() {
                    ai.next();
                }
                for _ in 0..rb.len() {
                    bi.next();
                }
            }
            (Some((_, ac)), Some((_, bc))) => {
                let ord = if sensitive { ac.cmp(&bc) } else { ac.to_lowercase().cmp(bc.to_lowercase()) };
                if ord != Ordering::Equal {
                    return ord;
                }
                ai.next();
                bi.next();
            }
        }
    }
}

fn digit_run(s: &str, start: usize) -> &str {
    let rest = &s[start..];
    let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    &rest[..end]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::Kind;

    fn e(name: &str, kind: Kind, size: u64, mtime: i64) -> Entry {
        Entry { path: name.into(), name: name.into(), kind, size, mtime_ms: Some(mtime), hidden: false, readonly: false, executable: false, link: None, broken: false, mime: None }
    }

    fn names(v: &[Entry]) -> Vec<&str> {
        v.iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn natural_orders_numbers_by_value() {
        let mut v = vec![e("f10", Kind::File, 0, 0), e("f2", Kind::File, 0, 0), e("f1", Kind::File, 0, 0), e("F03", Kind::File, 0, 0)];
        Sorter::default().sort(&mut v);
        assert_eq!(names(&v), ["f1", "f2", "F03", "f10"]);
    }

    #[test]
    fn alphabetical_is_not_natural() {
        let mut v = vec![e("f10", Kind::File, 0, 0), e("f2", Kind::File, 0, 0)];
        Sorter { by: SortBy::Alphabetical, ..Sorter::default() }.sort(&mut v);
        assert_eq!(names(&v), ["f10", "f2"]);
    }

    #[test]
    fn zero_padding_breaks_ties_shortest_first() {
        assert_eq!(natural_cmp("a1", "a01", true), Ordering::Less);
        assert_eq!(natural_cmp("a001", "a2", true), Ordering::Less);
        assert_eq!(natural_cmp("same", "same", false), Ordering::Equal);
    }

    #[test]
    fn dirs_stay_first_when_reversed() {
        let mut v = vec![e("b", Kind::File, 0, 0), e("a", Kind::Dir, 0, 0), e("c", Kind::File, 0, 0), e("d", Kind::Dir, 0, 0)];
        Sorter { reverse: true, ..Sorter::default() }.sort(&mut v);
        assert_eq!(names(&v), ["d", "a", "c", "b"]);
        Sorter { reverse: true, dirs_first: false, ..Sorter::default() }.sort(&mut v);
        assert_eq!(names(&v), ["d", "c", "b", "a"]);
    }

    #[test]
    fn size_mtime_and_extension_modes() {
        let mut v = vec![e("a.rs", Kind::File, 30, 3), e("b.c", Kind::File, 10, 9), e("c.md", Kind::File, 20, 1)];
        Sorter { by: SortBy::Size, ..Sorter::default() }.sort(&mut v);
        assert_eq!(names(&v), ["b.c", "c.md", "a.rs"]);
        Sorter { by: SortBy::Mtime, ..Sorter::default() }.sort(&mut v);
        assert_eq!(names(&v), ["c.md", "a.rs", "b.c"]);
        Sorter { by: SortBy::Extension, ..Sorter::default() }.sort(&mut v);
        assert_eq!(names(&v), ["b.c", "c.md", "a.rs"]);
    }

    #[test]
    fn case_sensitivity() {
        let mut v = vec![e("b", Kind::File, 0, 0), e("A", Kind::File, 0, 0), e("a", Kind::File, 0, 0)];
        Sorter { sensitive: true, ..Sorter::default() }.sort(&mut v);
        assert_eq!(names(&v), ["A", "a", "b"]);
        assert_eq!(SortBy::parse("alpha"), Some(SortBy::Alphabetical));
        assert_eq!(SortBy::parse("nope"), None);
    }
}
