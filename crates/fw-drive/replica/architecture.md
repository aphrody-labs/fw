# Architecture: fw-drive (a Rust rebuild of Google Drive's file loop)

## Stack

| layer | choice | why |
| --- | --- | --- |
| core | `fw-drive` library, Rust 2024 | one crate holds the model, rules and storage, so the CLI, the server and tests share it |
| metadata | SQLite through `rusqlite` (bundled), one `drive.db` file | transactions, indexes and search with no server to run |
| content | content-addressed blobs, `blobs/<2 hex>/<blake3>` | everything is a file: identical uploads are stored once, a blob is plain bytes you can `cat` |
| web | `axum` 0.8, served by `fw-http` | the HTTP stack fw already ships (h1, h2, h3) |
| UI | HTML rendered in Rust plus `app.css` tokens from `fw-m3`; a short script only adds drag-and-drop | works with no JavaScript; colours come from the fw brand seed |
| auth | Cloudflare Access: `Cf-Access-Authenticated-User-Email` behind Access, or `--owner` for a single-user local drive | Google sign-in for people and service tokens for agents, with no password store in fw-drive |
| CLI | `fw-drive serve`, `ls`, `put`, `get`, `mkdir`, `rm` | agents use files and stdout |

## Schema

Tables: 2. Access rules: one check per operation in the data layer (`Drive::require`), inherited from ancestors.

```sql
create table nodes (
  id text primary key,                -- 22-char random base62
  parent text references nodes(id) on delete cascade,
  name text not null,
  kind text not null check (kind in ('file','folder')),
  mime text not null,
  size integer not null default 0,
  blob text,                          -- blake3 hex, null for folders
  owner text not null,
  starred integer not null default 0,
  trashed_at integer,                 -- unix ms; set on the node the user trashed
  trashed_by_ancestor integer not null default 0,
  created_at integer not null,
  modified_at integer not null
);
create index nodes_parent on nodes(parent, kind, name);
create index nodes_owner on nodes(owner, modified_at);
create index nodes_blob on nodes(blob);
create table shares (
  node text not null references nodes(id) on delete cascade,
  principal text not null,            -- an email, or '*' for anyone with the link
  role text not null check (role in ('reader','commenter','writer')),
  primary key (node, principal)
);
```

## API (JSON, under `/api/v1`)

| method path | does | who |
| --- | --- | --- |
| GET /files?parent=&view=&q=&type=&sort= | list a folder or a view (recent, starred, trash, shared) | reader |
| GET /files/{id} | metadata | reader |
| GET /files/{id}/content | bytes, Range and ETag | reader |
| POST /files?parent=&name= (body = bytes) | upload | writer of parent |
| POST /folders | create folder | writer of parent |
| PATCH /files/{id} | rename, move, star | writer (star: reader) |
| POST /files/{id}/trash, /restore | trash and restore | writer |
| DELETE /files/{id} | delete forever (from trash) | owner |
| POST /trash/empty | empty trash | owner |
| GET/PUT/DELETE /files/{id}/shares | sharing | owner |
| GET /quota | usage | any |

HTML pages map to recon screens: `/`, `/folders/{id}`, `/recent`, `/starred`, `/shared`, `/trash`, `/quota`, `/search`.

## The parts that bite

- Races: uploads stream to a temp file, hash, then rename into the blob path; the database insert is one transaction.
- Cycles: moving a folder under its own descendant is refused by walking the parents.
- Trash: trashing marks the subtree; restore clears it; delete forever removes rows, then deletes blobs no row references.
- Range: single byte ranges, `206` with `Content-Range`; ETag is the blake3 hash.
- Names: Drive allows duplicate names in one folder, so does fw-drive; ids are the identity.

## Build order

1. Vertical slice: S01, S02, F01, F02 (nodes table, upload, list, download).
2. Must-haves: rename, move, trash, search, share, Access identity.
3. Should-haves: starred, recent, shared with me, quota, preview.
4. Edge: run behind Cloudflare Access next to fw-edge, which keeps the Google Drive mirror at `/drive`.
