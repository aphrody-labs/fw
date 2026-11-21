//! Read-only queries over the graph database; every answer is a JSON value with file:line locations and the label of the indexed tree.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use rusqlite::{Connection, OpenFlags, OptionalExtension, Row, params};
use serde_json::{Map, Value, json};

use crate::error::{Error, Result};
use crate::graph::{SCHEMA_VERSION, SymbolKind};

fn open(db: &Path) -> Result<Connection> {
    if !db.is_file() {
        return Err(Error::message(format!("graph: database {} not found; build it with `fw kernel graph build`", db.display())));
    }
    let corrupt = |detail: String| Error::message(format!("graph: {} is not a usable graph database ({detail}); rebuild it with `fw kernel graph build`", db.display()));
    let connection = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX).map_err(|error| corrupt(error.to_string()))?;
    let version: Option<String> = connection
        .query_row("SELECT value FROM meta WHERE key = 'schema_version'", [], |row| row.get(0))
        .optional()
        .map_err(|error| corrupt(error.to_string()))?;
    match version.and_then(|version| version.parse::<u32>().ok()) {
        Some(SCHEMA_VERSION) => Ok(connection),
        Some(other) => Err(corrupt(format!("schema version {other}, this build reads {SCHEMA_VERSION}"))),
        None => Err(corrupt("no schema version in meta".to_owned())),
    }
}

fn meta(connection: &Connection) -> Result<Map<String, Value>> {
    let mut statement = connection.prepare("SELECT key, value FROM meta ORDER BY key")?;
    let rows = statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
    let mut map = Map::new();
    for row in rows {
        let (key, value) = row?;
        map.insert(key, Value::String(value));
    }
    Ok(map)
}

fn tree_label(meta: &Map<String, Value>) -> Value {
    let pick = |key: &str| meta.get(key).cloned().unwrap_or(Value::Null);
    json!({
        "label": pick("label"),
        "version": pick("version"),
        "commit": pick("commit"),
        "schema_version": pick("schema_version"),
        "builder_version": pick("builder_version"),
        "built_at": pick("built_at"),
    })
}

fn location(path: &str, line: i64) -> String {
    format!("{path}:{line}")
}

fn name_id(connection: &Connection, name: &str) -> Result<Option<i64>> {
    Ok(connection.query_row("SELECT id FROM names WHERE name = ?1", [name], |row| row.get(0)).optional()?)
}

fn name_of(connection: &Connection, id: i64) -> Result<String> {
    Ok(connection.query_row("SELECT name FROM names WHERE id = ?1", [id], |row| row.get(0))?)
}

fn file_id(connection: &Connection, path: &str) -> Result<Option<(i64, String)>> {
    let path = path.trim_start_matches("./");
    let exact: Option<(i64, String)> = connection.query_row("SELECT id, path FROM files WHERE path = ?1", [path], |row| Ok((row.get(0)?, row.get(1)?))).optional()?;
    if exact.is_some() {
        return Ok(exact);
    }
    Ok(connection
        .query_row("SELECT id, path FROM files WHERE path LIKE '%/' || ?1 ORDER BY length(path), path LIMIT 1", [path], |row| Ok((row.get(0)?, row.get(1)?)))
        .optional()?)
}

fn collect(connection: &Connection, sql: &str, parameters: impl rusqlite::Params, build: impl Fn(&Row<'_>) -> rusqlite::Result<Value>) -> Result<Vec<Value>> {
    let mut statement = connection.prepare_cached(sql)?;
    let rows = statement.query_map(parameters, |row| build(row))?;
    rows.map(|row| row.map_err(Error::from)).collect()
}

struct Answer<'a> {
    connection: &'a Connection,
    tree: Value,
    limit: usize,
}

impl Answer<'_> {
    fn limited(&self, command: &str, mut results: Vec<Value>, extra: Value) -> Value {
        let truncated = results.len() > self.limit;
        results.truncate(self.limit);
        let mut value = json!({ "ok": true, "command": command, "tree": self.tree, "count": results.len(), "truncated": truncated });
        if let (Some(map), Some(extra)) = (value.as_object_mut(), extra.as_object()) {
            for (key, item) in extra {
                map.insert(key.clone(), item.clone());
            }
        }
        if let Some(map) = value.as_object_mut() {
            map.insert("results".to_owned(), Value::Array(results));
        }
        value
    }

    fn fetch(&self) -> i64 {
        self.limit as i64 + 1
    }
}

