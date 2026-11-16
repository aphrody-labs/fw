import { describe, expect, test } from "bun:test";
import { ApiError, getJson, HttpError, query, stripXssi, upstreamError } from "../src/http.ts";

const reply = (body: string, status = 200) => async () => new Response(body, { status });

describe("http", () => {
  test("stripXssi removes the Chrome Status guard only", () => {
    expect(stripXssi(")]}'\n{\"a\":1}")).toBe('{"a":1}');
    expect(stripXssi('{"a":1}')).toBe('{"a":1}');
  });

  test("query drops undefined values and encodes the rest", () => {
    expect(query({ a: 1, b: undefined, c: "x y", d: false })).toBe("?a=1&c=x+y&d=false");
    expect(query({})).toBe("");
  });

  test("a pydantic error list inside a JSON string is an ApiError", () => {
    const inner = JSON.stringify([{ type: "value_error", loc: ["channel"], msg: "Value error, Invalid channel: Foo" }]);
    const e = upstreamError("u", inner);
    expect(e).toBeInstanceOf(ApiError);
    expect(e?.message).toContain("channel: Value error, Invalid channel: Foo");
    expect(upstreamError("u", { error: "User not authenticated" })?.message).toContain("User not authenticated");
    expect(upstreamError("u", { error: "x", other: 1 })).toBeUndefined();
    expect(upstreamError("u", [{ a: 1 }])).toBeUndefined();
  });

  test("getJson parses XSSI-guarded bodies", async () => {
    expect(await getJson("u", { fetch: reply(")]}'\n{\"ok\":true}") })).toEqual({ ok: true });
  });

  test("getJson retries 5xx and then succeeds", async () => {
    let calls = 0;
    const flaky = async () => (++calls < 3 ? new Response("boom", { status: 503 }) : new Response("[1]"));
    expect(await getJson("u", { fetch: flaky, retries: 2 })).toEqual([1]);
    expect(calls).toBe(3);
  });

  test("getJson does not retry a 404", async () => {
    let calls = 0;
    const gone = async () => (calls++, new Response("<h1>Not Found</h1>", { status: 404 }));
    await expect(getJson("u", { fetch: gone, retries: 2 })).rejects.toBeInstanceOf(HttpError);
    expect(calls).toBe(1);
  });

  test("getJson does not retry an upstream validation error", async () => {
    let calls = 0;
    const bad = async () => (calls++, new Response(JSON.stringify({ error: "bad mstone" }), { status: 200 }));
    await expect(getJson("u", { fetch: bad })).rejects.toBeInstanceOf(ApiError);
    expect(calls).toBe(1);
  });
});
