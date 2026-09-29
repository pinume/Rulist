# <img src="web/public/rulist.svg" width="32" alt="" /> Rulist

A lightweight Linux-only file browser written in Rust. Rulist serves real
local files: every user is bound directly to one Linux directory.

## Build

Requirements:

- Rust 1.85+
- Node.js 22.13+
- pnpm 12.5.1

```bash
./build-frontend.sh
cargo build --release
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
file contents remain on the Linux filesystem.

Use HTTPS in production.
