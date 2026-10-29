# Lowkey implementation plan

A low-resource key/value HTTP database in Rust. This document records the
design decisions behind the implementation.

## Decisions

- **Auth:** HTTP Basic auth. Writes require the owner (or admin); reads are public,
  including listings.
- **Signup:** admin only. The admin is the reserved user `admin`, whose password
  hash lives in the config (`admin_password_hash`, generated with
  `lowkey hash-password`). The admin may also manage any user's account.
- **Login cache:** after a successful Argon2 check, a SHA-256 digest of the
  password is cached for 5 minutes alongside the hash it matched, so password
  changes and deletions invalidate it automatically.
- **Storage:** plain files on disk, atomic writes (temp file, fsync, rename).
- **Anonymous documents:** `POST /:user/:db` returns `201`, a `Location` header and
  a ULID.
- **Content:** bodies are stored verbatim with their content type. JSON
  (`application/json`, `text/json` alias) is validated on write.
- **Key lookup:** nested paths like JSON Pointer (`/or/0`, `~1` escapes `/`,
  `~0` escapes `~`), returned as compact JSON. Array indexes have no leading
  zeros. Asking for a key in a non-JSON document is `404` with an explanation
  (`415` would wrongly suggest the request body was at fault). A malformed `~`
  escape is `400`.
- **PUT** replaces the whole document. Deleting a user removes all their data
  immediately. One process owns the data directory.
- **Limits** (`[limits]` in the config, defaults in parentheses):
  - `max_document_bytes` (1 MiB): document bodies only, `413` beyond. Forms
    keep axum's default limit.
  - Per-user quotas `max_bytes_per_user` (100 MiB, bodies only),
    `max_documents_per_user` (10 000), `max_databases_per_user` (50): `507`
    beyond. Usage is scanned from disk at startup and tracked in memory under
    a per-user lock, so concurrent writes can't overshoot. A change that
    doesn't grow a counter is always allowed, so users over a lowered limit
    can still clean up.
  - `requests_per_minute` (600) per client address, and
    `failed_logins_per_minute` (10): once reached, logins from that address
    are refused without checking the password. Both `429` with
    `Retry-After`, fixed one-minute windows, `0` disables.
  - `trust_forwarded_for` (false, top level): take the client address from
    the last `X-Forwarded-For` entry, for use behind a reverse proxy.
- **Deployment:** static binary, systemd, TLS terminated by a reverse proxy (Caddy).

## API

| Method | Path | Auth | Purpose |
|---|---|---|---|
| POST | `/users` (form `username`, `password`) | admin | create user |
| PUT | `/users/:user/password` | self or admin | change password |
| DELETE | `/users/:user` | self or admin | delete user and data |
| GET | `/:user` | public | list databases |
| POST | `/:user` (form `dbname`) | owner | create database |
| DELETE | `/:user/:db` | owner | delete database |
| GET | `/:user/:db` | public | list documents |
| POST | `/:user/:db` | owner | create anonymous document |
| PUT | `/:user/:db/:doc` | owner | create or replace named document |
| GET | `/:user/:db/:doc` | public | raw body with original content type |
| DELETE | `/:user/:db/:doc` | owner | delete document |
| GET | `/:user/:db/:doc/*path` | public | nested JSON value |

Reserved usernames: `users`, `admin`.

Responses:
- Listings are JSON arrays of names, sorted.
- Creating returns `201` with a `Location` header; `POST /:user/:db` also
  returns `{"id": "<ulid>"}`.
- `PUT` returns `201` when it creates a document and `204` when it replaces one.
- Deletes and password changes return `204`.
- Errors are plain text: `400` invalid name or body, `401` missing or wrong
  login (with `WWW-Authenticate`), `403` not yours, `404`, `409` name taken,
  `413` document too large, `415` missing or unsupported content type, `429`
  rate limited, `507` quota exceeded.

The admin may write to any user's databases, for moderation.

## Architecture

Dependencies point inward only:

```
http  ──►  service  ──►  storage
  │           │             │
  └───────────┴──────►  domain   (depends on nothing)
```

- **domain:** pure types and rules, no I/O. Validated name types (`Username`,
  `DbName`, `DocName`) that are safe to use as path segments, `Document`,
  `ContentType`, `JsonPath`.
- **storage:** `FsStore` knows the directory layout and atomic writes, nothing
  about permissions or quotas. No trait until a second backend exists.
- **service:** behavior. `Users`, `Databases`, `Documents` take a `Caller`
  (`Anonymous`, `User`, `Admin`) and enforce authorization, quotas and JSON
  validation. `auth` hashes and caches credentials; `quota` tracks usage.
  Errors are a single `ServiceError` with no HTTP knowledge.
- **http:** translation only. Route table, thin handlers, the `Caller`
  extractor, the one `ServiceError -> Response` mapping, tower middleware.
- **config / main:** typed config, wiring. `anyhow` only in `main`.

### Layout

```
src/
  main.rs  config.rs  lib.rs
  domain/    mod.rs  name.rs  document.rs  json_path.rs
  storage/   mod.rs  fs.rs  error.rs
  service/   mod.rs  auth.rs  quota.rs  users.rs  databases.rs  documents.rs  error.rs
  http/      mod.rs  routes.rs  handlers.rs  extract.rs  error.rs  middleware.rs
tests/       api.rs
```

### Storage layout

```
data/<user>/user.json             # argon2 hash
data/<user>/<db>/<doc>            # content type line, then the raw body
```

Names match `[A-Za-z0-9_-]{1,64}`, so they can never contain a dot. Content
type and body share one file so a single atomic rename replaces both. Temporary
files and pending deletions are hidden (`.tmp-*`, `.deleted-*`) and removed
when the store opens.

### Conventions

- Rust 2024, stable, single crate, `rustfmt` and `clippy -D warnings`.
- `thiserror` in the library, `From` conversions between layers.
- Readability over cleverness: no custom macros, short functions, doc comments
  on public items.
- Tests per layer; HTTP tests drive `build_app` with `tower::ServiceExt::oneshot`.
- Each feature starts with a user-story test ("As a ..., I can ..., so that
  ..."), then the implementation.

## Milestones

1. Cargo project, config loading, axum skeleton, `/healthz`, logging.
2. Storage layer: names, atomic writes, startup scan, tests.
3. Users and auth: admin, Basic auth with credential cache, user management.
4. Database and document endpoints.
5. Nested JSON key lookup.
6. Size limits, quotas, rate limiting.
7. Integration tests, systemd unit, sample Caddyfile, README update.
