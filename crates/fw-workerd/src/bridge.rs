//! The `BLOBS` service of workerd's bundled R2 Worker, backed by an S3 endpoint (Garage). workerd's own `bucket` service writes to a local
//! directory, or to an S3 cluster bucket that must honour conditional writes, which Garage structurally cannot. This Worker speaks the same small
//! HTTP protocol (HEAD, GET with Range, PUT, DELETE, `?list`, multipart) over plain S3 calls signed with SigV4, so R2 objects stay ordinary S3
//! objects that rclone and the AWS SDKs can read and write. The `If-Match` and `If-None-Match` guards of a PUT are checked with a HEAD first: they are
//! not atomic against a concurrent writer of the same key.

pub const BRIDGE_FILE: &str = "fw-s3-bridge.js";

pub const BRIDGE_MODULE: &str = r##"const encoder = new TextEncoder();
const hex = (buffer) => [...new Uint8Array(buffer)].map((byte) => byte.toString(16).padStart(2, "0")).join("");
async function sha256(text) { return hex(await crypto.subtle.digest("SHA-256", encoder.encode(text))); }
async function hmac(key, text) {
  const imported = await crypto.subtle.importKey("raw", key, { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
  return crypto.subtle.sign("HMAC", imported, encoder.encode(text));
}
const encodeComponent = (text) => encodeURIComponent(text).replace(/[!'()*]/g, (c) => "%" + c.charCodeAt(0).toString(16).toUpperCase());
const unescapeXml = (text) => text.replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&quot;/g, '"').replace(/&apos;/g, "'").replace(/&amp;/g, "&");
const escapeXml = (text) => text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
const unquote = (etag) => (etag ?? "").replace(/^W\//, "").replace(/^"|"$/g, "");
const tag = (xml, name) => { const match = new RegExp("<" + name + ">([\\s\\S]*?)</" + name + ">").exec(xml); return match ? unescapeXml(match[1]) : undefined; };
const json = (value, status = 200) => Response.json(value, { status });
const failure = (code, message, status) => Response.json({ code, message }, { status });

async function signed(env, method, key, query, headers, body) {
  const path = "/" + env.BUCKET + (key === null ? "" : "/" + key.split("/").map(encodeComponent).join("/"));
  const canonicalQuery = Object.keys(query).sort().map((name) => encodeComponent(name) + "=" + encodeComponent(String(query[name]))).join("&");
  const stamp = new Date().toISOString().replace(/[:-]|\.\d{3}/g, "");
  const day = stamp.slice(0, 8);
  const toSign = { host: env.S3_HOST, "x-amz-content-sha256": "UNSIGNED-PAYLOAD", "x-amz-date": stamp };
  for (const [name, value] of Object.entries(headers)) if (name.startsWith("x-amz-")) toSign[name] = String(value).trim();
  const names = Object.keys(toSign).sort();
  const canonical = [method, path, canonicalQuery, names.map((name) => name + ":" + toSign[name] + "\n").join(""), names.join(";"), "UNSIGNED-PAYLOAD"].join("\n");
  const scope = day + "/" + env.REGION + "/s3/aws4_request";
  let secret = await hmac(encoder.encode("AWS4" + env.SECRET_ACCESS_KEY), day);
  for (const part of [env.REGION, "s3", "aws4_request"]) secret = await hmac(secret, part);
  const signature = hex(await hmac(secret, ["AWS4-HMAC-SHA256", stamp, scope, await sha256(canonical)].join("\n")));
  const outgoing = new Headers(headers);
  for (const [name, value] of Object.entries(toSign)) if (name !== "host") outgoing.set(name, value);
  outgoing.set("authorization", `AWS4-HMAC-SHA256 Credential=${env.ACCESS_KEY_ID}/${scope}, SignedHeaders=${names.join(";")}, Signature=${signature}`);
  const url = "http://" + env.S3_HOST + path + (canonicalQuery ? "?" + canonicalQuery : "");
  return env.S3.fetch(url, { method, headers: outgoing, body });
}

const CONTENT_HEADERS = ["content-type", "cache-control", "content-encoding", "content-language", "content-disposition"];

function toS3Headers(headers) {
  const out = {};
  for (const [name, value] of headers) {
    if (name.startsWith("x-meta-")) out["x-amz-meta-" + name.slice(7)] = value;
    else if (CONTENT_HEADERS.includes(name)) out[name] = value;
  }
  return out;
}

function fromS3Headers(response) {
  const out = new Headers();
  for (const name of [...CONTENT_HEADERS, "content-length", "content-range", "accept-ranges"]) if (response.headers.get(name)) out.set(name, response.headers.get(name));
  const etag = unquote(response.headers.get("etag"));
  out.set("etag", '"' + etag + '"');
  out.set("x-version", etag);
  out.set("x-modified", String(Date.parse(response.headers.get("last-modified") ?? "") || Date.now()));
  for (const [name, value] of response.headers) if (name.startsWith("x-amz-meta-")) out.set("x-meta-" + name.slice(11), value);
  return out;
}

async function listing(env, url) {
  const query = { "list-type": 2 };
  const prefix = url.searchParams.get("prefix");
  if (prefix) query.prefix = prefix;
  if (url.searchParams.get("start-after")) query["start-after"] = url.searchParams.get("start-after");
  if (url.searchParams.get("delimiter")) query.delimiter = url.searchParams.get("delimiter");
  if (url.searchParams.get("max")) query["max-keys"] = url.searchParams.get("max");
  if (url.searchParams.get("token")) query["continuation-token"] = url.searchParams.get("token");
  const response = await signed(env, "GET", null, query, {});
  if (!response.ok) return failure("InternalError", "list failed (" + response.status + ")", 500);
  const xml = await response.text();
  const objects = [...xml.matchAll(/<Contents>([\s\S]*?)<\/Contents>/g)].map((match) => {
    const etag = unquote(tag(match[1], "ETag"));
    return { key: tag(match[1], "Key"), size: Number(tag(match[1], "Size")), etag, version: etag, modified: Date.parse(tag(match[1], "LastModified")) };
  });
  const prefixes = [...xml.matchAll(/<CommonPrefixes>\s*<Prefix>([\s\S]*?)<\/Prefix>\s*<\/CommonPrefixes>/g)].map((match) => unescapeXml(match[1]));
  return json({ objects, prefixes, token: tag(xml, "IsTruncated") === "true" ? tag(xml, "NextContinuationToken") ?? null : null });
}

async function guard(env, key, request) {
  const noneMatch = request.headers.get("if-none-match");
  const match = request.headers.get("if-match");
  if (!noneMatch && !match) return null;
  const current = await signed(env, "HEAD", key, {}, {});
  const etag = current.status === 404 ? null : unquote(current.headers.get("etag"));
  if (noneMatch === "*" && etag !== null) return new Response(null, { status: 412 });
  if (match && match !== etag) return new Response(null, { status: 412 });
  return null;
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (url.searchParams.has("list")) return listing(env, url);
    const key = decodeURIComponent(url.pathname.slice(1));
    const upload = url.searchParams.get("upload");
    const method = request.method;
    if (method === "HEAD" || (method === "GET" && !upload)) {
      const headers = {};
      if (request.headers.get("range")) headers.range = request.headers.get("range");
      const response = await signed(env, method, key, {}, headers);
      if (response.status === 404) return method === "HEAD" ? new Response(null, { status: 404 }) : failure("NoSuchKey", "The specified key does not exist.", 404);
      if (response.status === 416) return failure("InvalidRange", "range not satisfiable", 416);
      if (!response.ok && response.status !== 206) return failure("InternalError", "S3 " + method + " " + response.status, 500);
      return new Response(method === "HEAD" ? null : response.body, { status: response.status, headers: fromS3Headers(response) });
    }
    if (method === "PUT" && url.searchParams.has("part")) {
      const response = await signed(env, "PUT", key, { partNumber: url.searchParams.get("part"), uploadId: upload }, {}, await request.arrayBuffer());
      if (!response.ok) return failure("InternalError", "S3 part " + response.status, 500);
      return json({ etag: unquote(response.headers.get("etag")) });
    }
    if (method === "PUT") {
      const refused = await guard(env, key, request);
      if (refused) return refused;
      const body = await request.arrayBuffer();
      const response = await signed(env, "PUT", key, {}, toS3Headers(request.headers), body);
      if (!response.ok) return failure("InternalError", "S3 put " + response.status, 500);
      const headers = new Headers({ etag: '"' + unquote(response.headers.get("etag")) + '"', "x-version": unquote(response.headers.get("etag")), "x-modified": String(Date.now()) });
      return new Response(null, { status: 200, headers });
    }
    if (method === "POST" && url.searchParams.has("uploads")) {
      const response = await signed(env, "POST", key, { uploads: "" }, toS3Headers(request.headers));
      if (!response.ok) return failure("InternalError", "S3 create " + response.status, 500);
      return json({ uploadId: tag(await response.text(), "UploadId") });
    }
    if (method === "GET" && upload) {
      const parts = [];
      let marker;
      do {
        const query = { uploadId: upload };
        if (marker) query["part-number-marker"] = marker;
        const response = await signed(env, "GET", key, query, {});
        if (response.status === 404) return failure("NoSuchUpload", "The specified upload does not exist.", 404);
        if (!response.ok) return failure("InternalError", "S3 parts " + response.status, 500);
        const xml = await response.text();
        for (const match of xml.matchAll(/<Part>([\s\S]*?)<\/Part>/g)) parts.push({ part: Number(tag(match[1], "PartNumber")), etag: unquote(tag(match[1], "ETag")), size: Number(tag(match[1], "Size")) });
        marker = tag(xml, "IsTruncated") === "true" ? tag(xml, "NextPartNumberMarker") : undefined;
      } while (marker);
      return json(parts);
    }
    if (method === "POST" && upload) {
      const chosen = await request.json();
      const xml = "<CompleteMultipartUpload>" + chosen.map((part) => `<Part><PartNumber>${part.part}</PartNumber><ETag>"${escapeXml(part.etag)}"</ETag></Part>`).join("") + "</CompleteMultipartUpload>";
      const response = await signed(env, "POST", key, { uploadId: upload }, { "content-type": "application/xml" }, xml);
      if (!response.ok) return failure("InternalError", "S3 complete " + response.status, 500);
      return json(null);
    }
    if (method === "DELETE" && upload) {
      const response = await signed(env, "DELETE", key, { uploadId: upload }, {});
      if (response.status === 404) return failure("NoSuchUpload", "The specified upload does not exist.", 404);
      return new Response(null, { status: response.ok ? 204 : 500 });
    }
    if (method === "DELETE") {
      const response = await signed(env, "DELETE", key, {}, {});
      return new Response(null, { status: response.ok || response.status === 404 ? 204 : 500 });
    }
    return failure("InvalidRequest", "unsupported request", 400);
  },
};
"##;
