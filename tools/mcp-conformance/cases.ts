// One valid and one invalid payload for every tool `fw mcp` lists. tools.test.ts fails when a listed tool has no
// case here, or a case names a tool that is not listed, so the table cannot drift from the server.

import type { Json } from "./client.ts";

export type Case = {
  valid: Json;
  /** Returns a problem description, or nothing when the valid call's result is right. */
  check: (result: Json) => string | void;
  invalid: Json;
  /** The invalid call must answer isError with text matching this. */
  problem: RegExp;
  /** The tool shells out to a program that may be absent; the absence is reported as a tool error naming it. */
  needsBinary?: string;
};

export type Fixture = { workspace: string; url: string; tag: string };

const text = (result: Json): string => (result.content ?? []).map((c: Json) => c.text ?? "").join("");
const json = (result: Json): Json => JSON.parse(text(result));
const must = (ok: boolean, problem: string): string | void => (ok ? undefined : problem);

const probe = (kernel: string, capability: string): Case => ({
  valid: { action: "probe" },
  check: (r) => must(r.structuredContent?.result?.kernel === kernel && r.structuredContent.result.capabilities.includes(capability), `probe lacks kernel=${kernel} capability=${capability}: ${text(r).slice(0, 120)}`),
  invalid: { action: 5 },
  problem: /action/,
});

const crates = (valid: Json, invalid: Json, problem: RegExp, check: (r: Json) => string | void): Case => ({ valid, check, invalid, problem });
const hasObject = (r: Json) => must(typeof r.structuredContent === "object" && r.structuredContent !== null, `no structuredContent: ${text(r).slice(0, 120)}`);

export function buildCases({ workspace, url, tag }: Fixture): Record<string, Case> {
  return {
    linux_kernel_mcp: probe("linux", "io_uring"),
    windows_kernel_mcp: probe("windows", "nt-win32"),
    wasm_kernel_mcp: probe("webassembly", "webgpu-wgsl"),
    fw_pm_scan: { valid: {}, check: (r) => must(text(r).includes("fw-unified-pm"), "scan report lacks fw-unified-pm"), invalid: { bogus: 1 }, problem: /Unexpected property 'bogus'/ },
    fw_m3_terminal_frame: { valid: {}, check: (r) => must(text(r).includes("kitty_keyboard_flags"), "frame lacks kitty_keyboard_flags"), invalid: { bogus: 1 }, problem: /Unexpected property 'bogus'/ },
    crates_search: crates({ query: "http client" }, {}, /Missing required property 'query'/, hasObject),
    crates_info: crates({ name: "serde" }, {}, /Missing required property 'name'/, hasObject),
    crates_compare: crates({ names: ["serde", "anyhow"] }, { names: "serde" }, /names/, hasObject),
    crates_awesome: crates({}, { limit: "ten" }, /limit/, hasObject),
    crates_stack: crates({ goal: "web API with Postgres" }, {}, /Missing required property 'goal'/, hasObject),
    read_file: { valid: { path: "notes.txt" }, check: (r) => must(json(r).content.includes("needle in a haystack"), "content missing"), invalid: { path: "absent.txt" }, problem: /absent\.txt/ },
    write_file: { valid: { path: `written-${tag}.txt`, content: "fresh" }, check: (r) => must(json(r).bytes === 5, "bytes != 5"), invalid: { path: `written-${tag}.txt` }, problem: /Missing required property 'content'/ },
    edit_file: { valid: { path: `edit-${tag}.txt`, old_string: "before", new_string: "after" }, check: (r) => must(json(r).replaced === 1, "replaced != 1"), invalid: { path: `edit-${tag}.txt`, old_string: "nowhere", new_string: "x" }, problem: /expected 1 match, found 0/ },
    glob_files: { valid: { pattern: "*.txt" }, check: (r) => must(json(r).count >= 1, "no txt file found"), invalid: { mode: "list" }, problem: /Missing required property 'pattern'/ },
    grep_files: { valid: { query: "needle" }, check: (r) => must(json(r).count >= 1, "no match for needle"), invalid: {}, problem: /Missing required property 'query'/ },
    shell: { valid: { action: "run", command: "echo fw-mcp-shell" }, check: (r) => must(json(r).stdout.includes("fw-mcp-shell"), "stdout lacks the echo"), invalid: { action: "run", command: "exit 3" }, problem: /"exit_code":\s*3/ },
    web_fetch: { valid: { url: `${url}/page` }, check: (r) => must(json(r).text.includes("served by the test"), "page text missing"), invalid: { url: "ftp://example.test/x" }, problem: /http:\/\/ or https:\/\//i },
    rg_search: { valid: { pattern: "needle", path: workspace }, check: (r) => must(text(r).includes("needle"), "rg found no match"), invalid: {}, problem: /Missing required property 'pattern'/, needsBinary: "rg" },
    fd_find: { valid: { pattern: "notes", path: workspace }, check: (r) => must(text(r).includes("notes.txt"), "fd found no file"), invalid: {}, problem: /Missing required property 'pattern'/, needsBinary: "fd" },
    ast_grep_scan: { valid: { pattern: "fn main() {}", lang: "rust" }, check: (r) => must(text(r).includes("main.rs"), "ast-grep found no match"), invalid: { pattern: "x" }, problem: /Missing required property 'lang'/, needsBinary: "ast-grep" },
    xh_get: { valid: { url: `${url}/page` }, check: (r) => must(text(r).includes("served by the test"), "body missing"), invalid: {}, problem: /Missing required property 'url'/, needsBinary: "xh" },
  };
}
