/** Resume state per (repo, kind) in sync_state. */
import type { Database } from "bun:sqlite";

export interface SyncState {
  cursor: string | null;
  high: string | null;
  until: string | null;
  status: string;
}

export function getState(db: Database, repo: string, kind: string): SyncState {
  return (
    db.query<SyncState, [string, string]>("SELECT cursor, high, until, status FROM sync_state WHERE repo = ? AND kind = ?").get(repo, kind) ?? { cursor: null, high: null, until: null, status: "idle" }
  );
}

export function setState(db: Database, repo: string, kind: string, s: Partial<SyncState> & { note?: string | null }): void {
  const cur = getState(db, repo, kind);
  const next = { ...cur, ...s };
  db.query(
    `INSERT INTO sync_state (repo, kind, cursor, high, until, status, updated_at, note) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
     ON CONFLICT(repo, kind) DO UPDATE SET cursor = excluded.cursor, high = excluded.high, until = excluded.until, status = excluded.status, updated_at = excluded.updated_at, note = excluded.note`,
  ).run(repo, kind, next.cursor, next.high, next.until, next.status, new Date().toISOString(), s.note ?? null);
}
