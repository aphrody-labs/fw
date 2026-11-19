import { Database } from "bun:sqlite";
import { existsSync, mkdirSync, readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

export interface VfsEntry {
  path: string;
  kind: "file" | "dir";
  bytes: number;
  sha256?: string;
  mount: string;
}

function openVfsDb(root: string): Database {
  const dbPath = resolve(root, "target", "fairwind-vfs.db");
  mkdirSync(dirname(dbPath), { recursive: true });
  const db = new Database(dbPath);
  db.exec(`
    CREATE TABLE IF NOT EXISTS blobs (
      sha256 TEXT PRIMARY KEY,
      bytes INTEGER NOT NULL,
      content BLOB NOT NULL
    );
    CREATE TABLE IF NOT EXISTS files (
      vpath TEXT PRIMARY KEY,
      sha256 TEXT NOT NULL,
      bytes INTEGER NOT NULL,
      updated_at TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS snapshots (
      id TEXT PRIMARY KEY,
      created_at TEXT NOT NULL,
      file_count INTEGER NOT NULL,
      manifest_json TEXT NOT NULL
    );
  `);
  return db;
}

export function normalizeVPath(vpath: string): string {
  const cleaned = vpath.replaceAll("\\", "/").replace(/\/+/g, "/").trim();
  if (!cleaned || cleaned === "/") return "/";
  const withSlash = cleaned.startsWith("/") ? cleaned : `/${cleaned}`;
  return withSlash.length > 1 ? withSlash.replace(/\/$/, "") : "/";
}

export function listVfsMounts(root: string) {
  return {
    ok: true,
    mounts: [
      { mount: "/repo", backing: root, mode: "read-only-host" },
      { mount: "/target", backing: resolve(root, "target"), mode: "read-write-host" },
      { mount: "/overlay", backing: resolve(root, "target", "fairwind-vfs.db"), mode: "read-write-cas-sqlite" },
      { mount: "/models", backing: resolve(root, "target", "models"), mode: "read-write-host" },
      { mount: "/datasets", backing: resolve(root, "target", "datasets"), mode: "read-write-host" },
    ],
  };
}

export function vfsWrite(root: string, vpath: string, content: string | Uint8Array) {
  const norm = normalizeVPath(vpath);
  const bytes = typeof content === "string" ? new TextEncoder().encode(content) : content;
  const hasher = new Bun.CryptoHasher("sha256");
  hasher.update(bytes);
  const sha256 = hasher.digest("hex");
  const now = new Date().toISOString();

  const db = openVfsDb(root);
  try {
    const insBlob = db.query("INSERT OR IGNORE INTO blobs (sha256, bytes, content) VALUES (?, ?, ?)");
    const insFile = db.query(
      "INSERT OR REPLACE INTO files (vpath, sha256, bytes, updated_at) VALUES (?, ?, ?, ?)",
    );
    insBlob.run(sha256, bytes.byteLength, bytes);
    insFile.run(norm, sha256, bytes.byteLength, now);
    insBlob.finalize();
    insFile.finalize();
  } finally {
    db.close();
  }
  return { ok: true, path: norm, bytes: bytes.byteLength, sha256, updatedAt: now };
}

export function vfsRead(root: string, vpath: string): { ok: boolean; path: string; content?: string; sha256?: string; error?: string } {
  const norm = normalizeVPath(vpath);
  const db = openVfsDb(root);
  try {
    const q = db.query(
      "SELECT f.sha256 as sha256, b.content as content FROM files f JOIN blobs b ON f.sha256 = b.sha256 WHERE f.vpath = ?",
    );
    const row = q.get(norm) as { sha256: string; content: Uint8Array } | null;
    q.finalize();
    if (row) {
      return {
        ok: true,
        path: norm,
        sha256: row.sha256,
        content: new TextDecoder().decode(row.content),
      };
    }
  } finally {
    db.close();
  }

  if (norm.startsWith("/repo/")) {
    const rel = norm.slice("/repo/".length);
    const abs = resolve(root, rel);
    if (abs.startsWith(resolve(root)) && existsSync(abs) && statSync(abs).isFile()) {
      const buf = readFileSync(abs);
      const h = new Bun.CryptoHasher("sha256");
      h.update(buf);
      return { ok: true, path: norm, sha256: h.digest("hex"), content: buf.toString("utf8") };
    }
  }
  return { ok: false, path: norm, error: "not found in VFS" };
}

export function vfsList(root: string, vpath = "/"): { ok: boolean; path: string; entries: VfsEntry[] } {
  const norm = normalizeVPath(vpath);
  const entries: VfsEntry[] = [];

  if (norm === "/") {
    for (const m of ["/repo", "/target", "/overlay", "/models", "/datasets"]) {
      entries.push({ path: m, kind: "dir", bytes: 0, mount: m });
    }
  } else if (norm === "/repo" || norm.startsWith("/repo/")) {
    const rel = norm === "/repo" ? "" : norm.slice("/repo/".length);
    const abs = resolve(root, rel);
    if (abs.startsWith(resolve(root)) && existsSync(abs) && statSync(abs).isDirectory()) {
      for (const e of readdirSync(abs, { withFileTypes: true }).slice(0, 200)) {
        const childAbs = join(abs, e.name);
        entries.push({
          path: `${norm}/${e.name}`,
          kind: e.isDirectory() ? "dir" : "file",
          bytes: e.isFile() ? statSync(childAbs).size : 0,
          mount: "/repo",
        });
      }
    }
  }

  const db = openVfsDb(root);
  try {
    const prefix = norm === "/" ? "/" : `${norm}/`;
    const q = db.query("SELECT vpath, sha256, bytes FROM files WHERE vpath LIKE ? ORDER BY vpath");
    const rows = q.all(`${prefix}%`) as Array<{ vpath: string; sha256: string; bytes: number }>;
    q.finalize();
    for (const r of rows) {
      entries.push({ path: r.vpath, kind: "file", bytes: r.bytes, sha256: r.sha256, mount: "/overlay" });
    }
  } finally {
    db.close();
  }

  return { ok: true, path: norm, entries };
}

export function vfsSnapshot(root: string, label?: string) {
  const db = openVfsDb(root);
  const id = label?.trim() || `snap-${Date.now()}`;
  const createdAt = new Date().toISOString();
  try {
    const q = db.query("SELECT vpath, sha256, bytes FROM files ORDER BY vpath");
    const rows = q.all() as Array<{ vpath: string; sha256: string; bytes: number }>;
    q.finalize();
    const ins = db.query(
      "INSERT OR REPLACE INTO snapshots (id, created_at, file_count, manifest_json) VALUES (?, ?, ?, ?)",
    );
    ins.run(id, createdAt, rows.length, JSON.stringify(rows));
    ins.finalize();
    return { ok: true, id, createdAt, fileCount: rows.length };
  } finally {
    db.close();
  }
}

export function vfsSnapshots(root: string) {
  const db = openVfsDb(root);
  try {
    const q = db.query("SELECT id, created_at, file_count FROM snapshots ORDER BY created_at DESC LIMIT 50");
    const rows = q.all() as Array<{ id: string; created_at: string; file_count: number }>;
    q.finalize();
    return { ok: true, snapshots: rows };
  } finally {
    db.close();
  }
}
