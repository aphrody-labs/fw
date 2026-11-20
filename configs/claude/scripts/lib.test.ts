import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  ensureDirLink,
  renderTemplate,
  runCmd,
  templateVars,
  toSlash,
  writeIfChanged,
} from "./lib.ts";

let tmp: string;
beforeAll(() => {
  tmp = mkdtempSync(join(tmpdir(), "claude-lib-test-"));
});
afterAll(() => {
  rmSync(tmp, { recursive: true, force: true });
});

describe("toSlash", () => {
  test("converts backslashes and leaves slashes alone", () => {
    expect(toSlash("C:\\a\\b\\c.txt")).toBe("C:/a/b/c.txt");
    expect(toSlash("/a/b")).toBe("/a/b");
    expect(toSlash("")).toBe("");
  });
});

describe("writeIfChanged", () => {
  test("creates parents, then reports change only on new content", () => {
    const p = join(tmp, "deep", "nested", "f.txt");
    expect(writeIfChanged(p, "one")).toBe(true);
    expect(readFileSync(p, "utf8")).toBe("one");
    expect(writeIfChanged(p, "one")).toBe(false);
    expect(writeIfChanged(p, "two")).toBe(true);
    expect(readFileSync(p, "utf8")).toBe("two");
  });
});

describe("ensureDirLink", () => {
  test("links a directory and re-points it without touching targets", () => {
    const a = join(tmp, "targetA");
    const b = join(tmp, "targetB");
    mkdirSync(a, { recursive: true });
    mkdirSync(b, { recursive: true });
    writeFileSync(join(a, "a.txt"), "A");
    writeFileSync(join(b, "b.txt"), "B");

    const link = join(tmp, "links", "current");
    ensureDirLink(a, link);
    expect(readFileSync(join(link, "a.txt"), "utf8")).toBe("A");
    expect(realpathSync(link)).toBe(realpathSync(a));

    ensureDirLink(b, link);
    expect(existsSync(join(link, "a.txt"))).toBe(false);
    expect(readFileSync(join(link, "b.txt"), "utf8")).toBe("B");
    expect(readFileSync(join(a, "a.txt"), "utf8")).toBe("A");
  });

  test("replaces a plain file and a plain directory at the link path", () => {
    const target = join(tmp, "targetC");
    mkdirSync(target, { recursive: true });
    writeFileSync(join(target, "c.txt"), "C");

    const fileLink = join(tmp, "plain-file");
    writeFileSync(fileLink, "x");
    ensureDirLink(target, fileLink);
    expect(readFileSync(join(fileLink, "c.txt"), "utf8")).toBe("C");

    const dirLink = join(tmp, "plain-dir");
    mkdirSync(dirLink);
    writeFileSync(join(dirLink, "old.txt"), "old");
    ensureDirLink(target, dirLink);
    expect(existsSync(join(dirLink, "old.txt"))).toBe(false);
    expect(readFileSync(join(dirLink, "c.txt"), "utf8")).toBe("C");
  });
});

describe("runCmd", () => {
  test("captures trimmed stdout and success", () => {
    const r = runCmd([process.execPath, "-e", "console.log('  hi  ')"]);
    expect(r).toEqual({ ok: true, code: 0, out: "hi", err: "" });
  });

  test("reports failure code and stderr", () => {
    const r = runCmd([
      process.execPath,
      "-e",
      "console.error('boom'); process.exit(3)",
    ]);
    expect(r.ok).toBe(false);
    expect(r.code).toBe(3);
    expect(r.err).toBe("boom");
  });

  test("honours cwd", () => {
    const r = runCmd(
      [process.execPath, "-e", "console.log(process.cwd())"],
      tmp,
    );
    expect(toSlash(realpathSync(r.out)).toLowerCase()).toBe(
      toSlash(realpathSync(tmp)).toLowerCase(),
    );
  });
});

describe("renderTemplate", () => {
  const vars = { A: "C:\\x\\y", B: "plain" };

  test("substitutes every occurrence of ${KEY}", () => {
    expect(renderTemplate("${B}-${B}/${A}", vars)).toBe("plain-plain/C:\\x\\y");
  });

  test("jsonEscape doubles backslashes in values only", () => {
    const out = renderTemplate('{"p":"${A}","q":"${B}"}', vars, true);
    expect(out).toBe('{"p":"C:\\\\x\\\\y","q":"plain"}');
    expect(JSON.parse(out)).toEqual({ p: "C:\\x\\y", q: "plain" });
  });

  test("leaves unknown placeholders untouched", () => {
    expect(renderTemplate("${NOPE} $B ${B", vars)).toBe("${NOPE} $B ${B");
  });

  test("defaults to templateVars()", () => {
    const v = templateVars();
    expect(renderTemplate("${REPO_ROOT}|${HOME}")).toBe(
      `${v.REPO_ROOT}|${v.HOME}`,
    );
  });
});

describe("templateVars", () => {
  test("exposes the documented keys with slash-normalised paths", () => {
    const v = templateVars();
    expect(Object.keys(v).sort()).toEqual(
      [
        "HOME",
        "HOME_WIN",
        "REPO_ROOT",
        "RELAY_URL",
        "SRC_ROOT",
        "SRC_ROOT_WIN",
        "WORKSPACE_ROOT",
        "WORKSPACE_ROOT_WIN",
      ].sort(),
    );
    expect(v.HOME).not.toContain("\\");
    expect(v.REPO_ROOT).not.toContain("\\");
    expect(v.HOME_WIN).toBe(v.HOME!.replaceAll("/", "\\"));
    expect(v.RELAY_URL).toBe(process.env.RELAY_URL ?? "http://127.0.0.1:8790");
  });
});
