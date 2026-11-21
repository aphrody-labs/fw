// SPDX-License-Identifier: Apache-2.0
//! `fw-browser`: the implementation of `fw browser`. Headless open, screenshot, eval and pdf over CDP, a DevTools server,
//! and a stdio MCP server, on the lite engine (Obscura: V8, WebAssembly, wgpu) or on Blink.

use std::io::{BufRead, Write};
use std::process::{Command, ExitCode, Stdio};

use fw_browser_engine::{Needs, pick};
use fw_browser_headless::session::{Engine, Page, SessionError, Viewport};
use serde_json::{Value, json};

const USAGE: &str = "fw browser: headless browser, WebAssembly and WebGPU first (engines: lite = Obscura on V8 and wgpu, blink = Chromium headless)

  fw browser engines                                   which engine binaries are found, and which one `auto` picks
  fw browser open <url> [--engine E] [--limit N]       title, final URL and visible text as JSON
  fw browser screenshot <url> <out.png> [--engine E] [--width N] [--height N] [--full]
  fw browser eval <url> <expression> [--engine E]      JSON value of the expression (promises are awaited)
  fw browser pdf <url> <out.pdf> [--engine E]
  fw browser cdp serve [--engine E] [--port N]         DevTools endpoint for Playwright, Puppeteer, agent-browser
  fw browser mcp [--engine E] [--passthrough]          stdio MCP server (tools: browser_open, browser_screenshot, browser_eval,
                                                       browser_pdf); --passthrough runs the engine's own MCP server instead

Engine E: lite (default when found), blink, auto. FW_BROWSER_LITE and FW_BROWSER_BLINK pin the binaries.";

struct Options {
    engine: Option<Engine>,
    positional: Vec<String>,
    width: Option<u32>,
    height: Option<u32>,
    limit: usize,
    port: Option<u16>,
    full: bool,
    passthrough: bool,
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mut options =
        Options { engine: None, positional: Vec::new(), width: None, height: None, limit: 20_000, port: None, full: false, passthrough: false };
    let mut iterator = args.iter();
    while let Some(arg) = iterator.next() {
        let mut value = |name: &str| iterator.next().cloned().ok_or_else(|| format!("{name} needs a value"));
        match arg.as_str() {
            "--engine" => {
                let name = value("--engine")?;
                options.engine = match name.as_str() {
                    "auto" => None,
                    other => Some(Engine::parse(other).ok_or_else(|| format!("unknown engine `{other}` (lite, blink, auto)"))?),
                };
            }
            "--width" => options.width = Some(value("--width")?.parse().map_err(|_| "--width is a number".to_owned())?),
            "--height" => options.height = Some(value("--height")?.parse().map_err(|_| "--height is a number".to_owned())?),
            "--limit" => options.limit = value("--limit")?.parse().map_err(|_| "--limit is a number".to_owned())?,
            "--port" => options.port = Some(value("--port")?.parse().map_err(|_| "--port is a number".to_owned())?),
            "--full" => options.full = true,
            "--passthrough" => options.passthrough = true,
            other if other.starts_with("--") => return Err(format!("unknown flag `{other}`")),
            other => options.positional.push(other.to_owned()),
        }
    }
    Ok(options)
}

/// `auto`: the lightest engine whose binary exists, as the catalog orders them (lite first, then Blink).
fn resolve(requested: Option<Engine>) -> Result<Engine, SessionError> {
    if let Some(engine) = requested {
        return Ok(engine);
    }
    let found: Vec<&'static str> = [Engine::Lite, Engine::Blink]
        .into_iter()
        .filter(|engine| engine.locate().is_ok())
        .map(|engine| match engine {
            Engine::Lite => "obscura",
            Engine::Blink => "blink",
        })
        .collect();
    let needs = Needs { javascript: true, paint: false, webgpu: false, media: false, cdp: None };
    match pick(&needs, &found, true).map(|backend| backend.id) {
        Some("obscura") => Ok(Engine::Lite),
        Some(_) => Ok(Engine::Blink),
        None => Engine::Lite.locate().map(|_| Engine::Lite),
    }
}

fn viewport(options: &Options) -> Option<Viewport> {
    match (options.width, options.height) {
        (None, None) => None,
        (width, height) => Some(Viewport { width: width.unwrap_or(1280), height: height.unwrap_or(800) }),
    }
}

fn write_file(path: &str, bytes: &[u8]) -> Result<Value, SessionError> {
    std::fs::write(path, bytes)?;
    Ok(json!({ "path": path, "bytes": bytes.len() }))
}

