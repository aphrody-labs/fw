//! `{{ key }}` and `{{ key | filter }}` templates, the omarchy `*.tpl` convention.
//!
//! Filters on a color: `hex` (default, `#rrggbb`), `bare` (`rrggbb`), `rgb` (`46, 52, 64`), `rgb_css` (`rgb(46 52 64)`),
//! `hypr` (`rgb(2e3440)`), `hyprwa` (`rgba(2e3440ff)`), `u32` (`0x2e3440`) and `alpha <hh>` (`#rrggbbhh`).

use crate::color::{bare, channels, hex};
use crate::error::{Error, Result};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub enum Var {
    Color(u32),
    Text(String),
}

pub type Vars = BTreeMap<String, Var>;

fn filter_color(color: u32, filter: &[&str], origin: &str) -> Result<String> {
    let [red, green, blue] = channels(color);
    Ok(match filter {
        [] | ["hex"] => hex(color),
        ["bare"] => bare(color),
        ["rgb"] => format!("{red}, {green}, {blue}"),
        ["rgb_css"] => format!("rgb({red} {green} {blue})"),
        ["hypr"] => format!("rgb({})", bare(color)),
        ["hyprwa"] => format!("rgba({}ff)", bare(color)),
        ["u32"] => format!("0x{}", bare(color)),
        ["alpha", digits] if digits.len() == 2 && digits.chars().all(|c| c.is_ascii_hexdigit()) => format!("{}{digits}", hex(color)),
        other => return Err(Error::Invalid(format!("{origin}: unknown filter `{}`", other.join(" ")))),
    })
}

pub fn render(template: &str, vars: &Vars, origin: &str) -> Result<String> {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find("{{") {
        out.push_str(&rest[..open]);
        let after = &rest[open + 2..];
        let close = after.find("}}").ok_or_else(|| Error::Invalid(format!("{origin}: `{{{{` without `}}}}`")))?;
        let inner = after[..close].trim();
        let (key, filter) = match inner.split_once('|') {
            Some((key, filter)) => (key.trim(), filter.split_whitespace().collect::<Vec<_>>()),
            None => (inner, Vec::new()),
        };
        let value = vars.get(key).ok_or_else(|| Error::Invalid(format!("{origin}: unknown key `{key}`")))?;
        match value {
            Var::Color(color) => out.push_str(&filter_color(*color, &filter, origin)?),
            Var::Text(text) if filter.is_empty() => out.push_str(text),
            Var::Text(_) => return Err(Error::Invalid(format!("{origin}: `{key}` is text and takes no filter"))),
        }
        rest = &after[close + 2..];
    }
    out.push_str(rest);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars() -> Vars {
        Vars::from([("accent".to_owned(), Var::Color(0x81a1c1)), ("name".to_owned(), Var::Text("nord".to_owned()))])
    }

    #[test]
    fn renders_keys_and_filters() -> Result<()> {
        let text = render("{{ name }} {{accent}} {{ accent | bare }} {{ accent | rgb }} {{ accent | hyprwa }} {{ accent | alpha 80 }}", &vars(), "t")?;
        assert_eq!(text, "nord #81a1c1 81a1c1 129, 161, 193 rgba(81a1c1ff) #81a1c180");
        Ok(())
    }

    #[test]
    fn unknown_keys_filters_and_unterminated_tags_fail() {
        assert!(render("{{ nope }}", &vars(), "t").is_err());
        assert!(render("{{ accent | nope }}", &vars(), "t").is_err());
        assert!(render("{{ name | hex }}", &vars(), "t").is_err());
        assert!(render("{{ accent", &vars(), "t").is_err());
    }
}