fn symbol_json(row: &Row<'_>) -> rusqlite::Result<Value> {
    let path: String = row.get(2)?;
    let line: i64 = row.get(3)?;
    Ok(json!({
        "name": row.get::<_, String>(0)?,
        "kind": row.get::<_, String>(1)?,
        "file": path,
        "line": line,
        "end_line": row.get::<_, i64>(4)?,
        "location": location(&path, line),
        "signature": row.get::<_, String>(5)?,
        "storage": row.get::<_, String>(6)?,
        "inline": row.get::<_, bool>(7)?,
        "exported": row.get::<_, bool>(8)?,
    }))
}

const SYMBOL_COLUMNS: &str = "n.name, k.name, f.path, s.line, s.end_line, s.signature, s.storage, s.inline, s.exported";
const SYMBOL_JOINS: &str = "FROM symbols s JOIN names n ON n.id = s.name_id JOIN kinds k ON k.id = s.kind JOIN files f ON f.id = s.file_id";

fn definition(answer: &Answer<'_>, name: &str) -> Result<Vec<Value>> {
    let mut id = name_id(answer.connection, name)?;
    if id.is_none()
        && let Some(stripped) = name.strip_prefix("CONFIG_")
    {
        id = name_id(answer.connection, stripped)?;
    }
    let Some(id) = id else { return Ok(Vec::new()) };
    collect(
        answer.connection,
        &format!("SELECT {SYMBOL_COLUMNS} {SYMBOL_JOINS} WHERE s.name_id = ?1 ORDER BY (s.storage = 'extern'), s.kind, f.path, s.line LIMIT ?2"),
        params![id, answer.fetch()],
        symbol_json,
    )
}

