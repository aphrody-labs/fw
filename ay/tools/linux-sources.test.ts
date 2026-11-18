import { expect, test } from "bun:test";
import { pickDir, select, toToml } from "./linux-sources.ts";

const repo = (name: string, language: string | null, stars: number, extra = {}) =>
  ({ name, full_name: `o/${name}`, language, stargazers_count: stars, archived: false, fork: false, pushed_at: "2026-11-17T00:00:00Z", ...extra });

test("all keeps every active non-fork repo, ranked by stars", () => {
  const got = select({ org: "o", take: "all", dir: "d" }, [repo("a", "C", 1), repo("b", null, 5), repo("c", "Rust", 9, { archived: true }), repo("f", "Rust", 9, { fork: true })]);
  expect(got.map(e => e.dir)).toEqual(["d/b", "d/a"]);
});

test("rust keeps Rust repos above the star floor", () => {
  const got = select({ org: "o", take: "rust", min_stars: 10, dir: "m" }, [repo("x", "Rust", 50), repo("y", "Rust", 5), repo("z", "C++", 99)]);
  expect(got.map(e => e.slug)).toEqual(["o/x"]);
});

test("picks land under their group and the TOML parses back", () => {
  expect(pickDir("kernel", "redox-os/kernel")).toBe("kernel/redox-os-kernel");
  expect(pickDir("gnu-rs", "sharkdp/fd")).toBe("gnu-rs/fd");
  expect(pickDir("gnu-rs", "x/fd", new Set(["gnu-rs/fd"]))).toBe("gnu-rs/x-fd");
  const t = Bun.TOML.parse(toToml(select({ org: "o", take: "all", dir: "d" }, [repo("a", "Rust", 3)]))) as { source: { slug: string; stars: number }[] };
  expect(t.source[0]).toMatchObject({ slug: "o/a", stars: 3 });
});
