//! Typed requests for the à-la-carte side of material.aphrody.com: a list of needs in, the canonical URL out.
//!
//! The URL is the same string `tools/material-cdn/client.ts` produces (sorted parameters, merged lists, trimmed numbers), so equal needs
//! share one cache entry; `tools/material-cdn/fixtures/urls.json` pins both implementations to the same expected URLs.
//!
//! ```
//! use fw_material::{Style, cdn::{Request, DEFAULT_BASE}};
//! let url = Request::sprite(Style::Rounded, ["search", "home"])
//!     .fill_both()
//!     .ids_of()
//!     .url(DEFAULT_BASE)
//!     .expect("valid request");
//! assert_eq!(url, "https://material.aphrody.com/v1/icons.svg?fill=0..1&icons=home,search&ids=of&style=rounded");
//! ```

use crate::Style;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

pub const DEFAULT_BASE: &str = "https://material.aphrody.com/v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    /// `icons.svg`: `<symbol>` sprite.
    Sprite,
    /// `icons.json`: path list.
    Json,
    /// `icon.svg`: one icon.
    Icon,
    /// `icons.woff2`: ligature font with exactly the requested icons.
    IconsFont,
    /// `icons.css`: `@font-face` and classes of that font.
    IconsCss,
    /// `fonts/<family>.woff2`.
    Font,
    /// `fonts/<family>.css`.
    FontCss,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// Icon names are `[a-z0-9_]+` (Aphrody icons may add `/` folders).
    InvalidName(String),
    /// A font output without a family, or an icon output without icons.
    Missing(&'static str),
    /// `min..max` with min greater than max, or a non-finite value.
    InvalidAxis(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidName(name) => write!(f, "invalid icon name {name:?}"),
            Error::Missing(what) => write!(f, "missing {what}"),
            Error::InvalidAxis(axis) => write!(f, "invalid axis range for {axis}"),
        }
    }
}

impl std::error::Error for Error {}

/// One request: an output plus what it needs. Build with a constructor, refine with the methods, then [`Request::url`].
#[derive(Clone, Debug)]
pub struct Request {
    output: Output,
    family: Option<String>,
    style: Option<Style>,
    aphrody: bool,
    icons: BTreeSet<String>,
    codepoints: BTreeSet<u32>,
    subsets: BTreeSet<String>,
    axes: BTreeMap<String, (f64, f64)>,
    all_axes: bool,
    features: BTreeSet<String>,
    ids_of: bool,
    css_family: Option<String>,
    css_class: Option<String>,
    display: Option<String>,
    size: Option<f64>,
    color: Option<String>,
    revision: Option<String>,
}

fn names<I, S>(icons: I) -> BTreeSet<String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    icons.into_iter().map(Into::into).collect()
}

impl Request {
    fn new(output: Output) -> Self {
        Request {
            output,
            family: None,
            style: None,
            aphrody: false,
            icons: BTreeSet::new(),
            codepoints: BTreeSet::new(),
            subsets: BTreeSet::new(),
            axes: BTreeMap::new(),
            all_axes: false,
            features: BTreeSet::new(),
            ids_of: false,
            css_family: None,
            css_class: None,
            display: None,
            size: None,
            color: None,
            revision: None,
        }
    }

    fn icon_output<I, S>(output: Output, style: Style, icons: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Request {
            style: Some(style),
            icons: names(icons),
            ..Request::new(output)
        }
    }

    /// SVG sprite: symbols `home` and `home-fill` (see [`Request::ids_of`]).
    pub fn sprite<I, S>(style: Style, icons: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Request::icon_output(Output::Sprite, style, icons)
    }

    /// JSON path list.
    pub fn json<I, S>(style: Style, icons: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Request::icon_output(Output::Json, style, icons)
    }

    /// One icon as an SVG.
    pub fn icon(style: Style, name: impl Into<String>) -> Self {
        Request::icon_output(Output::Icon, style, [name.into()])
    }

    /// Ligature font (woff2) holding exactly these icons.
    pub fn icon_font<I, S>(style: Style, icons: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Request::icon_output(Output::IconsFont, style, icons)
    }

    /// `@font-face` and classes for that font.
    pub fn icons_css<I, S>(style: Style, icons: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Request::icon_output(Output::IconsCss, style, icons)
    }

    /// woff2 of a text family (`google-sans-flex`, `google-sans-code`, `google-sans-code-italic`, `roboto-flex`).
    pub fn font(family: impl Into<String>) -> Self {
        Request {
            family: Some(family.into()),
            ..Request::new(Output::Font)
        }
    }

    /// `@font-face` rules of a text family, one per subset with `unicode-range`.
    pub fn font_css(family: impl Into<String>) -> Self {
        Request {
            family: Some(family.into()),
            ..Request::new(Output::FontCss)
        }
    }

    /// Our own icon set (`set=aphrody`) instead of Material Symbols; for sprite, json and icon outputs.
    pub fn aphrody(mut self) -> Self {
        self.aphrody = true;
        self
    }

    /// Keep these characters.
    pub fn text(mut self, text: &str) -> Self {
        self.codepoints.extend(text.chars().map(u32::from));
        self
    }