fn first_definition(connection: &Connection, name_id: i64) -> Result<Value> {
    let found: Option<(String, i64, String)> = connection
        .query_row(
            "SELECT f.path, s.line, k.name FROM symbols s JOIN files f ON f.id = s.file_id JOIN kinds k ON k.id = s.kind WHERE s.name_id = ?1 AND s.kind IN (1, 7, 8) ORDER BY (s.storage = 'extern'), s.id LIMIT 1",
            [name_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    Ok(found.map_or(Value::Null, |(path, line, kind)| json!({ "location": location(&path, line), "kind": kind })))
}

fn references(answer: &Answer<'_>, name: &str) -> Result<Value> {
    let Some(id) = name_id(answer.connection, name)? else {
        return Ok(answer.limited("refs", Vec::new(), json!({ "registrations": [], "exports": [] })));
    };
    let results = collect(
        answer.connection,
        "SELECT sn.name, sk.name, f.path, r.line, rk.name FROM refs r JOIN symbols s ON s.id = r.source JOIN names sn ON sn.id = s.name_id JOIN kinds sk ON sk.id = s.kind JOIN files f ON f.id = s.file_id JOIN kinds rk ON rk.id = r.kind WHERE r.name_id = ?1 ORDER BY f.path, r.line LIMIT ?2",
        params![id, answer.fetch()],
        |row| {
            let path: String = row.get(2)?;
            let line: i64 = row.get(3)?;
            Ok(json!({ "in": row.get::<_, String>(0)?, "in_kind": row.get::<_, String>(1)?, "file": path, "line": line, "location": location(&path, line), "references": row.get::<_, String>(4)? }))
        },
    )?;
    let registrations = collect(answer.connection, "SELECT r.macro, f.path, r.line FROM registrations r JOIN files f ON f.id = r.file_id WHERE r.name_id = ?1 ORDER BY f.path, r.line LIMIT ?2", params![id, answer.fetch()], |row| {
        let path: String = row.get(1)?;
        let line: i64 = row.get(2)?;
        Ok(json!({ "macro": row.get::<_, String>(0)?, "file": path, "line": line, "location": location(&path, line) }))
    })?;
    let exports = collect(answer.connection, "SELECT e.license, e.namespace, f.path, e.line FROM exports e JOIN files f ON f.id = e.file_id WHERE e.name_id = ?1 ORDER BY f.path, e.line LIMIT ?2", params![id, answer.fetch()], |row| {
        let path: String = row.get(2)?;
        let line: i64 = row.get(3)?;
        Ok(json!({ "license": row.get::<_, String>(0)?, "namespace": row.get::<_, String>(1)?, "file": path, "line": line, "location": location(&path, line) }))
    })?;
    Ok(answer.limited("refs", results, json!({ "registrations": registrations, "exports": exports })))
}

fn callers(answer: &Answer<'_>, name: &str) -> Result<Vec<Value>> {
    let Some(id) = name_id(answer.connection, name)? else { return Ok(Vec::new()) };
    collect(
        answer.connection,
        "SELECT sn.name, sk.name, f.path, c.line, s.line FROM calls c JOIN symbols s ON s.id = c.caller JOIN names sn ON sn.id = s.name_id JOIN kinds sk ON sk.id = s.kind JOIN files f ON f.id = s.file_id WHERE c.callee_id = ?1 ORDER BY f.path, c.line, sn.name LIMIT ?2",
        params![id, answer.fetch()],
        |row| {
            let path: String = row.get(2)?;
            let line: i64 = row.get(3)?;
            Ok(json!({ "caller": row.get::<_, String>(0)?, "kind": row.get::<_, String>(1)?, "file": path, "line": line, "location": location(&path, line), "caller_line": row.get::<_, i64>(4)? }))
        },
    )
}

fn callees(answer: &Answer<'_>, name: &str) -> Result<Vec<Value>> {
    let Some(id) = name_id(answer.connection, name)? else { return Ok(Vec::new()) };
    let rows: Vec<(String, i64, String, i64, i64)> = {
        let mut statement = answer.connection.prepare_cached(
            "SELECT cn.name, c.callee_id, f.path, c.line, s.line FROM symbols s JOIN calls c ON c.caller = s.id JOIN names cn ON cn.id = c.callee_id JOIN files f ON f.id = s.file_id WHERE s.name_id = ?1 AND s.kind IN (1, 8) ORDER BY f.path, c.line, cn.name LIMIT ?2",
        )?;
        let mapped = statement.query_map(params![id, answer.fetch()], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)))?;
        mapped.collect::<rusqlite::Result<_>>()?
    };
    rows.into_iter()
        .map(|(callee, callee_id, path, line, caller_line)| {
            Ok(json!({ "callee": callee, "file": path, "line": line, "location": location(&path, line), "caller_line": caller_line, "defined": first_definition(answer.connection, callee_id)? }))
        })
        .collect()
}

fn includes(answer: &Answer<'_>, path: &str) -> Result<Value> {
    let Some((id, path)) = file_id(answer.connection, path)? else { return Ok(answer.limited("includes", Vec::new(), json!({ "file": null }))) };
    let results = collect(
        answer.connection,
        "SELECT i.line, i.target, i.system, i.kind, r.path FROM includes i LEFT JOIN files r ON r.id = i.resolved WHERE i.file_id = ?1 ORDER BY i.line, i.id LIMIT ?2",
        params![id, answer.fetch()],
        |row| {
            let line: i64 = row.get(0)?;
            Ok(json!({ "line": line, "target": row.get::<_, String>(1)?, "system": row.get::<_, bool>(2)?, "kind": row.get::<_, String>(3)?, "resolved": row.get::<_, Option<String>>(4)? }))
        },
    )?;
    Ok(answer.limited("includes", results, json!({ "file": path })))
}

