//! Page templates. Prose is short and plain; every fact in it comes from `Data`.

use crate::{
    ApiSurface, CliCommand, CliHelp, CrateInfo, McpTool, Page, ReleaseTarget, Schema, Site, cell,
    code_cell, esc, markdown_section, short_hash,
};
use serde_json::{Value, json};
use std::fmt::Write as _;

pub struct Data<'a> {
    pub site: &'a Site,
    pub cli: &'a CliHelp,
    pub tools: &'a [McpTool],
    pub crates: &'a [CrateInfo],
    pub targets: &'a [ReleaseTarget],
    pub manifest: &'a Value,
    pub surfaces: &'a [ApiSurface],
    pub fw_product: &'a toml::Table,
    pub aphrody_product: &'a toml::Table,
    pub ay: &'a toml::Table,
    pub contracts: &'a str,
    pub install_host: &'a str,
}

const SECTION_ORDER: &[&str] = &[
    "Get started",
    "Guides",
    "Products",
    "CLI reference",
    "MCP tools",
    "API reference",
    "Crates",
    "Releases",
];

fn str_at<'a>(table: &'a toml::Table, path: &[&str]) -> &'a str {
    let mut current: Option<&toml::Value> = None;
    let mut map = Some(table);
    for key in path {
        current = map.and_then(|map| map.get(*key));
        map = current.and_then(toml::Value::as_table);
    }
    current.and_then(toml::Value::as_str).unwrap_or_default()
}

pub fn all_pages(data: &Data) -> Vec<Page> {
    let mut pages = vec![home(data)];
    pages.extend(start_pages(data));
    pages.extend(guide_pages(data));
    pages.extend(product_pages(data));
    pages.extend(cli_pages(data));
    pages.extend(mcp_pages(data));
    pages.extend(api_pages(data));
    pages.extend(crate_pages(data));
    pages.push(releases_page(data));
    pages
}

fn command_usage<'a>(cli: &'a CliHelp, name: &str) -> &'a str {
    cli.commands
        .iter()
        .find(|command| command.name == name)
        .map(|command| command.usage.as_str())
        .unwrap_or_default()
}

fn home(data: &Data) -> Page {
    let site = data.site;
    let host = data.install_host;
    let mut body = String::new();
    body.push_str("## Start in three commands\n\n```bash\n");
    let _ = writeln!(body, "curl -fsSL https://{host}/install.sh | sh");
    body.push_str("fw --version\nfw onboard\n```\n\n");
    body.push_str("On Windows, use PowerShell. The [install page](/start/install/) has both.\n\n");
    body.push_str(
        "## What these docs cover\n\n| Section | What you find | Pages |\n|---|---|---|\n",
    );
    let count = |prefix: &str| data_pages_with_prefix(data, prefix);
    let _ = writeln!(
        body,
        "| [Get started](/start/) | Install fw and run it, one action per step | 2 |\n| [CLI reference](/cli/) | Every `fw` command, read from `fw --help` | {} |\n| [MCP tools](/mcp/) | Every tool of the fw MCP server with its input schema | {} |\n| [API reference](/api/) | HTTP endpoints of `fw serve`, the edge Worker and ay | {} |\n| [Crates](/crates/) | Workspace crates from `cargo metadata` | {} |\n| [Products](/products/fw/) | fw, Aphrody and ay | 3 |\n| [Releases](/releases/) | Installers per platform and the live version | 1 |",
        data.cli.commands.len() + 1,
        data.tools.len() + 1,
        count("api"),
        count("crates"),
    );
    body.push_str("\n## For agents\n\nEvery page has a plain markdown copy: add `index.md` to the page address. [llms.txt](/llms.txt) lists all pages and [llms-full.txt](/llms-full.txt) holds them in one file. The [agents guide](/guides/agents/) shows how to connect an agent.\n");
    let mut page = Page::new(
        "index",
        "fw, Aphrody and ay: install and reference",
        &format!(
            "Install fw, read every command and MCP tool, and browse the crates of fw, Aphrody and ay. Generated from the code: fw {}.",
            data.cli.version
        ),
        "Get started",
        body,
    )
    .sourced("fw --help, cargo metadata, mcp tools/list", &format!("{}{}{}", data.cli.raw_root, data.tools.len(), data.crates.len()));
    page.label = Some("Home".to_owned());
    page.splash = Some(json!({
        "title": site.name,
        "tagline": site.tagline,
        "actions": [
            { "text": "Get started", "link": "/start/", "icon": "right-arrow", "variant": "primary" },
            { "text": "CLI reference", "link": "/cli/", "variant": "minimal" }
        ]
    }));
    page
}

fn data_pages_with_prefix(data: &Data, prefix: &str) -> usize {
    match prefix {
        "api" => data.surfaces.len() + 2,
        "crates" => crate_pages(data).len() + 1,
        _ => 0,
    }
}

fn os_name(os: &str) -> &str {
    match os {
        "linux" => "Linux",
        "windows" => "Windows",
        "macos" => "macOS",
        "android" => "Android",
        "browser" => "Browser (WebAssembly)",
        "wasi" => "WASI",
        "freestanding" => "Bare metal",
        other => other,
    }
}

fn latest(data: &Data, product: &str) -> Option<(String, String, String)> {
    let channels = data
        .manifest
        .pointer(&format!("/products/{product}"))?
        .as_object()?;
    let (channel, entry) = channels.iter().next()?;
    Some((
        channel.clone(),
        entry["version"].as_str()?.to_owned(),
        entry["date"].as_str().unwrap_or_default().to_owned(),
    ))
}

fn targets_table(targets: &[&ReleaseTarget]) -> String {
    let mut table = String::from(
        "| File | System | Architecture | ABI | Kind | Published |\n|---|---|---|---|---|---|\n",
    );
    for target in targets {
        let _ = writeln!(
            table,
            "| {} | {} | {} | {} | {} | {} |",
            code_cell(&target.asset),
            cell(os_name(&target.os)),
            cell(&target.arch),
            cell(&target.abi),
            cell(&target.kind),
            if target.ship { "yes" } else { "no" }
        );
    }
    table
}

