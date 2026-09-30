#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
web_root="$repo_root/web"

(
  cd "$web_root"
  CI=true pnpm install --frozen-lockfile
  CI=true pnpm build
)

test -f "$web_root/dist/index.html"
mkdir -p "$repo_root/public/dist"
find "$repo_root/public/dist" -mindepth 1 ! -name README.md -delete
cp -a "$web_root/dist/." "$repo_root/public/dist/"
test -f "$repo_root/public/dist/index.html"

touch "$repo_root/src/static_files.rs"

echo "Built Rulist frontend into $repo_root/public/dist"