fn included_by(answer: &Answer<'_>, path: &str) -> Result<Value> {
    let Some((id, path)) = file_id(answer.connection, path)? else { return Ok(answer.limited("included-by", Vec::new(), json!({ "file": null }))) };
    let results = collect(
        answer.connection,
        "SELECT f.path, i.line, i.target FROM includes i JOIN files f ON f.id = i.file_id WHERE i.resolved = ?1 ORDER BY f.path, i.line LIMIT ?2",
        params![id, answer.fetch()],
        |row| {
            let path: String = row.get(0)?;
            let line: i64 = row.get(1)?;
            Ok(json!({ "file": path, "line": line, "location": location(&path, line), "target": row.get::<_, String>(2)? }))
        },
    )?;
    Ok(answer.limited("included-by", results, json!({ "file": path })))
}

fn file(answer: &Answer<'_>, path: &str) -> Result<Value> {
    let Some((id, path)) = file_id(answer.connection, path)? else { return Ok(answer.limited("file", Vec::new(), json!({ "file": null }))) };
    let info = answer.connection.query_row("SELECT language, bytes, lines, parse_errors FROM files WHERE id = ?1", [id], |row| {
        Ok(json!({ "path": path, "language": row.get::<_, String>(0)?, "bytes": row.get::<_, i64>(1)?, "lines": row.get::<_, i64>(2)?, "parse_errors": row.get::<_, i64>(3)? }))
    })?;
    let results = collect(
        answer.connection,
        &format!("SELECT {SYMBOL_COLUMNS} {SYMBOL_JOINS} WHERE s.file_id = ?1 ORDER BY s.line, s.id LIMIT ?2"),
        params![id, answer.fetch()],
        symbol_json,
    )?;
    Ok(answer.limited("file", results, json!({ "file": info })))
}

fn search(answer: &Answer<'_>, pattern: &str) -> Result<Vec<Value>> {
    let sql = |condition: &str, limit: &str| format!("SELECT {SYMBOL_COLUMNS} {SYMBOL_JOINS} WHERE {condition} ORDER BY n.name, f.path, s.line LIMIT {limit}");
    if pattern.contains(['*', '?', '[']) {
        collect(answer.connection, &sql("n.name GLOB ?1", "?2"), params![pattern, answer.fetch()], symbol_json)
    } else {
        let upper = format!("{pattern}\u{10ffff}");
        collect(answer.connection, &sql("n.name >= ?1 AND n.name < ?2", "?3"), params![pattern, upper, answer.fetch()], symbol_json)
    }
}

fn dedupe_push(list: &mut Vec<(String, Value)>, key: String, value: Value) {
    if !list.iter().any(|(existing, _)| *existing == key) {
        list.push((key, value));
    }
}

fn source_path(connection: &Connection, id: Option<i64>) -> Result<Option<String>> {
    let Some(id) = id else { return Ok(None) };
    Ok(connection.query_row("SELECT path FROM files WHERE id = ?1", [id], |row| row.get(0)).optional()?)
}

/// kind, gate, parent, target, source file id, makefile path, line, condition.
type KbuildEdge = (String, String, Option<String>, String, Option<i64>, String, i64, String);