    /// Keep an inclusive codepoint range.
    pub fn unicode_range(mut self, first: u32, last: u32) -> Self {
        self.codepoints.extend(first..=last);
        self
    }

    /// Keep a named range of meta.json (`latin`, `latin-ext`, `cyrillic`, ...).
    pub fn subset(mut self, name: impl Into<String>) -> Self {
        self.subsets.insert(name.into());
        self
    }

    /// Pins an axis (`wght`, `FILL`, ...; the tag is case-insensitive).
    pub fn pin(self, tag: &str, value: f64) -> Self {
        self.axis(tag, value, value)
    }

    /// Limits an axis to `min..=max`.
    pub fn axis(mut self, tag: &str, min: f64, max: f64) -> Self {
        self.axes.insert(tag.to_ascii_lowercase(), (min, max));
        self
    }

    /// Both fills (`fill=0..1`): outline and filled symbols.
    pub fn fill_both(self) -> Self {
        self.axis("fill", 0.0, 1.0)
    }

    /// Filled symbols only.
    pub fn filled(self) -> Self {
        self.pin("fill", 1.0)
    }

    /// Axes that are not named keep their whole range instead of being pinned to their default.
    pub fn all_axes(mut self) -> Self {
        self.all_axes = true;
        self
    }

    /// Keep these OpenType layout features besides the defaults (`*` keeps all).
    pub fn feature(mut self, tag: impl Into<String>) -> Self {
        self.features.insert(tag.into());
        self
    }

    /// Sprite ids `o-home` and `f-home` instead of `home` and `home-fill`.
    pub fn ids_of(mut self) -> Self {
        self.ids_of = true;
        self
    }

    pub fn css_family(mut self, name: impl Into<String>) -> Self {
        self.css_family = Some(name.into());
        self
    }

    pub fn css_class(mut self, prefix: impl Into<String>) -> Self {
        self.css_class = Some(prefix.into());
        self
    }

    pub fn display(mut self, value: impl Into<String>) -> Self {
        self.display = Some(value.into());
        self
    }

    /// Width and height in pixels of a single icon.
    pub fn size(mut self, pixels: f64) -> Self {
        self.size = Some(pixels);
        self
    }

    pub fn color(mut self, color: impl Into<String>) -> Self {
        self.color = Some(color.into());
        self
    }

    /// Pins the response to a data revision (`rev` of meta.json): the response is then immutable.
    pub fn revision(mut self, rev: impl Into<String>) -> Self {
        self.revision = Some(rev.into());
        self
    }

    fn path(&self) -> Result<String, Error> {
        Ok(match self.output {
            Output::Sprite => "icons.svg".into(),
            Output::Json => "icons.json".into(),
            Output::Icon => "icon.svg".into(),
            Output::IconsFont => "icons.woff2".into(),
            Output::IconsCss => "icons.css".into(),
            Output::Font | Output::FontCss => {
                let family = self
                    .family
                    .as_deref()
                    .ok_or(Error::Missing("font family"))?;
                format!(
                    "fonts/{family}.{}",
                    if self.output == Output::Font {
                        "woff2"
                    } else {
                        "css"
                    }
                )
            }
        })
    }

    /// The canonical query string (parameters sorted by name, lists merged).
    pub fn query(&self) -> Result<String, Error> {
        let mut params: BTreeMap<String, String> = BTreeMap::new();
        for name in &self.icons {
            let valid = !name.is_empty()
                && name.split('/').all(|part| {
                    !part.is_empty()
                        && part
                            .bytes()
                            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
                })
                && (self.aphrody || !name.contains('/'));
            if !valid {
                return Err(Error::InvalidName(name.clone()));
            }
        }
        for (tag, (min, max)) in &self.axes {
            if !min.is_finite() || !max.is_finite() || min > max {
                return Err(Error::InvalidAxis(tag.clone()));
            }
            let value = if min == max {
                number(*min)
            } else {
                format!("{}..{}", number(*min), number(*max))
            };
            params.insert(tag.clone(), value);
        }
        if self.all_axes {
            params.insert("axes".into(), "all".into());
        }
        if let Some(v) = &self.css_class {
            params.insert("class".into(), v.clone());
        }
        if let Some(v) = &self.color {
            params.insert("color".into(), v.clone());
        }
        if let Some(v) = &self.display {
            params.insert("display".into(), v.clone());
        }
        if let Some(v) = &self.css_family {
            params.insert("family".into(), v.clone());
        }
        if !self.features.is_empty() {
            params.insert("features".into(), join(&self.features));
        }
        if !self.icons.is_empty() {
            params.insert("icons".into(), join(&self.icons));
        }
        if self.ids_of {
            params.insert("ids".into(), "of".into());
        }
        if self.aphrody {
            params.insert("set".into(), "aphrody".into());
        }
        if let Some(v) = self.size {
            params.insert("size".into(), number(v));
        }
        if let Some(style) = self.style {
            params.insert("style".into(), style.slug().into());
        }
        if !self.subsets.is_empty() {
            params.insert("subsets".into(), join(&self.subsets));
        }
        if !self.codepoints.is_empty() {
            params.insert("unicodes".into(), ranges(&self.codepoints));
        }
        let mut query = params
            .iter()
            .map(|(k, v)| format!("{k}={}", encode(v)))
            .collect::<Vec<_>>()
            .join("&");
        if let Some(rev) = &self.revision {
            if !query.is_empty() {
                query.push('&');
            }
            query.push_str("v=");
            query.push_str(rev);
        }
        Ok(query)
    }