fn run(command: &str, options: &Options) -> Result<Value, SessionError> {
    let argument = |index: usize, what: &str| {
        options.positional.get(index).cloned().ok_or_else(|| SessionError::Page(format!("{command}: missing {what}")))
    };
    match command {
        "open" => {
            let engine = resolve(options.engine)?;
            let mut page = Page::open(engine, &argument(0, "<url>")?, viewport(options))?;
            let mut summary = page.summary(options.limit)?;
            summary["engine"] = json!(engine.id());
            Ok(summary)
        }
        "screenshot" => {
            let engine = resolve(options.engine)?;
            let mut page = Page::open(engine, &argument(0, "<url>")?, viewport(options))?;
            let bytes = page.screenshot(options.full)?;
            let mut written = write_file(&argument(1, "<out.png>")?, &bytes)?;
            written["engine"] = json!(engine.id());
            Ok(written)
        }
        "eval" => {
            let engine = resolve(options.engine)?;
            let mut page = Page::open(engine, &argument(0, "<url>")?, viewport(options))?;
            Ok(json!({ "engine": engine.id(), "value": page.eval(&argument(1, "<expression>")?)? }))
        }
        "pdf" => {
            let engine = resolve(options.engine)?;
            let mut page = Page::open(engine, &argument(0, "<url>")?, viewport(options))?;
            let bytes = page.pdf()?;
            let mut written = write_file(&argument(1, "<out.pdf>")?, &bytes)?;
            written["engine"] = json!(engine.id());
            Ok(written)
        }
        other => Err(SessionError::Page(format!("unknown command `{other}`"))),
    }
}

fn engines() -> Value {
    let report = |engine: Engine| match engine.locate() {
        Ok(path) => json!({ "id": engine.id(), "found": true, "path": path, "bytes": std::fs::metadata(&path).map(|m| m.len()).ok() }),
        Err(error) => json!({ "id": engine.id(), "found": false, "reason": error.to_string() }),
    };
    json!({
        "engines": [report(Engine::Lite), report(Engine::Blink)],
        "auto": resolve(None).ok().map(Engine::id),
    })
}

fn cdp_serve(options: &Options) -> Result<ExitCode, SessionError> {
    let engine = resolve(options.engine)?;
    let binary = engine.locate()?;
    let port = match options.port {
        Some(port) => port,
        None => 9222,
    };
    let profile = std::env::temp_dir().join(format!("fw-browser-cdp-{port}"));
    let args = fw_browser_headless::session::serve_arguments(engine, port, &profile, true);
    eprintln!("fw browser: {} on http://127.0.0.1:{port}/json/version", engine.id());
    let status = Command::new(&binary).args(args).status()?;
    Ok(ExitCode::from(status.code().unwrap_or(1).clamp(0, 255) as u8))
}

fn tool_definitions() -> Value {
    let url = json!({ "type": "string", "description": "http(s) or file URL" });
    let engine = json!({ "type": "string", "enum": ["lite", "blink", "auto"], "description": "lite = Obscura (V8, wgpu), blink = Chromium headless" });
    json!([
        { "name": "browser_open", "description": "Open a URL in a headless engine and return title, final URL and visible text.",
          "inputSchema": { "type": "object", "properties": { "url": url, "engine": engine, "limit": { "type": "integer" } }, "required": ["url"] } },
        { "name": "browser_eval", "description": "Open a URL and evaluate a JavaScript expression (promises awaited); returns its JSON value.",
          "inputSchema": { "type": "object", "properties": { "url": url, "expression": { "type": "string" }, "engine": engine }, "required": ["url", "expression"] } },
        { "name": "browser_screenshot", "description": "Open a URL and write a PNG screenshot to `path`.",
          "inputSchema": { "type": "object", "properties": { "url": url, "path": { "type": "string" }, "engine": engine, "width": { "type": "integer" }, "height": { "type": "integer" }, "full": { "type": "boolean" } }, "required": ["url", "path"] } },
        { "name": "browser_pdf", "description": "Open a URL and write a PDF to `path`.",
          "inputSchema": { "type": "object", "properties": { "url": url, "path": { "type": "string" }, "engine": engine }, "required": ["url", "path"] } },
        { "name": "browser_engines", "description": "Which engine binaries are installed and which one `auto` picks.",
          "inputSchema": { "type": "object", "properties": {} } },
    ])
}

