#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")" && pwd)"
work_dir="$(mktemp -d /tmp/rulist-folder-e2e.XXXXXX)"
server_pid=""
cleanup() {
  if [[ -n "$server_pid" ]]; then
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
  rm -rf "$work_dir"
}
trap cleanup EXIT

e2e_password="$(python3 -c 'import secrets; print(secrets.token_hex(16))')"
port="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')"

python3 - "$work_dir" <<'PY'
from pathlib import Path
import sys

root = Path(sys.argv[1]) / "storage"
alpha = root / "alpha"
beta = root / "beta"
alpha.mkdir(parents=True)
beta.mkdir()
for n in range(105):
    (alpha / f"file{n:03d}.txt").write_bytes(b"x" * (106 - n))
for name, size in (("alpha.txt", 30), ("beta.txt", 10), ("gamma.txt", 20)):
    (beta / name).write_bytes(b"y" * size)
PY

cd "$repo_root"
./build-frontend.sh >/dev/null
cargo build --quiet
RULIST_ADMIN_PASSWORD="$e2e_password" target/debug/rulist --data-dir "$work_dir/data" admin >/dev/null
python3 - "$work_dir" <<'PY'
from pathlib import Path
import json
import sqlite3
import sys

root = Path(sys.argv[1])
with sqlite3.connect(root / "data" / "data.db") as db:
    db.execute(
        "INSERT INTO x_storages (mount_path, driver, addition) VALUES (?, ?, ?)",
        ("/data", "Local", json.dumps({"root_folder_path": str(root / "storage")})),
    )
PY

target/debug/rulist --data-dir "$work_dir/data" server --port "$port" --host 127.0.0.1 >"$work_dir/server.log" 2>&1 &
server_pid="$!"
for _ in {1..40}; do
  if curl --silent --fail "http://127.0.0.1:$port/ping" >/dev/null; then
    break
  fi
  sleep 0.25
done
curl --silent --fail "http://127.0.0.1:$port/ping" >/dev/null

npm install --prefix "$work_dir/tools" --no-save --no-audit --no-fund playwright@1.63.0 >/dev/null
"$work_dir/tools/node_modules/.bin/playwright" install chromium >/dev/null
RULIST_E2E_URL="http://127.0.0.1:$port" \
RULIST_E2E_PASSWORD="$e2e_password" \
RULIST_REPO_ROOT="$repo_root" \
NODE_PATH="$work_dir/tools/node_modules" \
node <<'NODE' >"$work_dir/report.json"
const assert = require("node:assert/strict")
const fs = require("node:fs")
const path = require("node:path")
const { chromium } = require("playwright")

async function main() {
  const base = process.env.RULIST_E2E_URL
  const html = fs.readFileSync(path.join(process.env.RULIST_REPO_ROOT, "public/dist/index.html"), "utf8")
  const assets = [...new Set(html.match(/\/assets\/[^"\s]+/g) ?? [])]
  assert.ok(assets.length > 0)
  for (const asset of assets) {
    assert.ok(fs.existsSync(path.join(process.env.RULIST_REPO_ROOT, "public/dist", asset)), asset)
  }

  const browser = await chromium.launch({ headless: true, args: ["--no-sandbox"] })
  const page = await browser.newPage()
  const requests = []
  const errors = []
  page.on("request", (request) => {
    if (request.url().endsWith("/api/fs/list")) requests.push(request.postDataJSON())
  })
  page.on("pageerror", (error) => errors.push(error.message))
  try {
    await page.goto(base + "/@login")
    await page.locator("#username").fill("admin")
    await page.locator("#password").fill(process.env.RULIST_E2E_PASSWORD)
    await page.getByRole("button", { name: "登录" }).click()
    await page.waitForURL(base + "/")

    await page.goto(base + "/data/alpha")
    await page.getByText("本页 100 个文件").waitFor()
    assert.match(await page.locator("body").innerText(), /共 105 项/)
    await page.getByText("大小", { exact: true }).first().click()
    await page.getByText("file104.txt").waitFor()
    assert.equal(
      (await page.evaluate(() => localStorage.getItem("dir_sort_/data/alpha"))).includes('"orderBy":"size"'),
      true,
    )

    requests.length = 0
    await page.reload()
    await page.getByText("本页 100 个文件").waitFor()
    await page.waitForTimeout(1400)
    const alphaRequests = requests.filter((request) => request.path === "/data/alpha")
    assert.equal(alphaRequests.length, 1, "saved sort must load exactly once")
    assert.equal(alphaRequests[0].order_by, "size")
    assert.equal(alphaRequests[0].reverse, false)
    assert.match(await page.locator("body").innerText(), /file104.txt/)

    await page.getByRole("button", { name: "下一页" }).click()
    await page.getByText("本页 5 个文件").waitFor()
    assert.match(await page.locator("body").innerText(), /共 105 项/)

    requests.length = 0
    await page.locator('a[href="/data"]').first().click()
    await page.locator('a[href="/data/beta"]').first().click()
    await page.getByText("本页 3 个文件").waitFor()
    const betaRequests = requests.filter((request) => request.path === "/data/beta")
    assert.equal(betaRequests.length, 1)
    assert.equal(betaRequests[0].order_by, "name")
    assert.equal(betaRequests[0].reverse, false)
    const names = await page.locator(".list .list-item").allTextContents()
    assert.equal(names.length, 3)
    for (const [index, name] of ["alpha.txt", "beta.txt", "gamma.txt"].entries()) {
      assert.ok(names[index].includes(name))
    }

    await page.evaluate(() => localStorage.setItem("dir_sort_/data/beta", "{bad"))
    requests.length = 0
    await page.reload()
    await page.getByText("本页 3 个文件").waitFor()
    const fallback = requests.filter((request) => request.path === "/data/beta")
    assert.equal(fallback.length, 1)
    assert.equal(fallback[0].order_by, "name")
    assert.deepEqual(errors, [])

    process.stdout.write(JSON.stringify({
      artifact: "rulist-folder-pagination-e2e",
      reproduce: "./verify-folder-pagination-e2e.sh",
      result: "pass",
      fixture: { alpha_files: 105, beta_files: 3 },
      checks: {
        saved_sort_requests: alphaRequests.length,
        saved_sort_order: alphaRequests[0].order_by,
        first_page_items: 100,
        second_page_items: 5,
        total_items: 105,
        spa_next_directory_requests: betaRequests.length,
        spa_next_directory_order: betaRequests[0].order_by,
        malformed_sort_fallback: fallback[0].order_by,
        page_errors: errors.length,
        index_assets_present: assets.length,
      },
    }, null, 2) + "\n")
  } finally {
    await browser.close()
  }
}

main().catch((error) => { console.error(error); process.exitCode = 1 })
NODE
mv "$work_dir/report.json" "$repo_root/verification-folder-pagination.json"
cat "$repo_root/verification-folder-pagination.json"
