import { describe, expect, test } from "bun:test";
import { extractAsm, extractCFn, extractWat, rewrite } from "./listings.ts";

const ASM = `\t.text
\t.globl\tay_rights_allow
\t.p2align\t4
ay_rights_allow:                        # @ay_rights_allow
# %bb.0:
\t.cfi_startproc
\tand\tedi, esi
\tcmp\tedi, esi
\tsete\tal
\tmovzx\teax, al
\tret
.Lfunc_end0:
\t.size\tay_rights_allow, .Lfunc_end0-ay_rights_allow
\t.cfi_endproc
other:
\tret
`;

describe("listing extraction", () => {
  test("asm: keeps instructions and drops assembler bookkeeping", () => {
    expect(extractAsm(ASM, "ay_rights_allow")).toBe("ay_rights_allow:\n\tand\tedi, esi\n\tcmp\tedi, esi\n\tsete\tal\n\tmovzx\teax, al\n\tret");
  });

  test("asm: a missing function is an error", () => {
    expect(() => extractAsm(ASM, "nope")).toThrow("not found");
  });

  test("asm: jump tables referenced by the function follow it", () => {
    const text = "f:\n\tjmp\t[.LJTI0_0+rax*4]\n.Lfunc_end0:\n.LJTI0_0:\n\t.long\t.LBB0_1-.LJTI0_0\n\t.long\t.LBB0_2-.LJTI0_0\n";
    expect(extractAsm(text, "f")).toBe("f:\n\tjmp\t[.LJTI0_0+rax*4]\n.LJTI0_0:\n\t.long\t.LBB0_1-.LJTI0_0\n\t.long\t.LBB0_2-.LJTI0_0");
  });

  test("wat: names a function through its export when the module has no name section", () => {
    const wat = `(module\n  (func (;0;) (param i32)\n    local.get 0\n    drop\n  )\n  (func (;1;) (type 2) (result i32)\n    i32.const 1\n  )\n  (memory (;0;) 2)\n  (export "ay_x" (func 1))\n)`;
    expect(extractWat(wat, "ay_x")).toBe(`(func $ay_x (export "ay_x") (type 2) (result i32)\n  i32.const 1\n)`);
  });

  test("wat: a named function is taken as is, without volatile indices", () => {
    const wat = `(module\n  (func $ay_x (;1;) (result i32)\n    i32.const 1\n  )\n  (memory (;0;) 2)\n)`;
    expect(extractWat(wat, "ay_x")).toBe(`(func $ay_x (result i32)\n  i32.const 1\n)`);
  });

  test("c: finds a function whose signature starts with an attribute macro", () => {
    const src = `/// Run.\nEXPORT("ay_boot", "canonical") int32_t ay_boot(void) { return 0; }\n\nint32_t other(void) { return ay_boot(); }\n`;
    expect(extractCFn(src, "ay_boot")).toBe(`/// Run.\nEXPORT("ay_boot", "canonical") int32_t ay_boot(void) { return 0; }`);
  });

  test("c: keeps the comment above and matches braces", () => {
    const src = `#include "core.h"\n\n// Doc.\nAY_API uint32_t ay_f(uint32_t a) {\n  if (a > 0) { return 1; }\n  return 2;\n}\n\nstatic void g(void) {}\n`;
    expect(extractCFn(src, "ay_f")).toBe("// Doc.\nAY_API uint32_t ay_f(uint32_t a) {\n  if (a > 0) { return 1; }\n  return 2;\n}");
  });
});

describe("listing blocks", () => {
  test("rewrite replaces only the block bodies and is idempotent", () => {
    const md = "intro\n<!-- listing file:a.txt -->\nold\n<!-- /listing -->\noutro\n";
    const once = rewrite(md, () => ({ lang: "text", body: "new" }));
    expect(once).toBe("intro\n<!-- listing file:a.txt -->\n```text\nnew\n```\n<!-- /listing -->\noutro\n");
    expect(rewrite(once, () => ({ lang: "text", body: "new" }))).toBe(once);
  });
});
