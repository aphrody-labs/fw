// SPDX-License-Identifier: Apache-2.0

//! SVG sanitizer for untrusted input (uploads, scraped pages, model output).
//!
//! Elements are an allowlist: only SVG-namespace elements known to be passive are written
//! (shapes, text, paint servers, filters, `use`, `image`, `style`, animation). Everything else
//! (`script`, `foreignObject`, `iframe`, unknown or foreign-namespace elements) is dropped with
//! its subtree. Animation elements that target a link or event attribute (`href`, `src`, `on*`,
//! `style`) are dropped, and every animation value (`to`, `from`, `values`, `by`) is checked piece
//! by piece (`;`-separated) for script URLs.
//!
//! Attributes are checked, not enumerated: every `on*` event attribute and `xml:base` is removed,
//! URL attributes (`href`, `xlink:href`, `src`, `action`, `formaction`) must pass [`is_safe_url`],
//! and every value that can carry CSS (`style`, `<style>` text, any value with `(` or `\`) is
//! normalized first (comments dropped, CSS escapes decoded) and refused when it holds
//! `expression(`, a script URL, `image-set(`, `image(`, `src(`, `element(`, `cross-fade(`,
//! `@import`, `@font-face`, `@namespace` or a `url(...)` that is not safe.
//! Processing instructions, comments and DTDs go too (the parser rejects DTDs, so
//! entity-expansion bombs never reach the tree).
//!
//! Optional: external references (`href="https://..."`, `url(https://...)`) and editor
//! metadata (`<metadata>`, Inkscape/Sodipodi/Illustrator/Sketch/Figma namespaces).

use roxmltree::Node;

use crate::{
    Result, SVG_NS,
    xml::{self, Rules, XML_NS},
};

/// What the sanitizer keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SanitizeOptions {
    /// Keep `href`/`url()` references to other documents (`http:`, `https:`, relative paths).
    /// Same-document (`#id`) and raster `data:image/*` references are always kept.
    pub allow_external_refs: bool,
    /// Drop `<metadata>` and editor namespaces (`<title>`/`<desc>` are kept for accessibility).
    pub strip_metadata: bool,
}

impl Default for SanitizeOptions {
    fn default() -> Self {
        Self {
            allow_external_refs: false,
            strip_metadata: true,
        }
    }
}

/// SVG elements that are written; any other element is dropped with its subtree.
const ALLOWED_ELEMENTS: &[&str] = &[
    "a",
    "circle",
    "clipPath",
    "defs",
    "desc",
    "ellipse",
    "feBlend",
    "feColorMatrix",
    "feComponentTransfer",
    "feComposite",
    "feConvolveMatrix",
    "feDiffuseLighting",
    "feDisplacementMap",
    "feDistantLight",
    "feDropShadow",
    "feFlood",
    "feFuncA",
    "feFuncB",
    "feFuncG",
    "feFuncR",
    "feGaussianBlur",
    "feImage",
    "feMerge",
    "feMergeNode",
    "feMorphology",
    "feOffset",
    "fePointLight",
    "feSpecularLighting",
    "feSpotLight",
    "feTile",
    "feTurbulence",
    "filter",
    "g",
    "image",
    "line",
    "linearGradient",
    "marker",
    "mask",
    "mpath",
    "path",
    "pattern",
    "polygon",
    "polyline",
    "radialGradient",
    "rect",
    "stop",
    "style",
    "svg",
    "switch",
    "symbol",
    "text",
    "textPath",
    "title",
    "tspan",
    "use",
    "view",
];

/// Animation elements, kept only when [`animation_is_safe`].
const ANIMATION_ELEMENTS: &[&str] = &["animate", "animateMotion", "animateTransform", "set"];

/// Animation attributes whose values replace a target attribute.
const ANIMATION_VALUES: &[&str] = &["to", "from", "values", "by"];

/// CSS at-rules that cannot load a resource.
const SAFE_AT_RULES: &[&str] = &[
    "media",
    "supports",
    "keyframes",
    "-webkit-keyframes",
    "layer",
    "container",
];

