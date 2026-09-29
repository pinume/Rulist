# <img src="web/public/rulist.svg" width="32" alt="" /> Rulist

A lightweight Linux-only file browser written in Rust. Rulist serves real
local files: every user is bound directly to one Linux directory.

## Build

Requirements:

- Rust 1.85+
- Node.js 22.13+
- pnpm 12.5.1

```bash
./scripts/build-release.sh
```

## Run

```bash
# Opens the interactive management console and creates the initial admin user.
./target/release/rulist

# Starts the HTTP server on 127.0.0.1:5244 by default.
./target/release/rulist server
```

On first run, Rulist creates `config.json` and `data.db` in the data directory
(`data` by default). If `~/.rulist/data` already exists, it is used as the
default data directory instead. Set a different location with
`--data-dir /path/to/data` or `RULIST_DATA_DIR`.

The initial `admin` account is bound to the current user's `$HOME`. Add normal
users through the interactive console and assign each one an existing absolute
local directory.

`config.json` stores the JWT secret, token lifetime, database path, HTTP scheme,
and site settings. SQLite stores only users and authentication-security state;
file contents remain on the Linux filesystem. Rulist exposes only
UTF-8-compatible file names through the web interface.

## Security model and deployment

Rulist listens only on `127.0.0.1` or `::1`. For remote access, put it behind a
trusted reverse proxy that provides HTTPS, and do not expose Rulist's local
port directly to the network.

Each account is bound to an existing absolute Linux directory. Rulist applies
its account permissions inside that root, while the Linux user running Rulist
must also have the required filesystem access.

## Backups and upgrades

Back up Rulist's `config.json`, its configured SQLite database (default:
`data.db`), and each user's original directory. Stop Rulist before making
file-based copies of the database. Protect the configuration and backups as
they contain authentication data.

Build upgrades with `./scripts/build-release.sh` and keep the existing
configuration, database, and user directories. The database records schema
version 1; an unversioned database with the current schema is recognized, and a
database created by a newer Rulist version is rejected by older binaries.

## Permissions

Manage users in the interactive console. Per-user permissions can allow
creating and uploading files, renaming, moving, copying, deleting, overwriting,
and passwordless login. Rulist's administrator account can manage all files;
Linux filesystem permissions still apply to every account.
