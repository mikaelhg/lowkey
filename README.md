# Lowkey, a low resource key/value HTTP database

Use a cheap low cpu/memory/disk VM to serve a simple key/value database over HTTP.

Lowkey stores JSON, text and binary documents in named databases, one set per
user, as plain files on disk. Anyone can read; only a database's owner can
write. It is a single 2.6 MB binary that idles at about 4 MB of memory and stays
under 50 MB under a burst of logins.

## Quick start

```sh
cargo build --release
export LOWKEY_ADMIN_PASSWORD_HASH="$(target/release/lowkey hash-password)"
target/release/lowkey
```

`hash-password` reads the admin password from stdin. The server listens on
`127.0.0.1:8080` and keeps data in `./data`. Then:

```sh
# The admin creates a user
curl -u admin:ADMIN-PASSWORD -d 'username=foobar&password=barfoo' localhost:8080/users

# The user creates a database and stores a document
curl -u foobar:barfoo -d 'dbname=xyzzy' localhost:8080/foobar
curl -u foobar:barfoo -X PUT -H 'Content-Type: application/json' \
     --data '{"any": "valid", "json": "document", "or": ["you", "know"]}' \
     localhost:8080/foobar/xyzzy/doc

# Anyone reads one value from inside it
curl localhost:8080/foobar/xyzzy/doc/or
["you","know"]
```

## API

Writes need HTTP Basic auth as the owner (or the admin). Reads are public.
Names of users, databases and documents are 1 to 64 letters, digits, `_` or
`-`; `admin` and `users` can't be usernames.

### Users

```text
POST   /users                      username=foobar&password=barfoo   admin only → 201
PUT    /users/:username/password   password=new-secret               self or admin → 204
DELETE /users/:username                                              self or admin → 204
```

Deleting a user deletes all their data.

### Databases

```text
GET    /:username                  list databases → ["xyzzy"]
POST   /:username                  dbname=xyzzy → 201, Location: /foobar/xyzzy
DELETE /:username/:dbname          deletes the database and its documents → 204
```

### Documents

```text
GET    /:username/:dbname                  list documents → ["doc", ...]
POST   /:username/:dbname                  push an anonymous document → 201
PUT    /:username/:dbname/:document        create (201) or replace (204)
GET    /:username/:dbname/:document        the document, with its content type
DELETE /:username/:dbname/:document        → 204
```

A document's `Content-Type` must be one of:

- `application/json` (or `text/json`): must be valid JSON.
- `text/plain`: must be valid UTF-8.
- `application/octet-stream`: anything.

Anonymous documents get a time-ordered ID, returned as `{"id": "01J..."}` and
in the `Location` header.

### Values inside JSON documents

```text
GET /:username/:dbname/:document/:key
GET /:username/:dbname/:document/:key/:index/...
```

The path walks into objects by key and arrays by index, like a JSON Pointer:
`/foobar/xyzzy/doc/or/1` returns `"know"`. In keys, write `/` as `~1` and `~`
as `~0`. Values come back as JSON.

### Errors

Errors are plain text with these statuses:

| Status | Meaning |
|---|---|
| 400 | Invalid name, body, or path escape |
| 401 | Missing or wrong login |
| 403 | Not yours to change |
| 404 | No such user, database, document or key; or a key lookup in a non-JSON document |
| 409 | Name already taken |
| 413 | Document too large |
| 415 | Missing or unsupported `Content-Type` |
| 429 | Too many requests or failed logins; see `Retry-After` |
| 507 | Storage quota exceeded |

`GET /healthz` returns `ok` for monitoring.

## Configuration

Settings come from an optional TOML file named by `LOWKEY_CONFIG`.
[`deploy/lowkey.toml`](deploy/lowkey.toml) lists every setting with its default.
The environment variables `LOWKEY_LISTEN`, `LOWKEY_DATA_DIR` and
`LOWKEY_ADMIN_PASSWORD_HASH` override the file, and `RUST_LOG` sets the log
level.

The defaults suit a small VM:

- **Size:** documents up to 1 MiB.
- **Quotas:** each user may store up to 100 MiB, 10 000 documents and 50
  databases.
- **Rate limits:** each client address may make 600 requests and 10 failed
  logins per minute. After that, logins from that address are refused without
  checking the password.

## Deployment

[`deploy/`](deploy/) has what a small Linux VM needs:

- [`lowkey.service`](deploy/lowkey.service): a hardened systemd unit. It runs
  as a throwaway user, keeps data in `/var/lib/lowkey` and caps memory at
  256 MB.
- [`lowkey.toml`](deploy/lowkey.toml): the config, set up for running behind a
  proxy.
- [`Caddyfile`](deploy/Caddyfile): Caddy in front, for automatic TLS. Basic
  auth sends passwords with every request, so always serve Lowkey over TLS.

To install:

```sh
cargo build --release
sudo install target/release/lowkey /usr/local/bin/
sudo install -D -m 0644 deploy/lowkey.toml /etc/lowkey/lowkey.toml
sudo install -m 0644 deploy/lowkey.service /etc/systemd/system/
printf "LOWKEY_ADMIN_PASSWORD_HASH='%s'\n" "$(lowkey hash-password)" \
    | sudo install -m 0600 /dev/stdin /etc/lowkey/admin.env
sudo systemctl enable --now lowkey
```

Keep the single quotes around the hash in `admin.env`, because the hash
contains `$` characters. To build one static binary that runs on any Linux,
build with `--target x86_64-unknown-linux-musl`.

Back up by copying `/var/lib/lowkey`. Each document is a plain file, with its
content type on the first line and the raw body after it.

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
```

The tests include end-to-end user stories in [`tests/`](tests/), which drive
the real router over a temporary data directory. Each feature starts with a
story like *"As a user, I can …, so that …"*.
[`PLAN.md`](PLAN.md) records the design: the layers (domain, storage, service,
HTTP), the storage format, and the reasoning behind each decision.
