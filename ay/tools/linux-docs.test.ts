import { expect, test } from "bun:test";
import { areaOf, describe, frontmatter, mdxSafe } from "./linux-docs.ts";

test("MDX-unsafe characters are escaped outside code only", () => {
  const out = mdxSafe("Use {x} and <T> here, `{keep}` too\n\n```c\nif (a < b) { f(); }\n```\n\nSee <https://example.test/a>");
  expect(out).toContain("Use \\{x\\} and &lt;T> here, `{keep}` too");
  expect(out).toContain("if (a < b) { f(); }");
  expect(out).toContain("<https://example.test/a>");
});

test("title is the first heading, description the first prose paragraph, and the heading leaves the body", () => {
  const d = describe("# Rust coding guidelines {#x}\n\nThis document describes **how to** write [Rust](https://x.test) code.\n\n- a\n- b\n", "fallback");
  expect(d.title).toBe("Rust coding guidelines");
  expect(d.description).toBe("This document describes how to write Rust code.");
  expect(d.body).not.toContain("# Rust");
  expect(describe("no heading here\n\nsecond", "fb").title).toBe("fb");
});

test("frontmatter is valid YAML-like with quoted values and areas follow the first directory", () => {
  const f = frontmatter({ title: 'A "quoted"\ntitle', description: "d", source: "Documentation/rust/index.rst", revision: "v7.4-rc3" });
  expect(f).toBe('---\ntitle: "A \\"quoted\\" title"\ndescription: "d"\nsource: "Documentation/rust/index.rst"\nrevision: "v7.4-rc3"\n---\n\n');
  expect(areaOf("/k/Documentation", "/k/Documentation/rust/index.rst")).toBe("rust");
  expect(areaOf("/k/Documentation", "/k/Documentation/index.rst")).toBe("top-level");
});