fn config(answer: &Answer<'_>, requested: &str) -> Result<Value> {
    let connection = answer.connection;
    let symbol = requested.strip_prefix("CONFIG_").unwrap_or(requested);
    let empty = |found: bool| {
        json!({ "ok": true, "command": "config", "tree": answer.tree, "symbol": symbol, "found": found, "definitions": [], "depends_on": [], "selects": [], "implies": [], "defaults": [], "ranges": [], "selected_by": [], "implied_by": [], "depended_on_by": [], "kbuild": [] })
    };
    let Some(id) = name_id(connection, symbol)? else { return Ok(empty(false)) };
    let definitions = collect(
        connection,
        &format!("SELECT {SYMBOL_COLUMNS} {SYMBOL_JOINS} WHERE s.name_id = ?1 AND s.kind = {} ORDER BY f.path, s.line", SymbolKind::Kconfig.code()),
        [id],
        symbol_json,
    )?;
    let mut result = empty(!definitions.is_empty());
    let limit = answer.limit;

    let mut depends: Vec<(String, Value)> = Vec::new();
    let mut selects: Vec<(String, Value)> = Vec::new();
    let mut implies: Vec<(String, Value)> = Vec::new();
    let mut defaults: Vec<(String, Value)> = Vec::new();
    let mut ranges: Vec<(String, Value)> = Vec::new();
    let mut statement = connection.prepare_cached(
        "SELECT e.kind, tn.name, e.expr, e.via, e.line, f.path FROM config_edges e JOIN symbols s ON s.id = e.source JOIN files f ON f.id = s.file_id LEFT JOIN names tn ON tn.id = e.target_id WHERE s.name_id = ?1 AND s.kind = 10 ORDER BY f.path, e.line, e.rowid",
    )?;
    let rows = statement.query_map([id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, i64>(4)?, row.get::<_, String>(5)?))
    })?;
    for row in rows {
        let (kind, target, expr, via, line, path) = row?;
        let at = location(&path, line);
        match kind.as_str() {
            "depends" => dedupe_push(&mut depends, format!("{expr}|{via}|{at}"), json!({ "expr": expr, "via": via, "location": at })),
            "select" => dedupe_push(&mut selects, format!("{target:?}|{expr}"), json!({ "symbol": target, "condition": expr, "location": at })),
            "imply" => dedupe_push(&mut implies, format!("{target:?}|{expr}"), json!({ "symbol": target, "condition": expr, "location": at })),
            "default" => dedupe_push(&mut defaults, format!("{expr}|{at}"), json!({ "expr": expr, "location": at })),
            "range" => dedupe_push(&mut ranges, format!("{expr}|{at}"), json!({ "expr": expr, "location": at })),
            _ => {}
        }
    }
    let values = |list: Vec<(String, Value)>| Value::Array(list.into_iter().take(limit).map(|(_, value)| value).collect());

    let reverse = |kind: &str| -> Result<Value> {
        Ok(Value::Array(collect(
            connection,
            "SELECT sn.name, e.expr, f.path, e.line FROM config_edges e JOIN symbols s ON s.id = e.source JOIN names sn ON sn.id = s.name_id JOIN files f ON f.id = s.file_id WHERE e.target_id = ?1 AND e.kind = ?2 GROUP BY sn.name, e.expr ORDER BY sn.name LIMIT ?3",
            params![id, kind, limit as i64],
            |row| {
                let path: String = row.get(2)?;
                Ok(json!({ "symbol": row.get::<_, String>(0)?, "condition": row.get::<_, String>(1)?, "location": location(&path, row.get(3)?) }))
            },
        )?))
    };

    let mut kbuild = Vec::new();
    let mut sources: Vec<String> = Vec::new();
    let mut seen_sources: HashSet<String> = HashSet::new();
    let mut note_source = |path: Option<String>, sources: &mut Vec<String>| {
        if let Some(path) = path
            && seen_sources.insert(path.clone())
        {
            sources.push(path);
        }
    };
    let edges: Vec<KbuildEdge> = {
        let mut statement = connection.prepare_cached(
            "SELECT b.kind, b.gate, b.parent, b.target, b.source_file, f.path, b.line, b.condition FROM build_edges b JOIN files f ON f.id = b.file_id WHERE b.config_id = ?1 ORDER BY f.path, b.rowid LIMIT ?2",
        )?;
        let mapped = statement.query_map(params![id, limit as i64], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?)))?;
        mapped.collect::<rusqlite::Result<_>>()?
    };
    for (kind, _gate, parent, target, source_file, path, line, condition) in edges {
        let source = source_path(connection, source_file)?;
        note_source(source.clone(), &mut sources);
        let mut entry = json!({ "kind": kind, "target": target, "parent": parent, "makefile": location(&path, line), "condition": condition, "source": source });
        if target.ends_with(".o") && source_file.is_none() {
            let parts = collect(
                connection,
                "SELECT b.target, b.gate, cn.name, b.source_file, f.path, b.line FROM build_edges b JOIN files f ON f.id = b.file_id LEFT JOIN names cn ON cn.id = b.config_id WHERE b.parent = ?1 ORDER BY f.path, b.rowid LIMIT ?2",
                params![target, limit as i64],
                |row| Ok(json!({ "target": row.get::<_, String>(0)?, "gate": row.get::<_, String>(1)?, "config": row.get::<_, Option<String>>(2)?, "source_file": row.get::<_, Option<i64>>(3)?, "makefile": location(&row.get::<_, String>(4)?, row.get(5)?) })),
            )?;
            let mut resolved = Vec::new();
            for part in parts {
                let source = source_path(connection, part["source_file"].as_i64())?;
                note_source(source.clone(), &mut sources);
                let mut part = part;
                if let Some(map) = part.as_object_mut() {
                    map.remove("source_file");
                    map.insert("source".to_owned(), json!(source));
                }
                resolved.push(part);
            }
            entry["parts"] = Value::Array(resolved);
        }
        if let Some(directory) = target.strip_suffix('/') {
            let contents = collect(
                connection,
                "SELECT b.target, b.gate, cn.name, b.source_file, f.path, b.line FROM build_edges b JOIN files f ON f.id = b.file_id LEFT JOIN names cn ON cn.id = b.config_id WHERE b.dir = ?1 AND b.parent IS NULL ORDER BY b.rowid LIMIT ?2",
                params![directory, limit as i64],
                |row| Ok(json!({ "target": row.get::<_, String>(0)?, "gate": row.get::<_, String>(1)?, "config": row.get::<_, Option<String>>(2)?, "source_file": row.get::<_, Option<i64>>(3)?, "makefile": location(&row.get::<_, String>(4)?, row.get(5)?) })),
            )?;
            let mut resolved = Vec::new();
            for item in contents {
                let source = source_path(connection, item["source_file"].as_i64())?;
                note_source(source.clone(), &mut sources);
                let mut item = item;
                if let Some(map) = item.as_object_mut() {
                    map.remove("source_file");
                    map.insert("source".to_owned(), json!(source));
                }
                resolved.push(item);
            }
            entry["directory_contents"] = Value::Array(resolved);
        }
        kbuild.push(entry);
    }
    sources.truncate(limit);

    if let Some(map) = result.as_object_mut() {
        map.insert("definitions".to_owned(), Value::Array(definitions));
        map.insert("depends_on".to_owned(), values(depends));
        map.insert("selects".to_owned(), values(selects));
        map.insert("implies".to_owned(), values(implies));
        map.insert("defaults".to_owned(), values(defaults));
        map.insert("ranges".to_owned(), values(ranges));
        map.insert("selected_by".to_owned(), reverse("select")?);
        map.insert("implied_by".to_owned(), reverse("imply")?);
        map.insert("depended_on_by".to_owned(), reverse("depends")?);
        map.insert("kbuild".to_owned(), Value::Array(kbuild));
        map.insert("sources".to_owned(), json!(sources));
    }
    Ok(result)
}

