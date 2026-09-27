#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
web_root="$repo_root/web"

(
  cd "$web_root"
  CI=true pnpm install --frozen-lockfile
  CI=true pnpm build
)

mkdir -p "$repo_root/public/dist"
find "$repo_root/public/dist" -mindepth 1 ! -name README.md -delete
cp -a "$web_root/dist/." "$repo_root/public/dist/"

echo "Built Rulist frontend into $repo_root/public/dist"
