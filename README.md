# <img src="web/public/rulist.svg" width="32" alt="" /> Rulist

A lightweight file listing tool written in Rust.

Rulist is designed to run on Linux.

## Build

Requirements:

- Rust 1.85+
- Node.js 22.13+
- pnpm 12.5.1

```bash
./build-frontend.sh
cargo build --release --locked
```

## Run

```bash
./target/release/rulist interactive
./target/release/rulist server
```

Use HTTPS in production.
