//! Queries over the dataset. Every operation returns a JSON value with an `op` field, so the CLI, HTTP, MCP and the
//! Bun plugin share one result shape and `render` can turn any of them into text or MDX.

use crate::awesome;
use crate::model::Record;
use crate::store::{self, Dataset};
use serde_json::{Value, json};

fn relevance(r: &Record, tokens: &[String]) -> Option<f64> {
    let name = store::key(&r.name);
    let desc = r.description.to_ascii_lowercase();
    let mut total = 0.0;
    for t in tokens {
        let tk = store::key(t);
        let mut s = 0.0;
        // An exact name wins outright; otherwise every field adds a little, so usage (the score) orders the rest.
        if name == tk {
            s += 100.0;
        } else if name.contains(&tk) {
            s += 12.0;
        }
        if r.keywords.contains(t) {
            s += 12.0;
        } else if r.keywords.iter().any(|k| k.contains(t.as_str())) {
            s += 4.0;
        }
        if r.categories.iter().any(|c| c.contains(t.as_str())) {
            s += 6.0;
        }
        if desc.contains(t.as_str()) {
            s += 8.0;
        }
        if s == 0.0 {
            return None;
        }
        total += s;
    }
    Some(total)
}

/// Full-text search over name, keywords, categories and description; every word must match somewhere.
pub fn search(ds: &Dataset, q: &str, category: Option<&str>, limit: usize) -> Result<Value, String> {
    let tokens: Vec<String> = q.split_whitespace().map(str::to_ascii_lowercase).collect();
    let cat = match category {
        Some(c) => Some(ds.find_category(c).ok_or_else(|| format!("no unique category matches `{c}`"))?.slug.clone()),
        None => None,
    };
    let mut hits: Vec<(f64, &Record)> = ds
        .records
        .iter()
        .filter(|r| !r.yanked && cat.as_ref().is_none_or(|c| r.categories.contains(c)))
        .filter_map(|r| if tokens.is_empty() { Some((0.0, r)) } else { relevance(r, &tokens).map(|s| (s, r)) })
        .collect();
    hits.sort_by(|a, b| (b.0 + b.1.score).total_cmp(&(a.0 + a.1.score)));
    let results: Vec<&Record> = hits.into_iter().take(limit).map(|(_, r)| r).collect();
    Ok(json!({ "op": "search", "query": q, "category": cat, "dump_date": ds.meta.dump_date, "count": results.len(), "results": results }))
}

fn live(name: &str) -> Value {
    #[cfg(feature = "live")]
    {
        crate::http::throttle();
        match fw_apm::unified_pm::resolve_live_package("cargo", name) {
            Ok(v) => v,
            Err(e) => json!({ "ok": false, "error": e }),
        }
    }
    #[cfg(not(feature = "live"))]
    {
        let _ = name;
        Value::Null
    }
}

/// One crate with its rank overall and in each category. `with_live` adds the live sparse-index record.
pub fn info(ds: &Dataset, name: &str, with_live: bool) -> Result<Value, String> {
    let r = ds.get(name).ok_or_else(|| format!("`{name}` is not in the dataset (dump {}); it has no recent downloads or dependents, or the name is wrong", ds.meta.dump_date))?;
    let mut by_category = serde_json::Map::new();
    for slug in &r.categories {
        if let Some(pos) = ds.category_members(slug).position(|m| m.name == r.name) {
            by_category.insert(slug.clone(), json!(pos + 1));
        }
    }
    Ok(json!({
        "op": "info",
        "dump_date": ds.meta.dump_date,
        "crate": r,
        "rank": { "overall": ds.overall_rank(&r.name), "of": ds.records.len(), "categories": by_category },
        "live": if with_live { live(&r.name) } else { Value::Null },
    }))
}

/// Side by side, with the best crate per metric.
pub fn compare(ds: &Dataset, names: &[String], with_live: bool) -> Result<Value, String> {
    if names.len() < 2 {
        return Err("compare needs at least two crates".to_owned());
    }
    let mut recs = Vec::new();
    for n in names {
        recs.push(ds.get(n).ok_or_else(|| format!("`{n}` is not in the dataset (dump {})", ds.meta.dump_date))?);
    }
    let best = |f: &dyn Fn(&Record) -> f64| recs.iter().max_by(|a, b| f(a).total_cmp(&f(b))).map(|r| r.name.clone());
    let winners = json!({
        "score": best(&|r| r.score),
        "recent_downloads": best(&|r| r.recent_downloads as f64),
        "dependents": best(&|r| f64::from(r.dependents)),
        "recency": best(&|r| r.components.recency),
        "maintenance": best(&|r| r.components.maintenance),
    });
    let lives: Vec<Value> = if with_live { recs.iter().map(|r| live(&r.name)).collect() } else { Vec::new() };
    Ok(json!({ "op": "compare", "dump_date": ds.meta.dump_date, "crates": recs, "winners": winners, "live": lives }))
}

/// Top crates overall or in one category.
pub fn rank(ds: &Dataset, category: Option<&str>, limit: usize) -> Result<Value, String> {
    let (slug, results): (Option<String>, Vec<&Record>) = match category {
        Some(c) => {
            let cat = ds.find_category(c).ok_or_else(|| format!("no unique category matches `{c}`"))?;
            (Some(cat.slug.clone()), ds.category_members(&cat.slug).take(limit).collect())
        }
        None => (None, ds.records.iter().filter(|r| !r.yanked && r.recent_downloads >= crate::rank::MIN_RECENT_DOWNLOADS).take(limit).collect()),
    };
    Ok(json!({ "op": "rank", "category": slug, "dump_date": ds.meta.dump_date, "results": results }))
}

/// The awesome list of one category, or the index of all lists.
pub fn awesome(ds: &Dataset, category: Option<&str>, limit: usize) -> Result<Value, String> {
    match category {
        Some(c) => {
            let cat = ds.find_category(c).ok_or_else(|| format!("no unique category matches `{c}`"))?;
            let mut v = awesome::list(ds, &cat.slug, limit).ok_or_else(|| format!("category `{}` has fewer than {} eligible crates", cat.slug, awesome::MIN_ENTRIES))?;
            v["op"] = json!("awesome");
            Ok(v)
        }
        None => {
            let lists = awesome::all_lists(ds, awesome::PER_CATEGORY);
            let cats: Vec<Value> = lists
                .iter()
                .map(|l| {
                    let names: Vec<&str> = l["entries"].as_array().map(|a| a.iter().filter_map(|e| e["name"].as_str()).collect()).unwrap_or_default();
                    json!({ "slug": l["category"]["slug"], "name": l["category"]["name"], "entries": names.len(), "top": names.iter().take(3).collect::<Vec<_>>() })
                })
                .collect();
            Ok(json!({ "op": "awesome", "dump_date": ds.meta.dump_date, "count": cats.len(), "categories": cats }))
        }
    }
}
