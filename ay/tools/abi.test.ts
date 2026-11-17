import { expect, test } from "bun:test";
import { check, loadSpec, outputs, validate, type Spec } from "./abi.ts";
import { cFacts, parse, pythonFacts, rustFacts, switchTable, syntaxErrors } from "./ts.ts";

const specText = await Bun.file(new URL("../abi/ay.abi.toml", import.meta.url)).text();
const spec = await loadSpec();

test("the spec is valid and the generated files on disk are the ones it generates", async () => {
  expect(validate(spec)).toEqual([]);
  for (const f of outputs(spec, specText)) expect(await Bun.file(new URL(`../${f.path}`, import.meta.url)).text(), f.path).toBe(f.text);
});

test("every generated file parses cleanly and agrees with the spec and with the corelib sources", async () => {
  expect(await check(spec, outputs(spec, specText))).toEqual([]);
});

test("drift is reported: a changed constant, a changed signature, an unknown export and a wrong syscall arity", async () => {
  const files = outputs(spec, specText).map((f) => ({ ...f }));
  const header = files.find((f) => f.path.endsWith(".h"))!;
  header.text = header.text.replace(/(AY_SYS_YIELD\s+)1/, "$19").replace("uint32_t ay_sum(const uint32_t *words, uintptr_t len);", "uint32_t ay_sum(uint32_t *words, uintptr_t len);");
  const problems = await check(spec, files);
  expect(problems.some((p) => p.includes("AY_SYS_YIELD is 9, the spec says 1"))).toBe(true);
  expect(problems.some((p) => p.includes("ay_sum is u32(*mut u32, usize), the spec says u32(*const u32, usize)"))).toBe(true);

  const bad: Spec = structuredClone(spec);
  bad.group.find((g) => g.id === "syscall")!.const.find((c) => c.name === "AY_SYS_LOG")!.nargs = 2;
  bad.function.pop();
  const drift = await check(bad, outputs(bad, specText));
  expect(drift.some((p) => p.includes("ay_arg_count(AY_SYS_LOG) returns 3, the spec says 2"))).toBe(true);
  expect(drift.some((p) => p.includes("ay_try_acquire is exported (AY_API) but not in the spec"))).toBe(true);
});

test("validation refuses duplicate values, out-of-range constants and unknown types", () => {
  const bad: Spec = structuredClone(spec);
  bad.group.find((g) => g.id === "right")!.const[1]!.value = 1;
  bad.group.find((g) => g.id === "error")!.const[0]!.value = 2 ** 31;
  bad.function[1]!.returns = "float";
  const problems = validate(bad);
  expect(problems.some((p) => p.includes("share the value 1"))).toBe(true);
  expect(problems.some((p) => p.includes("does not fit i32"))).toBe(true);
  expect(problems.some((p) => p.includes("unknown type float"))).toBe(true);
});

test("the Bun module, the Python module and the JSON form carry the same constants and symbols", async () => {
  const mod = await import("../abi/gen/ay_abi.ts");
  for (const g of spec.group) for (const c of g.const) expect((mod as Record<string, unknown>)[c.name], c.name).toBe(c.value);
  expect(Object.keys(mod.symbols)).toEqual(spec.function.filter((f) => !f.import).map((f) => f.name));
  expect(mod.symbols.ay_checksum).toEqual({ args: ["ptr", "usize"], returns: "u32" });
  expect(mod.SYSCALL_NARGS[mod.AY_SYS_LOG]).toBe(3);
  const json = JSON.parse(await Bun.file(new URL("../abi/gen/ay_abi.json", import.meta.url)).text());
  expect(json.spec_sha256).toBe(new Bun.CryptoHasher("sha256").update(specText).digest("hex"));
  expect(json.constants).toHaveLength(22);
});

test("tree-sitter extracts macros, signatures, Rust externs, Python tables and switch tables", async () => {
  const c = cFacts(await parse("c", "#define A (-3) /* x */\n#define B 4u\nconst char *f(const uint32_t *p, size_t n);\nstatic int g(void) { return 0; }"));
  expect([...c.consts]).toEqual([["A", -3], ["B", 4]]);
  expect(c.fns.get("f")).toEqual({ ret: "*const char", params: ["*const u32", "usize"] });
  expect(c.fns.get("g")).toEqual({ ret: "int", params: [] });
  const rs = await parse("rust", 'pub const X: i32 = -1;\nextern "C" { pub fn h(a: u32, p: *mut u32) -> u64; }');
  expect(rustFacts(rs).consts.get("X")).toBe(-1);
  expect(rustFacts(rs).fns.get("h")).toEqual({ ret: "u64", params: ["u32", "*mut u32"] });
  const py = pythonFacts(await parse("python", 'N = 7\nSIGNATURES = {"f": (a, [b, c]), "g": (a, [])}\n'));
  expect(py.consts.get("N")).toBe(7);
  expect([...py.arity]).toEqual([["f", 2], ["g", 0]]);
  const sw = switchTable(await parse("c", "unsigned t(unsigned n) { switch (n) { case P: case Q: return 0; case R: return 2; default: return 9; } }"), "t", new Map([["P", 1], ["Q", 2], ["R", 3]]));
  expect([...sw!]).toEqual([[1, 0], [2, 0], [3, 2], [-1, 9]]);
  expect(syntaxErrors(await parse("c", "int f( {"))).not.toEqual([]);
});

test("another spec: u8 pointers, custom output paths, and the generated files agree with it", async () => {
  const other: Spec = { version: 1, name: "vpn", title: "t", intro: ["i"], output: { header: "include/vpn.h", dir: "gen", doc: "docs/vpn.md" }, group: [{ id: "state", comment: "States.", type: "u32", const: [{ name: "VPN_STATE_UP", value: 1 }] }], function: [{ name: "vpn_open", returns: "i32", params: [["config", "*const u8"], ["len", "usize"], ["out", "*mut u64"]] }] };
  const files = outputs(other, "x");
  expect(files.map((f) => f.path).sort()).toEqual(["docs/vpn.md", "gen/vpn_abi.json", "gen/vpn_abi.py", "gen/vpn_abi.rs", "gen/vpn_abi.ts", "include/vpn.h"]);
  expect(files.find((f) => f.path === "include/vpn.h")!.text.replace(/[ ]+/g, " ")).toContain("int32_t vpn_open(const uint8_t *config, uintptr_t len, uint64_t *out);");
  expect(validate(other)).toEqual([]);
  expect(await check(other, files)).toEqual([]);
});
