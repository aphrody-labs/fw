// Remaps bun:sqlite.Database to the node:sqlite.DatabaseSync API (Node v22.5+).
// Drop-in for code that imports `node:sqlite`.
//
// Node docs : https://nodejs.org/api/sqlite.html
// Bun docs  : https://bun.com/docs/runtime/sqlite

import { Database } from "bun:sqlite";

export interface DatabaseSyncOptions {
  open?: boolean;
  readOnly?: boolean;
  enableForeignKeyConstraints?: boolean;
}

export class DatabaseSync {
  #db: Database;
  #closed = false;
  readonly location: string | ":memory:";

  constructor(location: string = ":memory:", opts: DatabaseSyncOptions = {}) {
    this.location = location;
    this.#db = opts.readOnly
      ? new Database(location, { readonly: true })
      : new Database(location);
    if (opts.enableForeignKeyConstraints !== false) {
      this.#db.exec("PRAGMA foreign_keys = ON");
    }
  }

  exec(sql: string): void {
    this.#db.exec(sql);
  }

  prepare(sql: string) {
    const stmt = this.#db.prepare(sql);
    return {
      all: (...args: unknown[]) => stmt.all(...(args as never[])),
      get: (...args: unknown[]) => stmt.get(...(args as never[])),
      run: (...args: unknown[]) => stmt.run(...(args as never[])),
      iterate: (...args: unknown[]) => stmt.iterate(...(args as never[])),
      finalize: () => stmt.finalize(),
    };
  }

  close(): void {
    this.#db.close();
    this.#closed = true;
  }

  get isOpen(): boolean {
    return !this.#closed;
  }
}

export default { DatabaseSync };
