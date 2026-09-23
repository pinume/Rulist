#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
web_root="$repo_root/web"

(
  cd "$web_root"
  if command -v corepack >/dev/null 2>&1; then
    CI=true corepack pnpm install --frozen-lockfile
    CI=true corepack pnpm build
  elif command -v pnpm >/dev/null 2>&1; then
    CI=true pnpm install --frozen-lockfile
    CI=true pnpm build
  elif command -v bun >/dev/null 2>&1; then
    NODE_ENV=production bun run build
  elif [[ -x "$HOME/.bun/bin/bun" ]]; then
    NODE_ENV=production "$HOME/.bun/bin/bun" run build
  else
    echo "No suitable package manager (corepack/pnpm/bun) found." >&2
    exit 1
  fi
)

mkdir -p "$repo_root/public/dist"
find "$repo_root/public/dist" -mindepth 1 ! -name README.md -delete
cp -a "$web_root/dist/." "$repo_root/public/dist/"

echo "Built Rulist frontend into $repo_root/public/dist"