fn call_path(answer: &Answer<'_>, from: &str, to: &str, depth: usize) -> Result<Value> {
    let connection = answer.connection;
    let (Some(start), Some(goal)) = (name_id(connection, from)?, name_id(connection, to)?) else {
        return Ok(answer.limited("path", Vec::new(), json!({ "found": false, "from": from, "to": to })));
    };
    let mut came_from: HashMap<i64, (i64, String, i64)> = HashMap::new();
    let mut visited: HashSet<i64> = HashSet::from([start]);
    let mut frontier: VecDeque<(i64, usize)> = VecDeque::from([(start, 0)]);
    let mut statement = connection.prepare_cached(
        "SELECT c.callee_id, f.path, c.line FROM symbols s JOIN calls c ON c.caller = s.id JOIN files f ON f.id = s.file_id WHERE s.name_id = ?1 AND s.kind IN (1, 8) ORDER BY c.callee_id, f.path, c.line",
    )?;
    let mut found = start == goal;
    'search: while let Some((current, distance)) = frontier.pop_front() {
        if distance >= depth || visited.len() > 200_000 {
            continue;
        }
        let neighbours: Vec<(i64, String, i64)> = statement.query_map([current], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
        for (callee, path, line) in neighbours {
            if visited.insert(callee) {
                came_from.insert(callee, (current, path, line));
                if callee == goal {
                    found = true;
                    break 'search;
                }
                frontier.push_back((callee, distance + 1));
            }
        }
    }
    let mut steps = Vec::new();
    if found {
        let mut cursor = goal;
        while cursor != start {
            let Some((previous, path, line)) = came_from.get(&cursor) else { break };
            steps.push(json!({ "name": name_of(connection, cursor)?, "called_at": location(path, *line), "called_from": name_of(connection, *previous)? }));
            cursor = *previous;
        }
        steps.push(json!({ "name": from, "called_at": null, "called_from": null }));
        steps.reverse();
    }
    Ok(answer.limited("path", steps, json!({ "found": found, "from": from, "to": to, "depth": depth })))
}