    /// The URL under `base` (for example [`DEFAULT_BASE`]).
    pub fn url(&self, base: &str) -> Result<String, Error> {
        if matches!(
            self.output,
            Output::Sprite | Output::Json | Output::Icon | Output::IconsFont | Output::IconsCss
        ) && self.icons.is_empty()
        {
            return Err(Error::Missing("icons"));
        }
        let query = self.query()?;
        let path = self.path()?;
        let base = base.trim_end_matches('/');
        Ok(if query.is_empty() {
            format!("{base}/{path}")
        } else {
            format!("{base}/{path}?{query}")
        })
    }
}

fn join(values: &BTreeSet<String>) -> String {
    values
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(",")
}

/// Rounded to four decimals, no trailing zeros: `400`, `0.5`.
fn number(value: f64) -> String {
    format!("{}", (value * 1e4).round() / 1e4)
}

/// Merged hexadecimal ranges without the `U+` prefix, as in the canonical query: `20-7E,A0`.
fn ranges(codepoints: &BTreeSet<u32>) -> String {
    let mut parts = Vec::new();
    let mut iter = codepoints.iter().copied().peekable();
    while let Some(first) = iter.next() {
        let mut last = first;
        while iter.peek() == Some(&(last + 1)) {
            last += 1;
            iter.next();
        }
        parts.push(if first == last {
            format!("{first:02X}")
        } else {
            format!("{first:02X}-{last:02X}")
        });
    }
    parts.join(",")
}

/// `encodeURIComponent` that keeps the comma and slash separators of lists and folders.
fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')'
            | b','
            | b'/' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn request(name: &str) -> Request {
        match name {
            "sprite-both-fills-of" => Request::sprite(Style::Rounded, ["search", "home"])
                .fill_both()
                .ids_of(),
            "sprite-weight" => Request::sprite(Style::Outlined, ["home"]).pin("wght", 700.0),
            "sprite-aphrody" => {
                Request::sprite(Style::Outlined, ["ai_claude", "file_icons/rust"]).aphrody()
            }
            "json-filled" => Request::json(Style::Sharp, ["home", "home", "add"]).filled(),
            "icon-single" => Request::icon(Style::Sharp, "home")
                .filled()
                .pin("wght", 500.0)
                .size(24.0)
                .color("#ffffff"),
            "icon-font-instanced" => Request::icon_font(Style::Outlined, ["search", "home"])
                .fill_both()
                .pin("wght", 400.0)
                .axis("opsz", 20.0, 48.0)
                .pin("grad", 0.0),
            "icon-font-all-axes" => Request::icon_font(Style::Rounded, ["home"]).all_axes(),
            "icons-css-class" => Request::icons_css(Style::Outlined, ["home"])
                .css_class("mi")
                .css_family("Mi")
                .display("swap"),
            "font-text" => Request::font("google-sans-flex")
                .text("Aphrody")
                .axis("wght", 400.0, 700.0)
                .pin("wdth", 100.0),
            "font-unicodes" => Request::font("roboto-flex")
                .unicode_range(0x20, 0x7e)
                .unicode_range(0xa0, 0xa0)
                .feature("tnum"),
            "font-css-subsets" => Request::font_css("google-sans-code")
                .subset("latin")
                .subset("greek")
                .axis("wght", 400.0, 700.0)
                .revision("0123456789ab"),
            other => panic!("no request for fixture {other}"),
        }
    }

    #[test]
    fn urls_match_the_typescript_client() {
        let fixtures: Vec<Value> = serde_json::from_str(include_str!(
            "../../../tools/material-cdn/fixtures/urls.json"
        ))
        .expect("fixtures parse");
        assert!(fixtures.len() >= 10, "fixtures: {}", fixtures.len());
        for fixture in &fixtures {
            let name = fixture["name"].as_str().expect("name");
            let expected = fixture["url"].as_str().expect("url");
            assert_eq!(
                request(name).url(DEFAULT_BASE).as_deref(),
                Ok(expected),
                "fixture {name}"
            );
        }
    }

    #[test]
    fn rejects_invalid_needs() {
        assert_eq!(
            Request::sprite(Style::Outlined, ["Home"]).url(DEFAULT_BASE),
            Err(Error::InvalidName("Home".into()))
        );
        assert_eq!(
            Request::sprite(Style::Outlined, Vec::<String>::new()).url(DEFAULT_BASE),
            Err(Error::Missing("icons"))
        );
        assert_eq!(
            Request::font("roboto-flex")
                .axis("wght", 700.0, 400.0)
                .url(DEFAULT_BASE),
            Err(Error::InvalidAxis("wght".into()))
        );
    }
}
