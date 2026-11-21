//! Locale catalogs: the TSV files of `ay/locales/<locale>/*.tsv` and the `[locales.<id>.messages]` tables of
//! `aphrody/assets/os/locales.toml`. User-facing text is looked up here, by English source text or by message id.
//!
//! TSV columns, separated by a TAB: context, English source text, translation. Lines starting with `#` and blank lines
//! are skipped; `\n`, `\t` and `\\` are escapes. An empty translation means "same as the source".

use crate::error::{Error, Result};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct Catalog {
    entries: BTreeMap<(String, String), String>,
    messages: BTreeMap<String, String>,
}

fn unescape(column: &str) -> String {
    let mut out = String::with_capacity(column.len());
    let mut chars = column.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('\\') | None => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
        }
    }
    out
}

impl Catalog {
    pub fn parse_tsv(text: &str) -> Self {
        let mut catalog = Self::default();
        for line in text.lines().filter(|line| !line.trim().is_empty() && !line.starts_with('#')) {
            let mut columns = line.splitn(3, '\t');
            let (Some(context), Some(source), Some(translation)) = (columns.next(), columns.next(), columns.next()) else { continue };
            if translation.is_empty() {
                continue;
            }
            catalog.entries.insert((context.to_owned(), unescape(source)), unescape(translation));
        }
        catalog
    }

    /// The `messages` table of `locale` (falling back to `default_locale`) of a `locales.toml`.
    pub fn parse_locales_toml(text: &str, locale: &str, origin: &str) -> Result<Self> {
        let table: toml::Table = text.parse().map_err(|source| Error::Toml { path: origin.into(), source })?;
        let locales = table.get("locales").and_then(toml::Value::as_table).ok_or_else(|| Error::Invalid(format!("{origin}: no [locales] table")))?;
        let default = locales.get("default_locale").and_then(toml::Value::as_str);
        let messages = [Some(locale), locale.split('-').next(), default]
            .into_iter()
            .flatten()
            .find_map(|id| locales.get(id).and_then(toml::Value::as_table).and_then(|entry| entry.get("messages")).and_then(toml::Value::as_table));
        let mut catalog = Self::default();
        for (id, value) in messages.into_iter().flatten() {
            if let Some(text) = value.as_str() {
                catalog.messages.insert(id.clone(), text.to_owned());
            }
        }
        Ok(catalog)
    }

    /// Reads `<root>/<locale>/*.tsv` (then the language without region) and `<root>/locales.toml` when present.
    pub fn load(root: &Path, locale: &str) -> Result<Self> {
        let mut catalog = Self::default();
        let language = locale.split('-').next().unwrap_or(locale);
        for tag in [language, locale] {
            let dir = root.join(tag);
            let mut files: Vec<_> = std::fs::read_dir(&dir).into_iter().flatten().flatten().map(|entry| entry.path()).filter(|path| path.extension().is_some_and(|ext| ext == "tsv")).collect();
            files.sort();
            for file in files {
                let text = std::fs::read_to_string(&file).map_err(|source| Error::io(&file, source))?;
                catalog.merge(Self::parse_tsv(&text));
            }
        }
        let toml_path = root.join("locales.toml");
        if toml_path.is_file() {
            let text = std::fs::read_to_string(&toml_path).map_err(|source| Error::io(&toml_path, source))?;
            catalog.merge(Self::parse_locales_toml(&text, locale, &toml_path.display().to_string())?);
        }
        Ok(catalog)
    }

    pub fn merge(&mut self, other: Self) {
        self.entries.extend(other.entries);
        self.messages.extend(other.messages);
    }

    /// The translation of an English source text, or the text itself when the catalog has none.
    pub fn tr<'a>(&'a self, source: &'a str) -> &'a str {
        self.tr_ctx("", source)
    }

    pub fn tr_ctx<'a>(&'a self, context: &str, source: &'a str) -> &'a str {
        self.entries.get(&(context.to_owned(), source.to_owned())).map_or(source, String::as_str)
    }

    /// A message by id (`settings_title`), from a `locales.toml`.
    pub fn message(&self, id: &str) -> Option<&str> {
        self.messages.get(id).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.entries.len() + self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tsv_lookup_with_context_escapes_and_fallback() {
        let catalog = Catalog::parse_tsv("# comment\n\tSave\tEnregistrer\n@plural\t{n} file\t{n} fichier\n\tLine\\none\tLigne\\nun\n\tUntranslated\t\n");
        assert_eq!(catalog.tr("Save"), "Enregistrer");
        assert_eq!(catalog.tr_ctx("@plural", "{n} file"), "{n} fichier");
        assert_eq!(catalog.tr("Line\none"), "Ligne\nun");
        assert_eq!(catalog.tr("Untranslated"), "Untranslated");
        assert_eq!(catalog.tr("Missing"), "Missing");
    }

    #[test]
    fn locales_toml_messages_fall_back_to_the_default_locale() -> Result<()> {
        let text = "[locales]\ndefault_locale = \"en\"\n[locales.en.messages]\ndark = \"Dark\"\nlight = \"Light\"\n[locales.fr.messages]\ndark = \"Sombre\"\n";
        let fr = Catalog::parse_locales_toml(text, "fr-CA", "test")?;
        assert_eq!(fr.message("dark"), Some("Sombre"));
        let de = Catalog::parse_locales_toml(text, "de", "test")?;
        assert_eq!(de.message("light"), Some("Light"));
        Ok(())
    }

    #[test]
    fn load_reads_the_language_directory() -> Result<()> {
        let dir = tempfile::tempdir().map_err(|source| Error::io("tempdir", source))?;
        std::fs::create_dir_all(dir.path().join("fr")).map_err(|source| Error::io("fr", source))?;
        std::fs::write(dir.path().join("fr/common.tsv"), "\tOpen\tOuvrir\n").map_err(|source| Error::io("tsv", source))?;
        assert_eq!(Catalog::load(dir.path(), "fr-CA")?.tr("Open"), "Ouvrir");
        Ok(())
    }
}