fn stats(answer: &Answer<'_>, meta: &Map<String, Value>) -> Result<Value> {
    let connection = answer.connection;
    let mut tables = Map::new();
    for table in ["files", "symbols", "includes", "calls", "refs", "exports", "registrations", "config_edges", "build_edges", "names"] {
        let count: i64 = connection.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0))?;
        tables.insert(table.to_owned(), json!(count));
    }
    let per_kind = collect(connection, "SELECT k.name, COUNT(*) FROM symbols s JOIN kinds k ON k.id = s.kind GROUP BY k.name ORDER BY k.id", [], |row| Ok(json!({ "kind": row.get::<_, String>(0)?, "count": row.get::<_, i64>(1)? })))?;
    let per_language = collect(connection, "SELECT language, COUNT(*), SUM(bytes), SUM(lines), SUM(parse_errors), SUM(parse_errors > 0) FROM files GROUP BY language ORDER BY language", [], |row| {
        Ok(json!({ "language": row.get::<_, String>(0)?, "files": row.get::<_, i64>(1)?, "bytes": row.get::<_, i64>(2)?, "lines": row.get::<_, i64>(3)?, "parse_errors": row.get::<_, i64>(4)?, "files_with_parse_errors": row.get::<_, i64>(5)? }))
    })?;
    let resolved: i64 = connection.query_row("SELECT COUNT(*) FROM includes WHERE resolved IS NOT NULL", [], |row| row.get(0))?;
    Ok(json!({
        "ok": true,
        "command": "stats",
        "tree": answer.tree,
        "meta": meta,
        "tables": tables,
        "symbols_by_kind": per_kind,
        "files_by_language": per_language,
        "includes_resolved": resolved,
    }))
}

pub fn run(command: &str, positional: &[String], db: &Path, limit: usize, depth: usize) -> Result<Value> {
    let known = ["stats", "def", "refs", "callers", "callees", "includes", "included-by", "file", "config", "search", "path"];
    if !known.contains(&command) {
        return Err(Error::message(format!("graph: unknown command `{command}`; expected build, {}", known.join(", "))));
    }
    let connection = open(db)?;
    let meta = meta(&connection)?;
    let answer = Answer { connection: &connection, tree: tree_label(&meta), limit };
    let argument = |index: usize, what: &str| -> Result<&str> {
        positional.get(index).map(String::as_str).ok_or_else(|| Error::message(format!("graph {command}: missing {what}")))
    };
    let extra = |name: &str, value: &str| json!({ name: value });
    Ok(match command {
        "stats" => stats(&answer, &meta)?,
        "def" => {
            let name = argument(0, "NAME")?;
            answer.limited("def", definition(&answer, name)?, extra("name", name))
        }
        "refs" => references(&answer, argument(0, "NAME")?)?,
        "callers" => {
            let name = argument(0, "NAME")?;
            answer.limited("callers", callers(&answer, name)?, extra("name", name))
        }
        "callees" => {
            let name = argument(0, "NAME")?;
            answer.limited("callees", callees(&answer, name)?, extra("name", name))
        }
        "includes" => includes(&answer, argument(0, "FILE")?)?,
        "included-by" => included_by(&answer, argument(0, "FILE")?)?,
        "file" => file(&answer, argument(0, "PATH")?)?,
        "config" => config(&answer, argument(0, "SYMBOL")?)?,
        "search" => {
            let pattern = argument(0, "PATTERN")?;
            answer.limited("search", search(&answer, pattern)?, extra("pattern", pattern))
        }
        _ => call_path(&answer, argument(0, "FROM")?, argument(1, "TO")?, depth)?,
    })
}