fn start_pages(data: &Data) -> Vec<Page> {
    let host = data.install_host;
    let version = &data.cli.version;
    let onboard = command_usage(data.cli, "onboard");
    let serve = command_usage(data.cli, "serve");
    let mut steps = String::new();
    let _ = write!(
        steps,
        "Each step is one action. Do them in order.\n\n## 1. Open a terminal\n\nOn Windows, open PowerShell. On Linux, open any terminal.\n\n## 2. Install fw\n\nFollow the [install page](/start/install/) for your system. It takes one command.\n\n## 3. Check the version\n\nRun this command.\n\n```bash\nfw --version\n```\n\nThe output starts with `fw {version}` or a newer number.\n\n## 4. Check your machine\n\n```bash\nfw doctor\n```\n\nThe command checks the toolchain, `FX_HOME`, git, API keys and Ollama on port 11434. Fix what it reports, then run it again.\n\n## 5. Go to your project\n\n```bash\ncd path/to/your/project\n```\n\n## 6. Describe the project to agents\n\n```bash\nfw onboard\n```\n\nThe usage is `{onboard}`. The command describes the workspace for agents.\n\n## 7. Write the agent config files\n\n```bash\nfw onboard export all\n```\n\nThis writes `.mcp.json`, `opencode.json` and `.zed/settings.json` in the project. Open one and check that it lists the fw MCP server.\n\n## 8. Ask the agent a question\n\n```bash\nfw ask \"what does this project do?\"\n```\n\nThe agent needs a model endpoint. Run `fw models` to see the configured providers.\n\n## 9. Start the server\n\n```bash\nfw serve 127.0.0.1:8787\n```\n\nThe usage is `{serve}`. Open `http://127.0.0.1:8787/` in your browser to see the chat page. The server also answers on `/mcp`, `/acp` and `/a2a`: see the [API reference](/api/fw-serve/).\n\n## Next\n\n- [CLI reference](/cli/): every command.\n- [Agents guide](/guides/agents/): connect Claude Code, Zed or OpenCode.\n- [Learning guide](/guides/learning/): let fw learn from your repository.\n"
    );
    let start = Page::new(
        "start",
        "Get started with fw",
        &format!("Install fw {version}, check your machine and run your first agent command in nine single-action steps."),
        "Get started",
        steps,
    )
    .label("Steps")
    .sourced("fw --help, fw onboard --help, fw serve --help", &format!("{}{}{}", data.cli.version, onboard, serve));

    let fw_targets: Vec<&ReleaseTarget> = data
        .targets
        .iter()
        .filter(|target| target.product == "fw")
        .collect();
    let shipped = fw_targets.iter().filter(|target| target.ship).count();
    let held: Vec<String> = fw_targets
        .iter()
        .filter(|target| !target.ship)
        .map(|target| format!("`{}`", target.asset))
        .collect();
    let release_line = latest(data, "fw")
        .map(|(channel, version, date)| {
            format!(
                "The latest `{channel}` build is `{version}`, published {}.",
                date.split('T').next().unwrap_or_default()
            )
        })
        .unwrap_or_default();
    let mut install =
        String::from("import { Tabs, TabItem } from '@astrojs/starlight/components';\n\n");
    let _ = write!(
        install,
        "Install the `fw` binary. It is one file. {release_line}\n\n"
    );
    let mdx_tabs = format!(
        "<Tabs syncKey=\"os\">\n<TabItem label=\"Linux and macOS\">\n\n```bash\ncurl -fsSL https://{host}/install.sh | sh\n```\n\n</TabItem>\n<TabItem label=\"Windows\">\n\n```powershell\nirm https://{host}/install.ps1 | iex\n```\n\n</TabItem>\n</Tabs>\n\n"
    );
    let md_tabs = format!(
        "### Linux and macOS\n\n```bash\ncurl -fsSL https://{host}/install.sh | sh\n```\n\n### Windows\n\n```powershell\nirm https://{host}/install.ps1 | iex\n```\n\n"
    );
    let rest = format!(
        "Check the install:\n\n```bash\nfw --version\n```\n\n## Update and remove\n\n```bash\nfw self-update --check\nfw self-update\nfw self-uninstall --purge-data\n```\n\n`fw self-update` verifies the SHA-256 of the download and keeps the previous binary as `.prev`. `--purge-data` also removes `~/.fx`.\n\n## Release files\n\nThe installer picks the file for your system from this list. {shipped} of {} files are published.{}\n\n{}\n\nThe table comes from `release.toml`. The [releases page](/releases/) shows the live versions.\n",
        fw_targets.len(),
        if held.is_empty() {
            String::new()
        } else {
            format!(" Not published: {}.", held.join(", "))
        },
        targets_table(&fw_targets)
    );
    let mut install_page = Page::new(
        "start/install",
        "Install fw on Linux, macOS and Windows",
        "Install the fw binary with one command on Linux, macOS or Windows, then update or remove it. The file list comes from release.toml.",
        "Get started",
        format!("{install}{mdx_tabs}{rest}"),
    )
    .label("Install")
    .sourced("release.toml, releases manifest", &format!("{}{}", data.manifest, fw_targets.len()));
    install_page.body_md = Some(format!(
        "{}{md_tabs}{rest}",
        install.replace(
            "import { Tabs, TabItem } from '@astrojs/starlight/components';\n\n",
            ""
        )
    ));
    vec![start, install_page]
}

fn guide_pages_inner(data: &Data) -> Vec<Page> {
    let cli = data.cli;
    let skills = command_usage(cli, "skills");
    let onboard = command_usage(cli, "onboard");
    let mcp = command_usage(cli, "mcp");
    let agents = format!(
        "An agent works best when it can read the rules of a project and call its tools. fw gives it three things: instruction files, an MCP server and skills.\n\n## Instruction files\n\nPut an `AGENTS.md` at the root of the repository. It holds the rules an agent must follow: how to build, how to test, what not to touch. Claude Code reads `CLAUDE.md`; keep it as a short pointer to `AGENTS.md`.\n\nRun `{onboard}`. It describes the workspace for agents and can export configs for MCP, OpenCode and Zed.\n\n```bash\nfw onboard export all\n```\n\n## Plain-text docs\n\nThis site serves text for machines:\n\n- [llms.txt](/llms.txt) lists every page with a one-line note.\n- [llms-full.txt](/llms-full.txt) holds every page in one file.\n- Add `index.md` to any page address for the markdown source, for example `/cli/index.md`.\n\n## MCP\n\n`{mcp}` serves fw tools over stdio, and `fw serve` serves them over HTTP on `/mcp`. The server lists {} tools. Each has a page with its input schema: see the [MCP tools](/mcp/).\n\n## Skills\n\nSkills are folders with a `SKILL.md`. The command is `{skills}`.\n\n```bash\nfw skills list\nfw skills read <name>\n```\n\n## Check your setup\n\n1. Run `fw onboard export mcp` in your project.\n2. Open the `.mcp.json` it wrote.\n3. Start your agent and ask it to call `fw_status`.\n",
        data.tools.len()
    );
    let learn = command_usage(cli, "learn");
    let corpus = command_usage(cli, "corpus");
    let mem = command_usage(cli, "mem");
    let graph = command_usage(cli, "graph");
    let docs = command_usage(cli, "docs");
    let learning = format!(
        "fw learns locally. It reads your git history, a code graph and your docs, stores them in SQLite, and searches them without a network call.\n\n## Commands\n\n| Command | Usage |\n|---|---|\n| `fw learn` | {} |\n| `fw corpus` | {} |\n| `fw mem` | {} |\n| `fw graph` | {} |\n| `fw docs` | {} |\n\nThe table is read from `fw <command> --help`.\n\n## Learn from a repository\n\n1. Go to the repository.\n2. Harvest history, graph and docs.\n\n```bash\nfw learn harvest\n```\n\n3. Look at what was stored.\n\n```bash\nfw learn status\n```\n\n4. Search it.\n\n```bash\nfw learn search \"how are releases published\"\n```\n\n## Run it on a server\n\n`fw learn systemd` prints a systemd unit for a VPS. `fw learn loop` repeats harvest and distill.\n\n## Reference docs\n\n`fw corpus` keeps a polite, resumable copy of design, Android, Chromium, Rust and Cargo docs. `fw learn search` includes it. See the [`fw corpus`](/cli/corpus/) and [`fw learn`](/cli/learn/) pages.\n",
        cell(learn),
        cell(corpus),
        cell(mem),
        cell(graph),
        cell(docs)
    );
    let docs_site = format!(
        "This site is generated. A person edits code, not pages.\n\n## Commands\n\n```bash\nfw site gen      # write apps/docs from the code\nfw site check    # fail when a generated file differs from the code\nfw site llms     # write llms.txt and llms-full.txt\n```\n\n## Sources\n\n| Pages | Source |\n|---|---|\n| [CLI reference](/cli/) | `fw --help` and `fw <command> --help` of the built binary ({} commands) |\n| [MCP tools](/mcp/) | `tools/list` of the MCP server in `src/fw_cli.ts` ({} tools) |\n| [API reference](/api/) | `fw openapi`, `crates/fw-edge/data` and `ay/docs/auto/openapi.json` |\n| [Crates](/crates/) | `cargo metadata --no-deps` ({} packages) |\n| [Products](/products/fw/) | `product.toml`, `aphrody/product.toml` and `ay/ay.toml` |\n| [Releases](/releases/) | `release.toml` and the live release manifest |\n| Colors | `fw-m3 css` from the `[brand]` seed in `product.toml` |\n\nEvery page has a `generated` front-matter field with its inputs and a short hash. `fw site check` regenerates in memory and compares. A scheduled workflow refreshes the live site every six hours.\n",
        cli.commands.len(),
        data.tools.len(),
        data.crates.len()
    );
    vec![
        Page::new(
            "guides/agents",
            "Agents guide: AGENTS.md, MCP, skills",
            "Connect an AI agent to a project: AGENTS.md and CLAUDE.md, llms.txt, the fw MCP server and skills. Commands come from fw --help.",
            "Guides",
            agents,
        )
        .label("Agents")
        .sourced("fw --help, mcp tools/list", &format!("{onboard}{skills}{mcp}{}", data.tools.len())),
        Page::new(
            "guides/learning",
            "Learning guide: fw learn and the corpus",
            "Harvest git history, the code graph and docs into a local store and search it with fw learn. Usage lines are read from fw --help.",
            "Guides",
            learning,
        )
        .label("Learning")
        .sourced("fw learn --help, fw corpus --help", &format!("{learn}{corpus}{mem}{graph}{docs}")),
        Page::new(
            "guides/docs-site",
            "How this site is generated",
            "The docs are generated by fw site from fw --help, the MCP server, cargo metadata, product files and the release manifest, and checked for drift in CI.",
            "Guides",
            docs_site,
        )
        .label("How this site is built")
        .sourced("fw site", &format!("{}{}{}", cli.commands.len(), data.tools.len(), data.crates.len())),
    ]
}

