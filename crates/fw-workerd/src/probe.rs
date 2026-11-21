//! A probe Worker that exercises every binding the compiler wires (R2, D1, KV, Queues with a consumer, a SQLite Durable Object, the Cache API,
//! assets and a cron trigger) and answers with deterministic JSON, so the same requests can be run against any mode and compared.

pub const WRANGLER: &str = r#"{
  "name": "fw-probe",
  "main": "worker.js",
  "compatibility_date": "2026-11-21",
  "r2_buckets": [{ "binding": "BUCKET", "bucket_name": "fw-probe" }],
  "d1_databases": [{ "binding": "DB", "database_name": "fw-probe", "database_id": "fw-probe" }],
  "kv_namespaces": [{ "binding": "KV", "id": "fw-probe" }],
  "queues": { "producers": [{ "binding": "QUEUE", "queue": "fw-probe" }], "consumers": [{ "queue": "fw-probe", "max_batch_timeout": 1 }] },
  "durable_objects": { "bindings": [{ "name": "COUNTER", "class_name": "Counter" }] },
  "migrations": [{ "tag": "v1", "new_sqlite_classes": ["Counter"] }],
  "triggers": { "crons": ["* * * * *"] },
  "vars": { "GREETING": "hello" }
}
"#;

pub const WORKER: &str = r#"export class Counter {
  constructor(state) { this.sql = state.storage.sql; this.sql.exec("CREATE TABLE IF NOT EXISTS n (v INTEGER)"); }
  async fetch() {
    this.sql.exec("INSERT INTO n (v) VALUES (1)");
    return Response.json({ count: [...this.sql.exec("SELECT COUNT(*) AS c FROM n")][0].c });
  }
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    const reply = (value) => Response.json(value);
    switch (url.pathname) {
      case "/vars": return reply({ greeting: env.GREETING });
      case "/r2": {
        await env.BUCKET.put("a/one.txt", "one", { httpMetadata: { contentType: "text/plain", cacheControl: "max-age=60" }, customMetadata: { k: "v" } });
        await env.BUCKET.put("a/two.txt", "two-two");
        const head = await env.BUCKET.head("a/one.txt");
        const ranged = await env.BUCKET.get("a/two.txt", { range: { offset: 1, length: 3 } });
        const listed = await env.BUCKET.list({ prefix: "a/" });
        const delimited = await env.BUCKET.list({ delimiter: "/" });
        const missing = await env.BUCKET.get("a/none.txt");
        const out = { size: head.size, type: head.httpMetadata.contentType, cache: head.httpMetadata.cacheControl, custom: head.customMetadata, range: await ranged.text(), keys: listed.objects.map((o) => o.key), prefixes: delimited.delimitedPrefixes, missing };
        await env.BUCKET.delete(["a/one.txt", "a/two.txt"]);
        out.after = (await env.BUCKET.list({ prefix: "a/" })).objects.length;
        return reply(out);
      }
      case "/d1": {
        await env.DB.exec("CREATE TABLE IF NOT EXISTS t (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT)");
        await env.DB.prepare("DELETE FROM t").run();
        const inserted = await env.DB.prepare("INSERT INTO t (name) VALUES (?)").bind("x").run();
        const batch = await env.DB.batch([env.DB.prepare("INSERT INTO t (name) VALUES (?)").bind("y"), env.DB.prepare("SELECT COUNT(*) AS c FROM t")]);
        const first = await env.DB.prepare("SELECT name FROM t WHERE id = ?").bind(1).first("name");
        const all = await env.DB.prepare("SELECT id, name FROM t ORDER BY id").all();
        const raw = await env.DB.prepare("SELECT id, name FROM t ORDER BY id").raw();
        return reply({ changes: inserted.meta.changes, lastRowId: inserted.meta.last_row_id, batchCount: batch[1].results[0].c, first, rows: all.results, raw });
      }
      case "/kv": {
        await env.KV.put("k1", "v1", { metadata: { m: 1 } });
        await env.KV.put("k2", JSON.stringify({ j: true }));
        const withMeta = await env.KV.getWithMetadata("k1");
        const json = await env.KV.get("k2", "json");
        const listed = await env.KV.list({ prefix: "k" });
        await env.KV.delete("k1");
        return reply({ value: withMeta.value, metadata: withMeta.metadata, json, keys: listed.keys.map((k) => k.name), gone: await env.KV.get("k1") });
      }
      case "/do": {
        const stub = env.COUNTER.get(env.COUNTER.idFromName("probe"));
        return reply(await (await stub.fetch("http://do/")).json());
      }
      case "/cache": {
        const key = new Request("https://probe.invalid/cached");
        await caches.default.put(key, new Response("cached-body", { headers: { "cache-control": "max-age=60" } }));
        const hit = await caches.default.match(key);
        return reply({ hit: hit ? await hit.text() : null });
      }
      case "/queue": {
        await env.QUEUE.send({ n: 1 });
        await env.QUEUE.sendBatch([{ body: { n: 2 } }, { body: { n: 3 } }]);
        return reply({ sent: 3 });
      }
      case "/queue/seen": return reply({ seen: await env.KV.get("queue-seen") });
      case "/cron/seen": return reply({ ran: (await env.KV.get("cron-ran")) !== null });
      default: return new Response("not found", { status: 404 });
    }
  },
  async queue(batch, env) {
    const seen = JSON.parse((await env.KV.get("queue-seen")) ?? "[]");
    for (const message of batch.messages) { seen.push(message.body.n); message.ack(); }
    await env.KV.put("queue-seen", JSON.stringify(seen.sort()));
  },
  async scheduled(controller, env) { await env.KV.put("cron-ran", "yes"); },
};
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_probe_wrangler_config_declares_every_binding_the_compiler_wires() {
        let value: serde_json::Value = serde_json::from_str(WRANGLER).expect("valid json");
        for key in ["r2_buckets", "d1_databases", "kv_namespaces", "queues", "durable_objects", "triggers"] {
            assert!(value.get(key).is_some(), "{key}");
        }
        assert!(WORKER.contains("async queue(") && WORKER.contains("async scheduled("));
    }
}