/// CSS functions and rules that load resources or run code: refused wherever CSS can appear.
const CSS_DENIED: &[&str] = &[
    "expression(",
    "javascript:",
    "vbscript:",
    "@import",
    "@font-face",
    "@namespace",
    "-moz-binding",
    "behavior:",
    "behaviour:",
    "image-set(",
    "image(",
    "src(",
    "element(",
    "cross-fade(",
    "paint(",
];

/// Namespaces written by editors that carry no rendering information.
pub const EDITOR_NAMESPACES: &[&str] = &[
    "http://www.inkscape.org/namespaces/inkscape",
    "http://sodipodi.sourceforge.net/DTD/sodipodi-0.dtd",
    "http://ns.adobe.com/AdobeIllustrator/10.0/",
    "http://ns.adobe.com/AdobeSVGViewerExtensions/3.0/",
    "http://ns.adobe.com/Extensibility/1.0/",
    "http://ns.adobe.com/Graphs/1.0/",
    "http://ns.adobe.com/SaveForWeb/1.0/",
    "http://ns.adobe.com/Variables/1.0/",
    "http://ns.adobe.com/xap/1.0/",
    "http://www.bohemiancoding.com/sketch/ns",
    "http://www.figma.com/figma/ns",
    "http://purl.org/dc/elements/1.1/",
    "http://creativecommons.org/ns#",
    "http://www.w3.org/1999/02/22-rdf-syntax-ns#",
];

/// Lower-cased `value` without whitespace and control characters, the form URL schemes are
/// matched on (browsers ignore both inside a scheme).
fn compact(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .collect::<String>()
        .to_ascii_lowercase()
}

/// `true` when a URL value is safe under `options`.
#[must_use]
pub fn is_safe_url(value: &str, options: SanitizeOptions) -> bool {
    let compact = compact(value);
    if compact.is_empty() || compact.starts_with('#') {
        return true;
    }
    if has_script_scheme(&compact) {
        return false;
    }
    if let Some(rest) = compact.strip_prefix("data:") {
        return [
            "image/png",
            "image/jpeg",
            "image/jpg",
            "image/gif",
            "image/webp",
            "image/avif",
        ]
        .iter()
        .any(|mime| rest.starts_with(mime));
    }
    options.allow_external_refs
}

fn has_script_scheme(compact: &str) -> bool {
    compact.starts_with("javascript:") || compact.starts_with("vbscript:")
}

/// `true` when no `;`-separated piece of `value` starts with a script scheme (the list syntax of
/// animation values).
fn pieces_are_safe(value: &str) -> bool {
    value
        .split(';')
        .all(|piece| !has_script_scheme(&compact(piece)))
}

/// CSS text as a parser sees it: comments become a space, escapes (`\75 `, `\u`) are decoded,
/// ASCII letters are lower-cased. Without this, `\75rl(` or `ur\6c(` hide a `url(`.
fn css_normalize(css: &str) -> String {
    let chars: Vec<char> = css.chars().collect();
    let mut out = String::with_capacity(css.len());
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '/' if chars.get(i + 1) == Some(&'*') => {
                i += 2;
                while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                    i += 1;
                }
                i += 2;
                out.push(' ');
            }
            '\\' => {
                i += 1;
                let hex_len = chars[i..]
                    .iter()
                    .take(6)
                    .take_while(|c| c.is_ascii_hexdigit())
                    .count();
                if hex_len > 0 {
                    let code = chars[i..i + hex_len]
                        .iter()
                        .fold(0u32, |n, c| n * 16 + c.to_digit(16).unwrap_or(0));
                    i += hex_len;
                    if chars.get(i).is_some_and(|c| c.is_whitespace()) {
                        i += 1;
                    }
                    out.push(
                        char::from_u32(code)
                            .filter(|c| *c != '\0')
                            .unwrap_or('\u{fffd}'),
                    );
                } else if let Some(&c) = chars.get(i) {
                    if c != '\n' {
                        out.push(c.to_ascii_lowercase());
                    }
                    i += 1;
                }
            }
            c => {
                out.push(c.to_ascii_lowercase());
                i += 1;
            }
        }
    }
    out
}