fn kv_table(rows: &[(&str, String)]) -> String {
    let mut table = String::from("| Field | Value |\n|---|---|\n");
    for (key, value) in rows {
        if value.is_empty() {
            continue;
        }
        let _ = writeln!(table, "| {} | {} |", cell(key), value);
    }
    table
}

fn product_pages_inner(data: &Data) -> Vec<Page> {
    let stack: Vec<String> = data
        .ay
        .get("ecosystem")
        .and_then(|e| e.get("stack"))
        .and_then(toml::Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let position = |key: &str| {
        stack
            .iter()
            .position(|item| item == key)
            .map(|index| index + 1)
            .unwrap_or(0)
    };
    let repo_kind = |key: &str| str_at(data.ay, &["repos", key, "kind"]).to_owned();
    let repo_slug = |key: &str| str_at(data.ay, &["repos", key, "slug"]).to_owned();
    let kind_text = |kind: &str| {
        match kind {
            "toolchain" => "toolchain and agent runtime",
            "os" => "operating system and desktop",
            "sdk" => "SDK",
            other => other,
        }
        .to_owned()
    };
    let github = |slug: &str| format!("[{slug}](https://github.com/{slug})");
    let systems_of = |product: &str| {
        let mut systems: Vec<String> = Vec::new();
        for target in data
            .targets
            .iter()
            .filter(|t| t.product == product && t.ship)
        {
            let name = os_name(&target.os).to_owned();
            if !systems.contains(&name) {
                systems.push(name);
            }
        }
        systems
    };
    let release_rows = |product: &str| {
        let rows: Vec<&ReleaseTarget> = data
            .targets
            .iter()
            .filter(|t| t.product == product)
            .collect();
        if rows.is_empty() {
            String::new()
        } else {
            format!(
                "\n## Release files\n\n{}\nSource: `release.toml`.\n",
                targets_table(&rows)
            )
        }
    };
    let crate_count = |repo: &str| data.crates.iter().filter(|c| c.repo == repo).count();
    let software = |name: &str, product: &str, version: &str, repository: &str| Schema::Software {
        name: name.to_owned(),
        version: version.to_owned(),
        systems: systems_of(product),
        download: format!("{}/start/install/", data.site.url),
        repository: repository.to_owned(),
    };

    // fw
    let fw_version = latest(data, "fw")
        .map(|(_, version, _)| version)
        .unwrap_or_else(|| str_at(data.fw_product, &["version"]).to_owned());
    let fw_body = format!(
        "`fw` is the {} of the Aphrody stack, number {} of {}. It is one binary with a coding agent, a Hugging Face client, MCP, ACP and A2A servers, a package manager and local learning.\n\n{}\n## Interfaces\n\n- [CLI reference](/cli/): {} commands.\n- [MCP tools](/mcp/): {} tools.\n- [API reference](/api/fw-serve/): `fw serve` endpoints.\n- [Crates](/crates/list-fw/): {} workspace crates.\n\n## Install\n\nSee [install fw](/start/install/).\n{}",
        kind_text(&repo_kind("fw")),
        position("fw"),
        stack.len(),
        kv_table(&[
            ("Name", code_cell(str_at(data.fw_product, &["name"]))),
            (
                "Product version",
                code_cell(str_at(data.fw_product, &["version"]))
            ),
            ("Latest release", code_cell(&fw_version)),
            ("Binary", code_cell(str_at(data.fw_product, &["binary"]))),
            ("License", cell(str_at(data.fw_product, &["license"]))),
            ("Repository", github(&repo_slug("fw"))),
            (
                "Brand seed",
                code_cell(str_at(data.fw_product, &["brand", "seed"]))
            ),
        ]),
        data.cli.commands.len(),
        data.tools.len(),
        crate_count("fw"),
        release_rows("fw")
    );
    let mut fw_page = Page::new(
        "products/fw",
        "fw: toolchain and agent runtime",
        &format!("fw is a Rust toolchain and agent runtime: one binary with a coding agent, MCP, ACP and A2A servers, and local learning. Version {fw_version}."),
        "Products",
        fw_body,
    )
    .label("fw")
    .sourced("product.toml, release.toml, ay/ay.toml", &format!("{}{}{}", data.fw_product, fw_version, data.targets.len()));
    fw_page.schema = software("fw", "fw", &fw_version, &repo_slug("fw"));

    // aphrody
    let ap = data.aphrody_product;
    let mut urls = String::from("| Name | Address |\n|---|---|\n");
    if let Some(table) = ap.get("urls").and_then(toml::Value::as_table) {
        for (key, value) in table {
            if let Some(url) = value.as_str() {
                let _ = writeln!(urls, "| {} | {} |", cell(key), code_cell(url));
            }
        }
    }
    let ap_version = latest(data, "aphrody")
        .map(|(_, version, _)| version)
        .unwrap_or_default();
    let ap_body = format!(
        "{}\n\nAphrody is the {} of the stack, number {} of {}. It is a fork of Zed built on GPUI.\n\n{}\n## Addresses\n\n{}\n## Crates\n\n{} workspace crates live under `aphrody/crates`. See [the list](/crates/list-aphrody/).\n{}",
        esc(str_at(ap, &["product", "description"])),
        kind_text(&repo_kind("aphrody")),
        position("aphrody"),
        stack.len(),
        kv_table(&[
            ("Name", cell(str_at(ap, &["product", "name"]))),
            ("Tagline", cell(str_at(ap, &["product", "tagline"]))),
            ("Company", cell(str_at(ap, &["product", "company_name"]))),
            (
                "URL scheme",
                code_cell(&format!("{}://", str_at(ap, &["product", "url_scheme"])))
            ),
            ("Repository", github(&repo_slug("aphrody"))),
        ]),
        urls,
        crate_count("aphrody"),
        release_rows("aphrody")
    );
    let mut ap_page = Page::new(
        "products/aphrody",
        "Aphrody: OS and desktop",
        &format!(
            "{} Reference of the Aphrody product: identity, addresses and release files.",
            str_at(ap, &["product", "description"])
        ),
        "Products",
        ap_body,
    )
    .label("Aphrody")
    .sourced(
        "aphrody/product.toml, release.toml",
        &format!("{ap}{}", data.targets.len()),
    );
    ap_page.schema = software("Aphrody", "aphrody", &ap_version, &repo_slug("aphrody"));

    // ay
    let mut repos = String::from("| Repository | GitHub | Kind |\n|---|---|---|\n");
    if let Some(table) = data.ay.get("repos").and_then(toml::Value::as_table) {
        let mut keys: Vec<&String> = table.keys().collect();
        keys.sort_by_key(|key| {
            (
                stack
                    .iter()
                    .position(|item| item == *key)
                    .unwrap_or(usize::MAX),
                (*key).clone(),
            )
        });
        for key in keys {
            let _ = writeln!(
                repos,
                "| {} | {} | {} |",
                code_cell(key),
                github(&repo_slug(key)),
                cell(&repo_kind(key))
            );
        }
    }
    let ay_version = latest(data, "ay")
        .map(|(_, version, _)| version)
        .unwrap_or_default();
    let ay_body = format!(
        "`{}` is the {} of the stack, number {} of {}: C ABI, corelib, OpenAPI, design tokens and the cloud and edge services.\n\n{}\n## Repositories\n\nThe stack order is {}. Every repository of the organization is listed in `ay/ay.toml`.\n\n{}\n## Crates\n\n{} workspace crates live under `ay/crates`. See [the list](/crates/list-ay/).\n{}",
        str_at(data.ay, &["product", "cli"]),
        kind_text(&repo_kind("ay")),
        position("ay"),
        stack.len(),
        kv_table(&[
            ("Name", cell(str_at(data.ay, &["product", "display"]))),
            ("CLI", code_cell(str_at(data.ay, &["product", "cli"]))),
            ("Owner", cell(str_at(data.ay, &["product", "owner"]))),
            (
                "Root domain",
                code_cell(str_at(data.ay, &["domains", "root"]))
            ),
            (
                "npm scope",
                code_cell(str_at(data.ay, &["registries", "npm"]))
            ),
            ("Repository", github(&repo_slug("ay"))),
        ]),
        stack
            .iter()
            .map(|key| format!("`{key}`"))
            .collect::<Vec<_>>()
            .join(", "),
        repos,
        crate_count("ay"),
        release_rows("ay")
    );
    let mut ay_page = Page::new(
        "products/ay",
        "ay: SDK, cloud and edge",
        "ay is the SDK of the Aphrody stack: C ABI, corelib, OpenAPI, design tokens, cloud and edge services. Repository list and release files.",
        "Products",
        ay_body,
    )
    .label("ay")
    .sourced("ay/ay.toml, release.toml", &format!("{}{}", data.ay, data.targets.len()));
    ay_page.schema = software("ay", "ay", &ay_version, &repo_slug("ay"));
    vec![fw_page, ap_page, ay_page]
}

fn cli_slug(name: &str) -> String {
    format!("cli/{name}")
}

fn cli_pages(data: &Data) -> Vec<Page> {
    let cli = data.cli;
    let mut pages = Vec::new();
    let mut groups: Vec<&str> = Vec::new();
    for command in &cli.commands {
        if !groups.contains(&command.group.as_str()) {
            groups.push(&command.group);
        }
    }
    let mut index = format!(
        "`fw` is one binary. `fw --help` lists the commands and `fw <command> --help` shows the usage of one. This section has a page for each of the {} commands, read from those outputs (fw {}).\n\n## Global options\n\nThese options work with every command.\n\n| Option | Meaning |\n|---|---|\n",
        cli.commands.len(),
        cli.version
    );
    for option in &cli.global_options {
        let _ = writeln!(
            index,
            "| {} | {} |",
            code_cell(&option.flags),
            cell(&option.description)
        );
    }
    for group in &groups {
        let _ = write!(
            index,
            "\n## {}\n\n| Command | Aliases | What it does |\n|---|---|---|\n",
            esc(group)
        );
        for command in cli.commands.iter().filter(|c| c.group == *group) {
            let _ = writeln!(
                index,
                "| [`fw {}`](/{}/) | {} | {} |",
                command.name,
                cli_slug(&command.name),
                command
                    .aliases
                    .iter()
                    .map(|a| format!("`{a}`"))
                    .collect::<Vec<_>>()
                    .join(", "),
                cell(&command.summary)
            );
        }
    }
    pages.push(
        Page::new(
            "cli",
            "fw command reference",
            &format!(
                "All {} fw commands with usage, aliases and flags, read from fw --help. fw {}.",
                cli.commands.len(),
                cli.version
            ),
            "CLI reference",
            index,
        )
        .label("Overview")
        .sourced("fw --help", &cli.raw_root),
    );
    for command in &cli.commands {
        pages.push(cli_page(data, command));
    }
    pages
}

fn cli_page(data: &Data, command: &CliCommand) -> Page {
    let name = &command.name;
    let mut body = format!(
        "{}\n\n## Usage\n\n```bash\n{}\n```\n",
        esc(&command.summary),
        command.usage
    );
    if !command.aliases.is_empty() {
        let _ = write!(
            body,
            "\n## Aliases\n\n{}\n",
            command
                .aliases
                .iter()
                .map(|alias| format!("`fw {alias}` runs `fw {name}`."))
                .collect::<Vec<_>>()
                .join(" ")
        );
    }
    if !command.subcommands.is_empty() {
        let _ = write!(body, "\n## Subcommands\n\n```bash\n");
        for sub in &command.subcommands {
            let _ = writeln!(body, "fw {name} {sub}");
        }
        body.push_str("```\n\nThe names come from the usage line.\n");
    }
    if command.flags.iter().any(|flag| !flag.argument.is_empty()) {
        body.push_str("\n## Flags\n\n| Flag | Argument |\n|---|---|\n");
        for flag in &command.flags {
            let _ = writeln!(body, "| `{}` | {} |", flag.name, code_cell(&flag.argument));
        }
    } else if !command.flags.is_empty() {
        let _ = write!(
            body,
            "\n## Flags\n\nThe help text names these flags: {}.\n",
            command
                .flags
                .iter()
                .map(|flag| format!("`{}`", flag.name))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    body.push_str("\n## Global options\n\nEvery command also accepts `-C`, `--directory`, `--offline`, `--json`, `-q` and `-v`. See the [overview](/cli/).\n");
    let related: Vec<String> = data
        .cli
        .commands
        .iter()
        .filter(|other| other.group == command.group && other.name != command.name)
        .take(8)
        .map(|other| format!("[`fw {}`](/{}/)", other.name, cli_slug(&other.name)))
        .collect();
    if !related.is_empty() {
        let _ = write!(
            body,
            "\n## Same group: {}\n\n{}\n",
            esc(&command.group),
            related.join(", ")
        );
    }
    let _ = write!(
        body,
        "\n## Source\n\nRead from `fw {name} --help` of fw {}.\n",
        data.cli.version
    );
    Page::new(
        &cli_slug(name),
        &format!("fw {name}"),
        &format!(
            "Reference of the fw {name} command: {}. Usage, aliases and flags.",
            command.summary
        ),
        "CLI reference",
        body,
    )
    .label(name)
    .sourced(&format!("fw {name} --help"), &command.raw)
}

fn mcp_slug(name: &str) -> String {
    format!("mcp/{}", name.replace('_', "-"))
}

fn schema_type(property: &Value) -> String {
    let base = property
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("any")
        .to_owned();
    match property.get("enum").and_then(Value::as_array) {
        Some(values) => format!(
            "{base}: {}",
            values
                .iter()
                .map(|v| v
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| v.to_string()))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        None => base,
    }
}

fn example_arguments(schema: &Value) -> Value {
    let mut arguments = serde_json::Map::new();
    let required: Vec<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        for (key, property) in properties {
            if !required.contains(&key.as_str()) {
                continue;
            }
            let value = if let Some(first) = property
                .get("enum")
                .and_then(Value::as_array)
                .and_then(|v| v.first())
            {
                first.clone()
            } else {
                match property.get("type").and_then(Value::as_str) {
                    Some("integer") | Some("number") => {
                        json!(property.get("minimum").cloned().unwrap_or(json!(1)))
                    }
                    Some("boolean") => json!(true),
                    Some("array") => json!([]),
                    Some("object") => json!({}),
                    _ => json!(format!("<{key}>")),
                }
            };
            arguments.insert(key.clone(), value);
        }
    }
    Value::Object(arguments)
}

fn mcp_pages(data: &Data) -> Vec<Page> {
    let mut pages = Vec::new();
    let mut index = format!(
        "The fw MCP server exposes {} tools over JSON-RPC 2.0 on stdio. Start it from a fw checkout with `bun src/fw_cli.ts mcp`, or use `fw serve` and post to `/mcp`.\n\n## Tools\n\n| Tool | What it does |\n|---|---|\n",
        data.tools.len()
    );
    for tool in data.tools {
        let _ = writeln!(
            index,
            "| [`{}`](/{}/) | {} |",
            tool.name,
            mcp_slug(&tool.name),
            cell(&tool.description)
        );
    }
    index.push_str("\n## Call a tool\n\nSend `initialize`, then `tools/list`, then `tools/call` with the tool name and its arguments. Each tool page has a ready example.\n\n## How this list is made\n\n`fw site` starts the server, sends `tools/list` and reads the answer. A tool added to `src/mcp_server.ts` appears here on the next run.\n");
    let all_names: String = data
        .tools
        .iter()
        .map(|t| t.name.as_str())
        .collect::<Vec<_>>()
        .join(",");
    pages.push(
        Page::new(
            "mcp",
            "fw MCP tools",
            &format!("The {} tools of the fw MCP server with descriptions and input schemas, read from tools/list.", data.tools.len()),
            "MCP tools",
            index,
        )
        .label("Overview")
        .sourced("mcp tools/list", &all_names),
    );
    for tool in data.tools {
        pages.push(mcp_page(tool));
    }
    pages
}

fn mcp_page(tool: &McpTool) -> Page {
    let mut body = format!("{}\n\n", esc(&tool.description));
    let call = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": tool.name, "arguments": example_arguments(&tool.schema) }
    });
    let _ = write!(
        body,
        "## Call it\n\n```json\n{}\n```\n",
        serde_json::to_string_pretty(&call).unwrap_or_default()
    );
    let required: Vec<&str> = tool
        .schema
        .get("required")
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    match tool.schema.get("properties").and_then(Value::as_object) {
        Some(properties) if !properties.is_empty() => {
            body.push_str(
                "\n## Parameters\n\n| Name | Type | Required | Description |\n|---|---|---|---|\n",
            );
            for (key, property) in properties {
                let _ = writeln!(
                    body,
                    "| `{key}` | {} | {} | {} |",
                    code_cell(&schema_type(property)),
                    if required.contains(&key.as_str()) {
                        "yes"
                    } else {
                        "no"
                    },
                    cell(
                        property
                            .get("description")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                    )
                );
            }
        }
        _ => body.push_str("\n## Parameters\n\nThis tool takes no parameters.\n"),
    }
    let _ = write!(
        body,
        "\n## Input schema\n\n```json\n{}\n```\n\n## Server\n\nPart of the fw MCP server. See the [tool list](/mcp/) and the [agents guide](/guides/agents/).\n",
        serde_json::to_string_pretty(&tool.schema).unwrap_or_default()
    );
    Page::new(
        &mcp_slug(&tool.name),
        &format!("{} MCP tool", tool.name),
        &format!(
            "{} Input schema, parameters and a tools/call example for the {} MCP tool.",
            tool.description, tool.name
        ),
        "MCP tools",
        body,
    )
    .label(&tool.name)
    .sourced(
        "mcp tools/list",
        &format!("{}{}{}", tool.name, tool.description, tool.schema),
    )
}

