import { describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, statSync, utimesSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  type Host,
  aySpecTarget,
  carapaceSpec,
  completionTargets,
  emitPs1,
  emitSh,
  expandDst,
  gitCompletionCandidates,
  gitInclude,
  linkTargets,
  loadEnv,
  loadLinks,
  parseHelp,
  put,
  staleCaches,
  srcRoot,
  stub,
  vars,
} from "./shells.ts";

const root = join(import.meta.dir, "..");

function host(over: Partial<Host> = {}): Host {
  return {
    plat: "windows",
    home: "H:\\u",
    appdata: "H:\\u\\ad",
    localappdata: "H:\\u\\lad",
    programfiles: "P:\\pf",
    root,
    which: () => undefined,
    run: () => null,
    ...over,
  };
}

const win = host();
const lin = host({ plat: "linux", home: "/home/u", appdata: "", localappdata: "", programfiles: "" });

describe("env generation", () => {
  const e = loadEnv(win);

  test("SRC_ROOT is the parent of the checkout, never read from env.toml", () => {
    expect(e.var.SRC_ROOT).toBeUndefined();
    const v = vars(e, "windows", win)[0];
    expect(v.name).toBe("SRC_ROOT");
    expect(v.value).toEqual([{ lit: srcRoot(win).replaceAll("/", "\\") }]);
    expect(vars(e, "linux", lin)[0].value).toEqual([{ lit: srcRoot(lin) }]);
  });

  test("env.sh exports the workspace root and the front PATH entries", () => {
    const sh = emitSh(e, win);
    expect(sh).toContain(`export SRC_ROOT='${srcRoot(win).replaceAll("/", "\\")}'`);
    expect(sh).toContain("_ay_pp 'P:\\pf\\coreutils\\bin' m");
    expect(sh).toContain(`export AY_SHELL_CACHE="$_ay_h"'\\.cache\\shell'`);
  });

  test("env.ps1 uses $env:* only and resolves {programfiles} at generation", () => {
    const ps = emitPs1(e, win);
    expect(ps).toContain("$env:SRC_ROOT = '");
    expect(ps).toContain("('P:\\pf\\coreutils\\bin')");
    expect(ps).not.toMatch(/\b(Get|Set|Select|Test)-\w+/);
  });

  test("keep vars only default when the OS environment has not set them", () => {
    const sh = emitSh(e, win);
    expect(sh).toContain(`[ -n "\${CARGO_HOME-}" ] || export CARGO_HOME="$_ay_h"'\\.cargo'`);
    expect(emitPs1(e, win)).toContain("if (-not $env:CARGO_HOME) { $env:CARGO_HOME = $_ayH + '\\.cargo' }");
    expect(sh).toContain("export PAGER='less'");
  });

  test("a Linux host emits only the Linux branch", () => {
    const sh = emitSh(e, lin);
    expect(sh).toContain("export XDG_CONFIG_HOME=");
    expect(sh).not.toContain("coreutils");
    expect(emitPs1(e, lin)).toContain("$_ayD = @()");
  });

  test("generated sh parses under bash", () => {
    const bash = Bun.which("bash");
    if (!bash) return;
    const dir = mkdtempSync(join(tmpdir(), "aysh-sh-"));
    const f = join(dir, "env.sh");
    writeFileSync(f, emitSh(e, win));
    const r = Bun.spawnSync([bash, "-n", f], { stderr: "pipe" });
    expect(r.stderr.toString()).toBe("");
    expect(r.exitCode).toBe(0);
  });
});