/// Checks a value that can carry CSS: a stylesheet (`stylesheet`), a `style` attribute or any
/// attribute value, after [`css_normalize`].
fn css_is_safe(css: &str, options: SanitizeOptions, stylesheet: bool) -> bool {
    let normalized = css_normalize(css);
    if CSS_DENIED.iter().any(|bad| normalized.contains(bad)) {
        return false;
    }
    if stylesheet {
        for (at, _) in normalized.match_indices('@') {
            let name: String = normalized[at + 1..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                .collect();
            if !SAFE_AT_RULES.contains(&name.as_str()) {
                return false;
            }
        }
    }
    let mut rest = normalized.as_str();
    while let Some(start) = rest.find("url(") {
        let after = &rest[start + 4..];
        let end = after.find(')').unwrap_or(after.len());
        let inner = after[..end].trim().trim_matches(['"', '\'']);
        if !is_safe_url(inner, options) {
            return false;
        }
        rest = &after[end..];
    }
    true
}

/// `true` when an animation element cannot rewrite a link, script or style attribute.
fn animation_is_safe(node: Node<'_, '_>) -> bool {
    match node.tag_name().name() {
        "animateMotion" => true,
        _ => node.attribute("attributeName").is_some_and(|name| {
            let name = compact(name);
            let local = name.rsplit(':').next().unwrap_or_default();
            !(local.is_empty()
                || local.starts_with("on")
                || matches!(local, "href" | "src" | "action" | "formaction" | "style"))
        }),
    }
}

struct Sanitizer(SanitizeOptions);

impl Sanitizer {
    /// A `<style>` is kept only when every child is a text node (a comment, processing
    /// instruction or element would split the text a browser joins) and the joined text passes
    /// [`css_is_safe`] once; the writer then emits exactly the checked text.
    fn style_is_safe(&self, node: Node<'_, '_>) -> bool {
        node.children().all(|child| child.is_text())
            && css_is_safe(
                &node
                    .children()
                    .filter_map(|child| child.text())
                    .collect::<String>(),
                self.0,
                true,
            )
    }
}

impl Rules for Sanitizer {
    fn keep_element(&self, node: Node<'_, '_>) -> bool {
        let tag = node.tag_name();
        match tag.namespace() {
            Some(SVG_NS) => {
                let name = tag.name();
                if name == "style" {
                    self.style_is_safe(node)
                } else if ANIMATION_ELEMENTS.contains(&name) {
                    animation_is_safe(node)
                } else if name == "metadata" {
                    !self.0.strip_metadata
                } else {
                    ALLOWED_ELEMENTS.contains(&name)
                }
            }
            Some(ns) => !self.0.strip_metadata && EDITOR_NAMESPACES.contains(&ns),
            None => false,
        }
    }

    fn keep_namespace(&self, uri: &str) -> bool {
        !(self.0.strip_metadata && EDITOR_NAMESPACES.contains(&uri))
    }

    fn attribute(
        &self,
        node: Node<'_, '_>,
        local: &str,
        ns: Option<&str>,
        value: &str,
    ) -> Option<String> {
        if local.len() > 2 && local[..2].eq_ignore_ascii_case("on") {
            return None;
        }
        if ns == Some(XML_NS) && local == "base" {
            return None;
        }
        let safe = match local {
            "href" | "src" | "action" | "formaction" => is_safe_url(value, self.0),
            "style" => css_is_safe(value, self.0, true),
            _ if ANIMATION_VALUES.contains(&local)
                && ANIMATION_ELEMENTS.contains(&node.tag_name().name()) =>
            {
                pieces_are_safe(value) && css_is_safe(value, self.0, false)
            }
            _ => {
                pieces_are_safe(value)
                    && (!value.contains(['(', '\\']) || css_is_safe(value, self.0, false))
            }
        };
        safe.then(|| value.to_owned())
    }
}

/// Returns a sanitized copy of `svg`.
///
/// # Errors
/// [`crate::Error::Xml`] for malformed XML or a DTD, [`crate::Error::NotSvg`] for a non-SVG root.
pub fn sanitize(svg: &str, options: SanitizeOptions) -> Result<String> {
    let doc = xml::parse(svg)?;
    Ok(xml::write_document(&doc, &Sanitizer(options)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean(svg: &str) -> String {
        sanitize(svg, SanitizeOptions::default()).expect("sanitize")
    }

    #[test]
    fn strips_active_content() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" onload="alert(1)"><script>alert(2)</script><a href="javascript:alert(3)"><rect width="1" height="1" onclick="x()"/></a><foreignObject><div/></foreignObject><use href="#ok"/><image href="https://evil/x.png"/><image href="data:image/png;base64,AAAA"/><image href="data:text/html,x"/><rect style="fill:url(https://evil/x)"/><style>@import url(x.css);</style><!-- c --></svg>"##;
        let out = clean(svg);
        for bad in [
            "onload",
            "script",
            "javascript",
            "onclick",
            "foreignObject",
            "evil",
            "text/html",
            "@import",
            "<!--",
        ] {
            assert!(!out.contains(bad), "{bad} survived in {out}");
        }
        assert!(out.contains(r##"<use href="#ok"/>"##));
        assert!(out.contains("data:image/png;base64,AAAA"));
    }

    #[test]
    fn animation_cannot_inject_script_urls() {
        let payloads = [
            r##"<a><set attributeName="href" to="javascript:alert(1)"/></a>"##,
            r##"<a><animate attributeName="xlink:href" values="#a;javascript:alert(1)"/></a>"##,
            r##"<a><animate attributeName=" HREF " from="#a" to="jav&#x09;ascript:alert(1)"/></a>"##,
            r##"<a><animate attributeName="x:href" xmlns:x="urn:x" by="javascript:alert(1)"/></a>"##,
            r##"<a><animate attributeName="onclick" to="alert(1)"/></a>"##,
            r##"<a><animate to="javascript:alert(1)"/></a>"##,
            r##"<a><animateMotion to="javascript:alert(1)"/></a>"##,
            r##"<a><animateTransform attributeName="href" to="vbscript:x"/></a>"##,
            r##"<a><discard begin="0s"/><set attributeName="style" to="x"/></a>"##,
        ];
        for body in payloads {
            let svg = format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">{body}</svg>"##
            );
            let out = clean(&svg);
            assert!(
                !out.contains("javascript") && !out.contains("vbscript"),
                "{body} -> {out}"
            );
            assert!(!out.contains("onclick"), "{body} -> {out}");
            assert!(!out.contains("<set"), "{body} -> {out}");
            assert!(!out.contains("discard"), "{body} -> {out}");
        }
        let ok = clean(
            r##"<svg xmlns="http://www.w3.org/2000/svg"><rect><animate attributeName="opacity" values="0;1" dur="1s"/></rect></svg>"##,
        );
        assert!(
            ok.contains(r##"<animate attributeName="opacity" values="0;1" dur="1s"/>"##),
            "{ok}"
        );
    }

    #[test]
    fn unknown_and_foreign_elements_are_dropped() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg"><frob/><html:iframe xmlns:html="http://www.w3.org/1999/xhtml" src="https://evil/"/><SCRIPT/><rect width="1" height="1"/></svg>"##;
        let out = clean(svg);
        assert_eq!(
            out,
            r##"<svg xmlns="http://www.w3.org/2000/svg"><rect width="1" height="1"/></svg>"##
        );
    }

    #[test]
    fn css_obfuscation_is_refused() {
        let payloads = [
            r##"<style>.a{background:\75 rl(https://evil/x)}</style>"##,
            r##"<style>.a{background:ur\6c(https://evil/x)}</style>"##,
            r##"<style>.a{background:u\72l(https://evil/x)}</style>"##,
            r##"<style>.a{background:image-set('https://evil/x' 1x)}</style>"##,
            r##"<style>.a{background:-webkit-image-set(url(https://evil/x) 1x)}</style>"##,
            r##"<style>@font-face{font-family:f;src:url(https://evil/f.woff)}</style>"##,
            r##"<style>@\69mport 'https://evil/x.css';</style>"##,
            r##"<style>@namespace url(https://evil/);</style>"##,
            r##"<style>@unknown x{.a{b:c}}</style>"##,
            r##"<style>.a{fill:url(  'https://evil/x' )}</style>"##,
        ];
        for body in payloads {
            let svg = format!(r##"<svg xmlns="http://www.w3.org/2000/svg">{body}</svg>"##);
            let out = clean(&svg);
            assert!(!out.contains("evil"), "{body} -> {out}");
        }
        for attr in [
            r##"style="fill:\75 rl(https://evil/x)""##,
            r##"style="fill:image-set('https://evil/x' 1x)""##,
            r##"fill="\75 rl(https://evil/x)""##,
            r##"fill="url(https://evil/x)""##,
        ] {
            let svg = format!(r##"<svg xmlns="http://www.w3.org/2000/svg"><rect {attr}/></svg>"##);
            let out = clean(&svg);
            assert!(!out.contains("evil"), "{attr} -> {out}");
        }
        let ok = clean(
            r##"<svg xmlns="http://www.w3.org/2000/svg"><style>@media (prefers-color-scheme: dark){.a{fill:url(#g)}}</style><rect fill="url(#g)" style="stroke:red"/></svg>"##,
        );
        assert!(
            ok.contains("@media") && ok.contains("url(#g)") && ok.contains("stroke:red"),
            "{ok}"
        );
    }

    #[test]
    fn style_split_across_text_nodes_is_checked_as_a_whole() {
        for body in [
            "<style>.a{background:u<!--x-->rl(https://evil/x)}</style>",
            "<style>.a{background:ur<![CDATA[l(https://evil/x)]]>}</style>",
            "<style>.a{background:u<!--1-->r<!--2-->l(<!--3-->https://evil/x)}</style>",
            "<style>@im<!--x-->port 'https://evil/x.css';</style>",
            "<style>.a{b:c}<?pi x?>.d{background:url(https://evil/x)}</style>",
            "<style><b>.a{background:url(https://evil/x)}</b></style>",
        ] {
            let svg = format!(r#"<svg xmlns="http://www.w3.org/2000/svg">{body}</svg>"#);
            let out = clean(&svg);
            assert!(!out.contains("evil"), "{body} -> {out}");
            assert!(!out.contains("rl("), "{body} -> {out}");
        }
    }

    #[test]
    fn xml_base_is_removed() {
        let out = clean(
            r##"<svg xmlns="http://www.w3.org/2000/svg" xml:base="https://evil/"><rect/></svg>"##,
        );
        assert!(!out.contains("evil"), "{out}");
    }

    #[test]
    fn external_refs_are_opt_in() {
        let svg =
            r##"<svg xmlns="http://www.w3.org/2000/svg"><image href="https://cdn/x.png"/></svg>"##;
        let opts = SanitizeOptions {
            allow_external_refs: true,
            ..SanitizeOptions::default()
        };
        assert!(
            sanitize(svg, opts)
                .expect("ok")
                .contains("https://cdn/x.png")
        );
    }

    #[test]
    fn strips_editor_metadata() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:inkscape="http://www.inkscape.org/namespaces/inkscape" inkscape:version="1"><metadata>m</metadata><inkscape:grid/><path d="M0 0"/></svg>"##;
        let out = clean(svg);
        assert_eq!(
            out,
            r##"<svg xmlns="http://www.w3.org/2000/svg"><path d="M0 0"/></svg>"##
        );
    }

    #[test]
    fn editor_metadata_is_kept_on_request() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:inkscape="http://www.inkscape.org/namespaces/inkscape"><metadata>m</metadata><inkscape:grid/><path d="M0 0"/></svg>"##;
        let opts = SanitizeOptions {
            strip_metadata: false,
            ..SanitizeOptions::default()
        };
        let out = sanitize(svg, opts).expect("ok");
        assert!(
            out.contains("<metadata>") && out.contains("inkscape:grid"),
            "{out}"
        );
    }
}