fn api_pages(data: &Data) -> Vec<Page> {
    let mut pages = Vec::new();
    let mut index = String::from(
        "Each page lists the HTTP endpoints of one surface, read from its OpenAPI data. The standards page names the contract formats and the tools that generate them.\n\n| Surface | Endpoints | Source |\n|---|---|---|\n",
    );
    for surface in data.surfaces {
        let _ = writeln!(
            index,
            "| [{}](/api/{}/) | {} | {} |",
            cell(&surface.title),
            surface.key,
            surface.endpoints.len(),
            code_cell(&surface.source)
        );
    }
    index.push_str("| [Standards and contract sources](/api/standards/) | | `docs/research/api-contracts-2026-11-21.md` |\n");
    pages.push(
        Page::new(
            "api",
            "API reference",
            &format!("HTTP endpoints of fw serve, the fw edge Worker and the ay Auto API, with the contract standards the stack follows. {} surfaces.", data.surfaces.len()),
            "API reference",
            index,
        )
        .label("Overview")
        .sourced("fw openapi, openapi data files", &data.surfaces.iter().map(|s| s.title.clone() + &s.endpoints.len().to_string()).collect::<String>()),
    );
    for surface in data.surfaces {
        let mut body = format!("{}\n\n", esc(&surface.description));
        if surface.key == "fw-serve" {
            let _ = write!(
                body,
                "Start the server with `{}`. The chat page is served on `/`.\n\n",
                command_usage(data.cli, "serve")
            );
        }
        body.push_str("## Endpoints\n\n| Method | Path | Summary |\n|---|---|---|\n");
        for endpoint in &surface.endpoints {
            let _ = writeln!(
                body,
                "| `{}` | `{}` | {} |",
                endpoint.method,
                endpoint.path,
                cell(&endpoint.summary)
            );
        }
        let _ = write!(body, "\n## Source\n\nRead from `{}`.\n", surface.source);
        let slug = format!("api/{}", surface.key);
        let material: String = surface
            .endpoints
            .iter()
            .map(|e| format!("{}{}{}", e.method, e.path, e.summary))
            .collect();
        pages.push(
            Page::new(
                &slug,
                &surface.title,
                &format!(
                    "{} endpoints of the {}: method, path and summary. {}",
                    surface.endpoints.len(),
                    surface.title,
                    surface.description
                ),
                "API reference",
                body,
            )
            .sourced(&surface.source, &material),
        );
    }
    let mut standards = String::new();
    for (heading, intro) in [
        (
            "Current standards",
            "The formats the stack follows, with the live version read on the date of the research.",
        ),
        (
            "What exists today (origin/main of each repo)",
            "The contracts present in each repository today.",
        ),
    ] {
        if let Some(section) = markdown_section(data.contracts, heading) {
            let _ = write!(
                standards,
                "## {}\n\n{intro}\n\n{}\n\n",
                esc(heading.split(" (").next().unwrap_or(heading)),
                esc(&section)
            );
        }
    }
    if standards.is_empty() {
        standards.push_str("The contracts document is not available in this checkout.\n");
    }
    pages.push(
        Page::new(
            "api/standards",
            "API standards and contract sources",
            "OpenAPI, GraphQL, AsyncAPI, JSON Schema, Protocol Buffers, MCP and A2A versions the Aphrody stack follows, and the contracts each repository ships today.",
            "API reference",
            standards,
        )
        .label("Standards")
        .sourced("docs/research/api-contracts-2026-11-21.md", data.contracts),
    );
    pages
}

