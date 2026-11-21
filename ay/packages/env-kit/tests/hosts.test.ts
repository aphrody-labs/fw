import { expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { hostAlias, hostAliases, hostRole } from "../src/hosts";

const root = (base: string, local?: string) => {
  const dir = mkdtempSync(join(tmpdir(), "ay-hosts-"));
  writeFileSync(join(dir, "ay.toml"), base);
  if (local) writeFileSync(join(dir, "ay.local.toml"), local);
  return { AY_ROOT: dir };
};

test("ay.local.toml overrides ay.toml, placeholders and gaps fall back to the role", () => {
  const env = root('[hosts]\nbuild = "<ssh alias of the build host>"\ntest = "alpha"\n', '[hosts]\nbuild = "beta"\n');
  expect(hostAliases(env)).toEqual({ build: "beta", test: "alpha", prod: "prod" });
});

test("AY_HOST_<ROLE> wins over the files", () => {
  const env = { ...root('[hosts]\nprod = "gamma"\n'), AY_HOST_PROD: "delta" };
  expect(hostAlias("prod", env)).toBe("delta");
});

test("a name that is not a role passes through; roles map back from aliases", () => {
  const env = root('[hosts]\nbuild = "beta"\n');
  expect(hostAlias("pc", env)).toBe("pc");
  expect(hostAlias("literal-alias", env)).toBe("literal-alias");
  expect(hostRole("beta", env)).toBe("build");
  expect(hostRole("test", env)).toBe("test");
  expect(hostRole("unknown", env)).toBeUndefined();
});

test("no configuration at all: the role is the alias", () => {
  expect(hostAlias("build", { AY_ROOT: mkdtempSync(join(tmpdir(), "ay-hosts-")) })).toBe("build");
});