describe("links", () => {
  const links = loadLinks(win);

  test("every link source exists and uses a known mode", () => {
    for (const l of links) {
      expect(statSync(join(root, "config", l.src)).isFile()).toBe(true);
      expect(["source", "copy"]).toContain(l.mode);
    }
  });

  test("expandDst expands ~, {appdata}, {localappdata} and picks the platform", () => {
    expect(expandDst("~/.x", win)).toBe(join("H:\\u", ".x"));
    expect(expandDst({ windows: "{appdata}/a", linux: "/l" }, win)).toBe(join("H:\\u\\ad", "a"));
    expect(expandDst({ windows: "{localappdata}/a" }, win)).toBe(join("H:\\u\\lad", "a"));
    expect(expandDst({ windows: "w" }, lin)).toBeUndefined();
  });

  test("a stub sources the checkout copy and quotes the path", () => {
    expect(stub("C:\\a b\\bashrc")).toContain(". 'C:/a b/bashrc'");
  });

  test("a copy link is dropped when its tool is absent, the bash stubs never are", () => {
    const none = linkTargets(win, links);
    expect(none.targets.map((t) => t.file)).toContain(join("H:\\u", ".bashrc"));
    expect(none.targets.some((t) => t.file.endsWith("starship.toml"))).toBe(false);
    expect(none.info.some((l) => l.includes("starship"))).toBe(true);
    const some = linkTargets(host({ which: (n) => (n === "starship" ? "x" : undefined) }), links);
    expect(some.targets.some((t) => t.file.endsWith("starship.toml"))).toBe(true);
  });

  test("{src_root} in a copied file becomes the workspace root", () => {
    const r = linkTargets(host({ which: (n) => (n === "yazi" ? "x" : undefined) }), links);
    const keymap = r.targets.find((t) => t.file.endsWith("keymap.toml"))!;
    expect(keymap.content).not.toContain("{src_root}");
    expect(keymap.content).toContain(`cd ${srcRoot(win).replaceAll("\\", "/")}`);
  });

  test("the starship config carries the git timeout fixes", () => {
    const s = Bun.TOML.parse(readFileSync(join(root, "config", "starship", "starship.toml"), "utf8")) as {
      command_timeout: number;
      git_status: { ignore_submodules: boolean };
    };
    expect(s.command_timeout).toBe(1000);
    expect(s.git_status.ignore_submodules).toBe(true);
  });
});

describe("completions", () => {
  test("git completion is searched from the installed git, nearest first", () => {
    const c = gitCompletionCandidates("X:/g/mingw64/libexec/git-core\n", win);
    expect(c[0]).toBe(join("X:\\g\\mingw64", "share", "git", "completion", "git-completion.bash"));
    expect(c).toContain(join("P:\\pf", "Git", "ucrt64", "share", "git", "completion", "git-completion.bash"));
    expect(new Set(c).size).toBe(c.length);
  });

  test("git, bun and ay completions come from what is installed", () => {
    const dir = mkdtempSync(join(tmpdir(), "aysh-c-"));
    mkdirSync(join(dir, "share", "git", "completion"), { recursive: true });
    writeFileSync(join(dir, "share", "git", "completion", "git-completion.bash"), "# git\n");
    const h = host({
      home: dir,
      which: (n) => ({ git: "g", bun: "b", carapace: "c" })[n],
      run: (a) => (a[0] === "git" ? join(dir, "libexec", "git-core") : a[0] === "carapace" ? "# bun\n" : null),
    });
    const r = completionTargets(loadEnv(h), h);
    const by = (n: string) => r.targets.find((t) => t.file.endsWith(n))!;
    expect(by("git.bash").content).toBe("# git\n");
    expect(by("bun.bash").content).toBe("# bun\n");
    expect(by("ay-completion.bash").content).toBeNull();
  });

  test("the ay completion is installed only with an ay on PATH", () => {
    const h = host({ which: (n) => (n === "ay" ? "ay" : undefined) });
    const t = completionTargets(loadEnv(h), h).targets.find((x) => x.file.endsWith("ay-completion.bash"))!;
    expect(t.content).toContain("_ay_complete");
  });

  test("no ay spec without ay and carapace", () => {
    expect(aySpecTarget(host()).targets).toEqual([]);
  });

  test("parseHelp reads clap and argh pages", () => {
    const text = [
      "Usage: ay [OPTIONS] <root> [COMMAND]",
      "",
      "Commands:",
      "  up       Update tools",
      "  help     Print help",
      "",
      "Options:",
      "  -v, --verbose       Verbose",
      "      --root <ROOT>   Workspace",
      "  -h, --help          Print help",
    ].join("\n");
    const p = parseHelp(text);
    expect(p.cmds).toEqual([{ name: "up", desc: "Update tools" }]);
    expect(p.opts.map((o) => [o.long, o.value])).toEqual([["--verbose", false], ["--root", true], ["--help", false]]);
    const spec = carapaceSpec({ name: "ay", desc: "d", opts: p.opts, cmds: [] }) as { flags: Record<string, string> };
    expect(Object.keys(spec.flags)).toContain("--root=");
  });
});