fn crate_link(data: &Data, name: &str) -> String {
    match data.crates.iter().find(|c| c.name == name) {
        Some(krate) if has_page(krate) => format!("[`{name}`](/crates/{name}/)"),
        _ => format!("`{name}`"),
    }
}

fn has_page(krate: &CrateInfo) -> bool {
    !krate.description.is_empty() || krate.repo != "aphrody"
}

fn repo_label(repo: &str) -> &str {
    match repo {
        "aphrody" => "Aphrody crates",
        "ay" => "ay crates",
        _ => "fw crates",
    }
}

fn crate_pages(data: &Data) -> Vec<Page> {
    let mut pages = Vec::new();
    let repository = data.site.repository.trim_end_matches('/');
    let mut index = format!(
        "The Cargo workspace has {} packages. They come from `cargo metadata --no-deps`.\n\n| Repository | Packages | With a page | List |\n|---|---|---|---|\n",
        data.crates.len()
    );
    for repo in ["fw", "ay", "aphrody"] {
        let all = data.crates.iter().filter(|c| c.repo == repo).count();
        let with_page = data
            .crates
            .iter()
            .filter(|c| c.repo == repo && has_page(c))
            .count();
        let _ = writeln!(
            index,
            "| {} | {all} | {with_page} | [full list](/crates/list-{repo}/) |",
            cell(repo_label(repo))
        );
    }
    index.push_str("\nA crate gets its own page when it belongs to fw or ay, or when it has a description. The lists show all packages.\n");
    pages.push(
        Page::new(
            "crates",
            "Workspace crates",
            &format!("The {} crates of the fw, Aphrody and ay Cargo workspace, listed from cargo metadata, with versions, descriptions and dependencies.", data.crates.len()),
            "Crates",
            index,
        )
        .label("Overview")
        .sourced("cargo metadata --no-deps", &data.crates.iter().map(|c| format!("{}{}", c.name, c.version)).collect::<String>()),
    );
    for repo in ["fw", "ay", "aphrody"] {
        let mut body = format!(
            "All packages of the {} part of the workspace.\n\n| Package | Version | Description |\n|---|---|---|\n",
            repo_label(repo)
        );
        let members: Vec<&CrateInfo> = data.crates.iter().filter(|c| c.repo == repo).collect();
        for krate in &members {
            let description = if krate.description.is_empty() {
                String::new()
            } else {
                cell(&krate.description)
            };
            let _ = writeln!(
                body,
                "| {} | {} | {description} |",
                crate_link(data, &krate.name),
                code_cell(&krate.version)
            );
        }
        let material: String = members
            .iter()
            .map(|c| format!("{}{}{}", c.name, c.version, c.description))
            .collect();
        pages.push(
            Page::new(
                &format!("crates/list-{repo}"),
                &format!("{} list", repo_label(repo)),
                &format!("Every package of the {} part of the Cargo workspace: {} crates with version and description.", repo_label(repo), members.len()),
                "Crates",
                body,
            )
            .label(&format!("All {} packages", repo_label(repo).to_lowercase().replace(" crates", "")))
            .sourced("cargo metadata --no-deps", &material),
        );
    }
    for krate in data.crates.iter().filter(|c| has_page(c)) {
        let mut body = String::new();
        if krate.description.is_empty() {
            let _ = writeln!(
                body,
                "`{}` is a package of the {} part of the workspace. It has no description in its manifest.\n",
                krate.name,
                repo_label(&krate.repo)
            );
        } else {
            let _ = writeln!(body, "{}\n", esc(&krate.description));
        }
        body.push_str(&format!(
            "## Package\n\n{}\n",
            kv_table(&[
                ("Version", code_cell(&krate.version)),
                ("Part of", cell(repo_label(&krate.repo))),
                (
                    "Source",
                    format!("[`{}`]({repository}/tree/main/{})", krate.path, krate.path)
                ),
                ("License", cell(&krate.license)),
                ("Edition", code_cell(&krate.edition)),
                (
                    "Keywords",
                    krate
                        .keywords
                        .iter()
                        .map(|k| format!("`{k}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ])
        ));
        if !krate.targets.is_empty() {
            body.push_str("\n## Targets\n\n| Kind | Name |\n|---|---|\n");
            for (kind, name) in &krate.targets {
                let _ = writeln!(body, "| {} | {} |", cell(kind), code_cell(name));
            }
        }
        if !krate.dependencies.is_empty() {
            let _ = write!(
                body,
                "\n## Workspace dependencies\n\n{}. It also uses {} crates from outside the workspace.\n",
                krate
                    .dependencies
                    .iter()
                    .map(|d| crate_link(data, d))
                    .collect::<Vec<_>>()
                    .join(", "),
                krate.external_dependencies
            );
        } else {
            let _ = write!(
                body,
                "\n## Dependencies\n\nNo workspace dependencies. It uses {} crates from outside the workspace.\n",
                krate.external_dependencies
            );
        }
        if !krate.dependents.is_empty() {
            let _ = write!(
                body,
                "\n## Used by\n\n{}\n",
                krate
                    .dependents
                    .iter()
                    .map(|d| crate_link(data, d))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        if !krate.features.is_empty() {
            let _ = write!(
                body,
                "\n## Features\n\n{}\n",
                krate
                    .features
                    .iter()
                    .map(|f| format!("`{f}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        let _ = write!(
            body,
            "\n## Build\n\nRun this from the repository root.\n\n```bash\ncargo build --release -p {}\n```\n",
            krate.name
        );
        let description = if krate.description.is_empty() {
            format!(
                "Package {} version {} of the {} part of the Cargo workspace: targets, dependencies and dependents.",
                krate.name,
                krate.version,
                repo_label(&krate.repo)
            )
        } else {
            format!(
                "{} Version {}, targets and dependencies.",
                krate.description, krate.version
            )
        };
        pages.push(
            Page::new(
                &format!("crates/{}", krate.name),
                &format!("{} crate", krate.name),
                &description,
                &format!("Crates/{}", repo_label(&krate.repo)),
                body,
            )
            .label(&krate.name)
            .sourced(
                "cargo metadata --no-deps",
                &format!(
                    "{}{}{}{:?}{:?}",
                    krate.name,
                    krate.version,
                    krate.description,
                    krate.dependencies,
                    krate.dependents
                ),
            ),
        );
    }
    pages
}

fn releases_page(data: &Data) -> Page {
    let mut body = String::from(
        "The installers read the release manifest. This page shows what it held when the site was built.\n\n## Latest versions\n\n| Product | Channel | Version | Tag | Date |\n|---|---|---|---|---|\n",
    );
    if let Some(products) = data.manifest.get("products").and_then(Value::as_object) {
        for (product, channels) in products {
            for (channel, entry) in channels.as_object().into_iter().flatten() {
                let _ = writeln!(
                    body,
                    "| {} | {} | {} | {} | {} |",
                    cell(product),
                    cell(channel),
                    code_cell(entry["version"].as_str().unwrap_or_default()),
                    code_cell(entry["tag"].as_str().unwrap_or_default()),
                    cell(entry["date"].as_str().unwrap_or_default())
                );
            }
        }
    }
    let _ = write!(
        body,
        "\nManifest address: `https://releases.aphrody.com/manifest.json`. Generated {}.\n",
        cell(data.manifest["generated"].as_str().unwrap_or_default())
    );
    let mut products: Vec<String> = Vec::new();
    for target in data.targets {
        if !products.contains(&target.product) {
            products.push(target.product.clone());
        }
    }
    for product in products {
        let rows: Vec<&ReleaseTarget> = data
            .targets
            .iter()
            .filter(|t| t.product == product)
            .collect();
        let _ = write!(
            body,
            "\n## {} files\n\n{}",
            esc(&product),
            targets_table(&rows)
        );
    }
    body.push_str("\nThe file tables come from `release.toml`. See [install fw](/start/install/) for the install commands.\n");
    Page::new(
        "releases",
        "Releases and platforms",
        "Latest fw, Aphrody and ay versions from the live release manifest, and the installer files published for each operating system and architecture.",
        "Releases",
        body,
    )
    .sourced("release.toml, releases manifest", &format!("{}{}", data.manifest, data.targets.len()))
}

/// Sidebar for Starlight: top-level groups in a fixed order, nested groups by the part after the first slash of the section.
pub fn sidebar(pages: &[Page]) -> String {
    let mut groups: Vec<Value> = Vec::new();
    for top in SECTION_ORDER {
        let mut direct: Vec<Value> = Vec::new();
        let mut nested: Vec<(String, Vec<Value>)> = Vec::new();
        for page in pages
            .iter()
            .filter(|p| p.section.split('/').next() == Some(*top))
        {
            let mut item = serde_json::Map::new();
            item.insert(
                "slug".into(),
                json!(if page.slug == "index" {
                    "index".to_owned()
                } else {
                    page.slug.clone()
                }),
            );
            if let Some(label) = &page.label {
                item.insert("label".into(), json!(label));
            }
            let item = Value::Object(item);
            match page.section.split_once('/') {
                None => direct.push(item),
                Some((_, sub)) => match nested.iter_mut().find(|(name, _)| name == sub) {
                    Some((_, items)) => items.push(item),
                    None => nested.push((sub.to_owned(), vec![item])),
                },
            }
        }
        let mut items = direct;
        for (name, sub_items) in nested {
            items.push(json!({ "label": name, "collapsed": true, "items": sub_items }));
        }
        if items.is_empty() {
            continue;
        }
        groups.push(json!({ "label": top, "collapsed": !matches!(*top, "Get started" | "Guides"), "items": items }));
    }
    serde_json::to_string_pretty(&groups).unwrap_or_default()
}

pub fn crumbs(page: &Page, all: &[Page]) -> Vec<(String, String)> {
    let mut list = vec![("Home".to_owned(), "/".to_owned())];
    if page.slug == "index" {
        return list;
    }
    let first = page.slug.split('/').next().unwrap_or_default();
    let top = match first {
        "start" => Some("Get started"),
        "guides" => Some("Guides"),
        "products" => Some("Products"),
        "cli" => Some("CLI reference"),
        "mcp" => Some("MCP tools"),
        "api" => Some("API reference"),
        "crates" => Some("Crates"),
        "releases" => Some("Releases"),
        _ => None,
    };
    if let Some(top) = top
        && page.slug != first
    {
        let landing = all
            .iter()
            .find(|p| p.slug == first)
            .map(|_| format!("/{first}/"))
            .unwrap_or_else(|| {
                all.iter()
                    .find(|p| p.slug.starts_with(&format!("{first}/")))
                    .map(|p| p.url_path())
                    .unwrap_or_default()
            });
        list.push((top.to_owned(), landing));
    }
    list.push((page.title.clone(), page.url_path()));
    list
}

pub fn json_ld(page: &Page, site: &Site, all: &[Page]) -> Value {
    let url = format!("{}{}", site.url, page.url_path());
    let breadcrumbs: Vec<Value> = crumbs(page, all)
        .iter()
        .enumerate()
        .map(|(index, (name, path))| {
            json!({ "@type": "ListItem", "position": index + 1, "name": name, "item": format!("{}{}", site.url, path) })
        })
        .collect();
    let publisher = json!({ "@type": "Organization", "name": site.organization, "url": site.repository.replace("/fw", "") });
    let main = match &page.schema {
        Schema::Article => json!({
            "@type": "TechArticle",
            "@id": format!("{url}#article"),
            "headline": page.title,
            "description": page.description,
            "url": url,
            "inLanguage": "en",
            "isPartOf": { "@type": "WebSite", "name": site.name, "url": site.url },
            "publisher": publisher,
        }),
        Schema::Software {
            name,
            version,
            systems,
            download,
            repository,
        } => json!({
            "@type": "SoftwareApplication",
            "@id": format!("{url}#software"),
            "name": name,
            "description": page.description,
            "url": url,
            "applicationCategory": "DeveloperApplication",
            "operatingSystem": systems.join(", "),
            "softwareVersion": version,
            "downloadUrl": download,
            "codeRepository": format!("https://github.com/{repository}"),
            "publisher": publisher,
        }),
    };
    json!({
        "@context": "https://schema.org",
        "@graph": [
            main,
            { "@type": "BreadcrumbList", "@id": format!("{url}#breadcrumb"), "itemListElement": breadcrumbs }
        ]
    })
}

pub fn robots(site: &Site) -> String {
    format!(
        "# generated by fw site\nUser-agent: *\nAllow: /\n\nSitemap: {0}/sitemap-index.xml\nSitemap: {0}/sitemap.xml\n",
        site.url
    )
}

pub fn sitemap(site: &Site, pages: &[Page]) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- generated by fw site -->\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );
    let mut sorted: Vec<&Page> = pages.iter().collect();
    sorted.sort_by(|a, b| a.slug.cmp(&b.slug));
    for page in sorted {
        let _ = writeln!(
            out,
            "  <url><loc>{}{}</loc></url>",
            site.url,
            page.url_path()
        );
    }
    out.push_str("</urlset>\n");
    out
}

pub fn llms_txt(site: &Site, pages: &[Page]) -> String {
    let mut out = format!(
        "# {}\n\n> {}. Every page below has a plain markdown version at its address plus `index.md`. {}/llms-full.txt holds all pages in one file.\n\n",
        site.name, site.tagline, site.url
    );
    for top in SECTION_ORDER {
        let members: Vec<&Page> = pages
            .iter()
            .filter(|p| p.section.split('/').next() == Some(*top))
            .collect();
        if members.is_empty() {
            continue;
        }
        let _ = writeln!(
            out,
            "## {}\n",
            if *top == "Crates" { "Optional" } else { top }
        );
        for page in members {
            let _ = writeln!(
                out,
                "- [{}]({}{}index.md): {}",
                page.title,
                site.url,
                if page.slug == "index" {
                    "/".to_owned()
                } else {
                    page.url_path()
                },
                page.description
            );
        }
        out.push('\n');
    }
    out
}

pub fn llms_full(site: &Site, pages: &[Page]) -> String {
    let mut out = format!("# {}\n\n> {}\n\n", site.name, site.tagline);
    for top in SECTION_ORDER {
        for page in pages
            .iter()
            .filter(|p| p.section.split('/').next() == Some(*top))
        {
            let _ = write!(
                out,
                "---\n\n# {}\n\nSource: {}{}\n\n{}\n\n",
                page.title,
                site.url,
                page.url_path(),
                demote_headings(&page_markdown(page))
            );
        }
    }
    out
}

fn page_markdown(page: &Page) -> String {
    crate::unescape_mdx(page.body_md.as_deref().unwrap_or(&page.body))
        .trim()
        .to_owned()
}

/// In the combined file the page title is `#`, so page headings move down one level (outside code fences).
fn demote_headings(text: &str) -> String {
    let mut in_fence = false;
    let mut out = String::new();
    for line in text.lines() {
        if line.starts_with("```") {
            in_fence = !in_fence;
        }
        if !in_fence && line.starts_with('#') {
            out.push('#');
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn css_value(css: &str, name: &str) -> String {
    css.lines()
        .find_map(|line| {
            let line = line.trim();
            line.strip_prefix(name)
                .and_then(|rest| rest.trim_start().strip_prefix(':'))
                .map(|value| value.trim().trim_end_matches(';').to_owned())
        })
        .unwrap_or_else(|| "#000000".to_owned())
}

/// The social card: brand colors from the fw-m3 roles (dark scheme), one title and the host.
pub fn og_card(site: &Site, css: &str, _pages: usize) -> String {
    let dark_start = css.find("data-theme=\"dark\"").unwrap_or(0);
    let dark = &css[dark_start..];
    let surface = css_value(dark, "--md-sys-color-surface");
    let on_surface = css_value(dark, "--md-sys-color-on-surface");
    let primary = css_value(dark, "--md-sys-color-primary");
    let container = css_value(dark, "--md-sys-color-primary-container");
    let host = site.url.trim_start_matches("https://");
    let hash = short_hash(css);
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1200\" height=\"630\" viewBox=\"0 0 1200 630\" role=\"img\" aria-label=\"{name}\">\n<!-- generated by fw site from fw-m3 css@{hash} -->\n<rect width=\"1200\" height=\"630\" fill=\"{surface}\"/>\n<rect x=\"72\" y=\"72\" width=\"96\" height=\"96\" rx=\"28\" fill=\"{container}\"/>\n<rect x=\"96\" y=\"96\" width=\"48\" height=\"48\" rx=\"14\" fill=\"{primary}\"/>\n<text x=\"72\" y=\"360\" font-family=\"Google Sans Flex, sans-serif\" font-size=\"96\" font-weight=\"600\" fill=\"{on_surface}\">{name}</text>\n<text x=\"72\" y=\"440\" font-family=\"Google Sans Flex, sans-serif\" font-size=\"36\" fill=\"{on_surface}\">fw, Aphrody and ay. Generated from the code.</text>\n<text x=\"72\" y=\"560\" font-family=\"Google Sans Code, monospace\" font-size=\"30\" fill=\"{primary}\">{host}</text>\n</svg>\n",
        name = site.name
    )
}

fn with_index(
    slug: &str,
    title: &str,
    description: &str,
    section: &str,
    intro: &str,
    mut pages: Vec<Page>,
) -> Vec<Page> {
    let mut body = format!("{intro}\n\n| Page | What it covers |\n|---|---|\n");
    for page in &pages {
        let _ = writeln!(
            body,
            "| [{}]({}) | {} |",
            cell(&page.title),
            page.url_path(),
            cell(&page.description)
        );
    }
    let material: String = pages
        .iter()
        .map(|p| format!("{}{}", p.slug, p.input_hash))
        .collect();
    let index = Page::new(slug, title, description, section, body)
        .label("Overview")
        .sourced("the pages of this section", &material);
    pages.insert(0, index);
    pages
}

fn guide_pages(data: &Data) -> Vec<Page> {
    with_index(
        "guides",
        "Guides",
        "Task guides for the Aphrody stack: connect an AI agent to a project, let fw learn from a repository, and see how this documentation is generated.",
        "Guides",
        "Guides explain a task from start to end.",
        guide_pages_inner(data),
    )
}

fn product_pages(data: &Data) -> Vec<Page> {
    with_index(
        "products",
        "Products",
        "The three first layers of the Aphrody stack: fw the toolchain and agent runtime, Aphrody the OS and desktop, and ay the SDK with cloud and edge services.",
        "Products",
        "The stack has an order. A product may depend on the ones before it, never on the ones after it.",
        product_pages_inner(data),
    )
}

/// The mark of the social card as an icon.
pub fn favicon(css: &str) -> String {
    let dark = &css[css.find("data-theme=\"dark\"").unwrap_or(0)..];
    let container = css_value(dark, "--md-sys-color-primary-container");
    let primary = css_value(dark, "--md-sys-color-primary");
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 96 96\">\n<!-- generated by fw site from fw-m3 css@{} -->\n<rect width=\"96\" height=\"96\" rx=\"28\" fill=\"{container}\"/>\n<rect x=\"24\" y=\"24\" width=\"48\" height=\"48\" rx=\"14\" fill=\"{primary}\"/>\n</svg>\n",
        short_hash(css)
    )
}
