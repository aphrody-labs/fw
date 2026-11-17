import { describe, expect, test } from "bun:test";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { hostAlias, hostWork, loadHosts, type HostConfig } from "./hosts.ts";

const cfg: HostConfig = { hosts: { build: "b1" }, paths: { build: { work: "/vol/x/" } } };

describe("host facts", () => {
  test("alias and work volume come from the config", () => {
    expect(hostAlias("build", cfg)).toBe("b1");
    expect(hostWork("build", cfg)).toBe("/vol/x");
  });
  test("a missing entry is an error naming the key", () => {
    expect(() => hostWork("prod", cfg)).toThrow("[paths.prod].work");
    expect(() => hostAlias("test", cfg)).toThrow("[hosts].test");
  });
  test.skipIf(!existsSync(join(import.meta.dir, "..", "ay.local.toml")))("ay.local.toml defines a work volume for the build and test roles", () => {
    const real = loadHosts();
    expect(hostWork("build", real)).toStartWith("/");
    expect(hostWork("test", real)).toStartWith("/");
  });
});