describe("applying", () => {
  test("put writes, then reports ok, then removes", () => {
    const dir = mkdtempSync(join(tmpdir(), "aysh-p-"));
    const file = join(dir, "a", "b.txt");
    const t = { file, content: "x\n", label: "t" };
    expect(put(t, true)).toStartWith("DIFF");
    expect(put(t, false)).toStartWith("set");
    expect(put(t, false)).toStartWith("ok");
    expect(readFileSync(file, "utf8")).toBe("x\n");
    const gone = { file, content: null, label: "t" };
    expect(put(gone, true)).toStartWith("DIFF");
    expect(put(gone, false)).toStartWith("rm");
    expect(put(gone, false)).toStartWith("ok");
  });

  test("caches older than the bash rc are removed, newer ones and other files are kept", () => {
    const dir = mkdtempSync(join(tmpdir(), "aysh-s-"));
    const rc = join(dir, "bashrc");
    writeFileSync(rc, "#\n");
    utimesSync(rc, 2000, 2000);
    for (const [n, t] of [["bash-old.bash", 1000], ["bash-new.bash", 3000], ["env.sh", 1000]] as const) {
      writeFileSync(join(dir, n), "#\n");
      utimesSync(join(dir, n), t, t);
    }
    expect(staleCaches(dir, rc, true)).toEqual(["DIFF stale cache bash-old.bash"]);
    expect(staleCaches(dir, rc, false)).toEqual(["rm   stale cache bash-old.bash"]);
    expect(readdirSync(dir).sort()).toEqual(["bash-new.bash", "bashrc", "env.sh"]);
  });

  test("gitInclude adds the delta include once", () => {
    const dir = mkdtempSync(join(tmpdir(), "aysh-g-"));
    const h = host({ home: dir });
    expect(gitInclude(h, "D:\\c\\delta.gitconfig", true)).toStartWith("DIFF");
    expect(gitInclude(h, "D:\\c\\delta.gitconfig", false)).toStartWith("set");
    expect(gitInclude(h, "D:\\c\\delta.gitconfig", false)).toStartWith("ok");
    expect(readFileSync(join(dir, ".gitconfig"), "utf8")).toContain("path = D:/c/delta.gitconfig");
    expect(readdirSync(dir)).toEqual([".gitconfig"]);
  });
});

describe("no host data in tracked files", () => {
  // Built from pieces so this file does not match itself.
  const banned = [
    new RegExp(["C:[\\\\/]", "aphro", "dy"].join("")),
    new RegExp(["packages", "[\\\\/]", "machine"].join("")),
    new RegExp(["third", "_party"].join("")),
    new RegExp(["C:[\\\\/]", "Users"].join(""), "i"),
  ];
  const files = (dir: string): string[] =>
    readdirSync(dir, { withFileTypes: true }).flatMap((d) => (d.isDirectory() ? files(join(dir, d.name)) : [join(dir, d.name)]));

  test("config/ and the installer carry no legacy path, account or host path", () => {
    for (const f of [...files(join(root, "config")), join(root, "tools", "shells.ts")]) {
      const text = readFileSync(f, "utf8");
      for (const re of banned) expect({ f, hit: re.test(text) }).toEqual({ f, hit: false });
    }
  });
});
