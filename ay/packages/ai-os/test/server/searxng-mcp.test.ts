import { describe, expect, test } from "bun:test";
import { resolveLaunch } from "../../src/server/searxng-mcp.ts";

const url = "http://127.0.0.1:8888";

describe("searxng-mcp launch", () => {
  test("local: mcp-searxng on Bun with the url and the [search.env] settings", () => {
    const l = resolveLaunch({ search: { url, env: { SEARXNG_MAX_RESULTS: "20" } } }, {});
    expect(l.argv).toEqual(["bun", "x", "--bun", "mcp-searxng"]);
    expect(l.env).toEqual({ SEARXNG_URL: url, SEARXNG_MAX_RESULTS: "20" });
  });

  test("SEARXNG_URL from the environment wins over the file", () => {
    expect(resolveLaunch({ search: { url } }, { SEARXNG_URL: "https://s.example/" }).env.SEARXNG_URL).toBe("https://s.example/");
  });

  test("via a host role: ssh to its alias with the settings on the remote command", () => {
    const l = resolveLaunch({ hosts: { test: "h2" }, search: { url, via: "test", env: { SEARXNG_MAX_RESULTS: "5" } } }, {});
    expect(l.argv.slice(0, 5)).toEqual(["ssh", "-o", "BatchMode=yes", "-o", "ServerAliveInterval=30"]);
    expect(l.argv[5]).toBe("h2");
    expect(l.argv[6]).toBe(`SEARXNG_URL=${url} SEARXNG_MAX_RESULTS=5 bunx mcp-searxng`);
    expect(l.env).toEqual({});
  });

  test("refuses a missing url, a non-http url, foreign settings, shell characters and unknown roles", () => {
    expect(() => resolveLaunch({}, {})).toThrow(/SEARXNG_URL/);
    expect(() => resolveLaunch({ search: { url: "file:///x" } }, {})).toThrow(/http/);
    expect(() => resolveLaunch({ search: { url, env: { PATH: "x" } } }, {})).toThrow(/SEARXNG_/);
    expect(() => resolveLaunch({ search: { url, env: { SEARXNG_X: "a; rm -rf /" } } }, {})).toThrow(/characters/);
    expect(() => resolveLaunch({ search: { url, via: "nope" } }, {})).toThrow(/via/);
    expect(() => resolveLaunch({ hosts: {}, search: { url, via: "test" } }, {})).toThrow(/hosts/);
  });
});