fn call_tool(name: &str, arguments: &Value) -> Result<Value, SessionError> {
    let text = |key: &str| arguments.get(key).and_then(Value::as_str).map(str::to_owned);
    let number = |key: &str| arguments.get(key).and_then(Value::as_u64);
    let mut options = Options {
        engine: text("engine").and_then(|name| Engine::parse(&name)),
        positional: Vec::new(),
        width: number("width").map(|n| n as u32),
        height: number("height").map(|n| n as u32),
        limit: number("limit").map_or(20_000, |n| n as usize),
        port: None,
        full: arguments.get("full").and_then(Value::as_bool).unwrap_or(false),
        passthrough: false,
    };
    let positional = |keys: &[&str]| keys.iter().filter_map(|key| text(key)).collect::<Vec<_>>();
    match name {
        "browser_engines" => Ok(engines()),
        "browser_open" => {
            options.positional = positional(&["url"]);
            run("open", &options)
        }
        "browser_eval" => {
            options.positional = positional(&["url", "expression"]);
            run("eval", &options)
        }
        "browser_screenshot" => {
            options.positional = positional(&["url", "path"]);
            run("screenshot", &options)
        }
        "browser_pdf" => {
            options.positional = positional(&["url", "path"]);
            run("pdf", &options)
        }
        other => Err(SessionError::Page(format!("unknown tool `{other}`"))),
    }
}

fn mcp() -> std::io::Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let Ok(request) = serde_json::from_str::<Value>(&line) else { continue };
        let Some(id) = request.get("id").cloned() else { continue };
        let params = request.get("params").cloned().unwrap_or(Value::Null);
        let result = match request.get("method").and_then(Value::as_str) {
            Some("initialize") => Ok(json!({
                "protocolVersion": params.get("protocolVersion").and_then(Value::as_str).unwrap_or("2025-06-18"),
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "fw-browser", "version": env!("CARGO_PKG_VERSION") },
            })),
            Some("ping") => Ok(json!({})),
            Some("tools/list") => Ok(json!({ "tools": tool_definitions() })),
            Some("tools/call") => {
                let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
                let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                Ok(match call_tool(name, &arguments) {
                    Ok(value) => json!({ "content": [{ "type": "text", "text": value.to_string() }] }),
                    Err(error) => json!({ "content": [{ "type": "text", "text": error.to_string() }], "isError": true }),
                })
            }
            Some(other) => Err(json!({ "code": -32601, "message": format!("method not found: {other}") })),
            None => Err(json!({ "code": -32600, "message": "invalid request" })),
        };
        let reply = match result {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err(error) => json!({ "jsonrpc": "2.0", "id": id, "error": error }),
        };
        writeln!(stdout, "{reply}")?;
        stdout.flush()?;
    }
    Ok(())
}

fn mcp_passthrough(options: &Options) -> Result<ExitCode, SessionError> {
    let engine = options.engine.unwrap_or(Engine::Lite);
    if engine != Engine::Lite {
        return Err(SessionError::Page("--passthrough needs the lite engine, Blink has no MCP server of its own".to_owned()));
    }
    let status = Command::new(engine.locate()?).arg("mcp").stdin(Stdio::inherit()).stdout(Stdio::inherit()).status()?;
    Ok(ExitCode::from(status.code().unwrap_or(1).clamp(0, 255) as u8))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((command, rest)) = args.split_first() else {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    };
    if matches!(command.as_str(), "help" | "--help" | "-h") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let (command, rest) = match (command.as_str(), rest.split_first()) {
        ("cdp", Some((sub, tail))) if sub == "serve" => ("cdp-serve", tail),
        _ => (command.as_str(), rest),
    };
    let options = match parse(rest) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("fw browser: {message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let outcome = match command {
        "engines" => Ok::<_, SessionError>(ExitCode::SUCCESS).inspect(|_| println!("{}", engines())),
        "cdp-serve" => cdp_serve(&options),
        "mcp" if options.passthrough => mcp_passthrough(&options),
        "mcp" => mcp().map(|()| ExitCode::SUCCESS).map_err(SessionError::from),
        "open" | "screenshot" | "eval" | "pdf" => run(command, &options).map(|value| {
            println!("{value}");
            ExitCode::SUCCESS
        }),
        other => {
            eprintln!("fw browser: unknown command `{other}`\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    outcome.unwrap_or_else(|error| {
        eprintln!("fw browser {command}: {error}");
        ExitCode::FAILURE
    })
}
