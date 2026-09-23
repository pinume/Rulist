#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "$0")" && pwd)
bin=${RULIST_BIN:-"$root/target/debug/rulist"}
port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')
base="http://127.0.0.1:$port"
work=$(mktemp -d)
data_dir="$work/data"
files_dir="$work/files"
playwright_root=${RULIST_PLAYWRIGHT_ROOT:-/home/ubuntu/.npm/_npx/e41f203b7505f1fb/node_modules}
chromium_bin=${RULIST_CHROMIUM_BIN:-/home/ubuntu/.cache/ms-playwright/chromium-1243/chrome-linux-arm64/chrome}
mkdir -p "$files_dir"

if [[ ! -d $playwright_root/playwright ]]; then
  playwright_root="$work/tools/node_modules"
  npm install --prefix "$work/tools" --no-save --no-audit --no-fund playwright@1.63.0 >/dev/null
fi
if [[ ! -x $chromium_bin ]]; then
  chromium_bin=$(NODE_PATH="$playwright_root" node -e 'process.stdout.write(require("playwright").chromium.executablePath())')
fi
if [[ ! -x $chromium_bin ]]; then
  "$playwright_root/.bin/playwright" install chromium >/dev/null
  chromium_bin=$(NODE_PATH="$playwright_root" node -e 'process.stdout.write(require("playwright").chromium.executablePath())')
fi
[[ -x $chromium_bin ]]

cleanup() {
  [[ -n ${server_pid:-} ]] && kill "$server_pid" 2>/dev/null || true
  wait "${server_pid:-}" 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT

status() {
  curl -sS -o "$work/response.json" -w '%{http_code}' -H 'content-type: application/json' "$@"
}

expect_status() {
  local expected=$1
  shift
  local actual
  actual=$(status "$@")
  if [[ $actual != "$expected" ]]; then
    printf 'expected HTTP %s, got %s: ' "$expected" "$actual" >&2
    cat "$work/response.json" >&2
    return 1
  fi
}

login() {
  curl -sS -X POST "$base/api/auth/login" -H 'content-type: application/json' \
    --data "$(jq -cn --arg username "$1" --arg password "$2" '{username: $username, password: $password}')" |
    jq -er 'select(.code == 200 and .data.token != null) | .data.token'
}

api_post() {
  curl -sS -X POST "$base$1" -H "authorization: Bearer $2" \
    -H 'content-type: application/json' --data "$3"
}

api_get() {
  curl -sS "$base$1" -H "authorization: Bearer $2"
}

(cd "$root" && ./build-frontend.sh >/dev/null && cargo build --quiet)
"$bin" --data-dir "$data_dir" server --host 127.0.0.1 --port "$port" >"$work/server.log" 2>&1 &
server_pid=$!
for _ in {1..50}; do
  curl -fsS "$base/ping" >/dev/null 2>&1 && break
  sleep 0.1
done
curl -fsS "$base/ping" >/dev/null
admin_help=$("$bin" --data-dir "$data_dir" admin)
[[ $admin_help == *"Show admin token"* && $admin_help != *"random"* && $admin_help != *"cancel2fa"* ]]
[[ $("$bin" user --help) == *"set-password"* && $("$bin" user --help) == *"reset-password"* ]]

empty_token=$(login admin '')
api_get /api/me "$empty_token" | jq -e '.code == 200 and .data.password_unset == true and .data.username == "admin"' >/dev/null
expect_status 401 -X POST -H "authorization: Bearer $empty_token" --data '{"path":"/"}' "$base/api/fs/list"
api_post /api/me/update "$empty_token" '{"username":"admin","password":"E2EInitialPass1!","current_password":""}' | jq -e '.code == 200' >/dev/null
expect_status 401 -H "authorization: Bearer $empty_token" "$base/api/me"

admin_token=$(login admin 'E2EInitialPass1!')
api_get /api/me "$admin_token" | jq -e '.code == 200 and .data.password_unset == false' >/dev/null
expect_status 200 "$base/"
api_post /api/admin/user/create "$admin_token" '{"username":"second-admin","password":"E2EInitialPass1!","role":2}' | jq -e '.code == 400' >/dev/null
admin_id=$(api_get /api/me "$admin_token" | jq -er '.data.id')
api_post /api/admin/user/update "$admin_token" "$(jq -cn --argjson id "$admin_id" '{id: $id, username: "renamed", disabled: false}')" | jq -e '.code == 400' >/dev/null
api_post /api/admin/user/update "$admin_token" "$(jq -cn --argjson id "$admin_id" '{id: $id, username: "admin", disabled: true}')" | jq -e '.code == 400' >/dev/null
expect_status 400 -X POST -H "authorization: Bearer $admin_token" "$base/api/admin/user/delete?id=$admin_id"

member_body=$(jq -cn --arg path "$files_dir" '{username: "member", password: "E2EMemberPass1!", role: 0, local_path: $path}')
api_post /api/admin/user/create "$admin_token" "$member_body" | jq -e '.code == 200' >/dev/null
member_token=$(login member 'E2EMemberPass1!')
printf '%s\n%s\n' 'E2ECliPass123!' 'E2ECliPass123!' | script -qefc "\"$bin\" --data-dir \"$data_dir\" user set-password member" /dev/null >/dev/null
member_cli_token=$(login member 'E2ECliPass123!')
expect_status 200 -H "authorization: Bearer $member_cli_token" "$base/api/me"
sqlite3 "$data_dir/data.db" "UPDATE x_users SET otp_secret = 'TESTSECRET' WHERE username = 'member'; INSERT INTO x_otp_pending (user_id, secret, expires_at) SELECT id, 'TESTPENDING', 9999999999 FROM x_users WHERE username = 'member';"
"$bin" --data-dir "$data_dir" user reset-password member >/dev/null
expect_status 401 -H "authorization: Bearer $member_token" "$base/api/me"
[[ $(sqlite3 "$data_dir/data.db" "SELECT (SELECT COUNT(*) FROM x_otp_pending WHERE user_id = (SELECT id FROM x_users WHERE username = 'member')) || '|' || (SELECT length(otp_secret) FROM x_users WHERE username = 'member');") == '0|0' ]]
member_id=$(sqlite3 "$data_dir/data.db" "SELECT id FROM x_users WHERE username = 'member';")
api_post /api/admin/user/update "$admin_token" "$(jq -cn --argjson id "$member_id" --arg path "$files_dir" '{id: $id, username: "member", password: "E2ERestoredPass1!", disabled: false, local_path: $path}')" | jq -e '.code == 200' >/dev/null
member_restored_token=$(login member 'E2ERestoredPass1!')
expect_status 200 -X POST -H "authorization: Bearer $member_restored_token" --data '{"path":"/"}' "$base/api/fs/list"

old_admin_token=$("$bin" --data-dir "$data_dir" admin token | sed 's/^Admin token: //')
"$bin" --data-dir "$data_dir" user reset-password admin >/dev/null
new_admin_token=$("$bin" --data-dir "$data_dir" admin token | sed 's/^Admin token: //')
[[ $old_admin_token != "$new_admin_token" ]]
reset_token=$(login admin '')
expect_status 401 -X POST -H "authorization: Bearer $reset_token" --data '{"path":"/"}' "$base/api/fs/list"

NODE_PATH="$playwright_root" RULIST_E2E_URL="$base" RULIST_CHROMIUM_BIN="$chromium_bin" node <<'NODE' >"$work/browser.json"
const assert = require("node:assert/strict")
const crypto = require("node:crypto")
const { chromium } = require("playwright")

const codeFor = (secret) => {
  const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"
  let bits = ""
  for (const char of secret.replace(/=|\s|-/g, "")) bits += alphabet.indexOf(char).toString(2).padStart(5, "0")
  const bytes = Buffer.from(bits.match(/.{8}/g).map((byte) => parseInt(byte, 2)))
  const counter = Buffer.alloc(8)
  counter.writeBigUInt64BE(BigInt(Math.floor(Date.now() / 30000)))
  const digest = crypto.createHmac("sha1", bytes).update(counter).digest()
  const offset = digest[19] & 15
  return String((((digest[offset] & 127) << 24) | (digest[offset + 1] << 16) | (digest[offset + 2] << 8) | digest[offset + 3]) % 1000000).padStart(6, "0")
}

async function main() {
  const base = process.env.RULIST_E2E_URL
  const password = "BrowserPass123!"
  const browser = await chromium.launch({ executablePath: process.env.RULIST_CHROMIUM_BIN, headless: true, args: ["--no-sandbox"] })
  const page = await browser.newPage()
  const errors = []
  page.on("pageerror", (error) => errors.push(error.message))
  try {
    await page.goto(base + "/@login")
    await page.locator("#username").fill("admin")
    await page.locator("#password").fill("")
    await page.getByRole("button", { name: "登录" }).click()
    await page.waitForURL(/\/@settings\/profile/)
    await page.getByRole("button", { name: "界面偏好" }).click()
    await page.waitForURL(/\/@settings\/profile/)
    assert.equal(await page.locator("#current-password").count(), 0)
    assert.equal(await page.locator("#username").isDisabled(), true)
    await page.locator("#password").fill(password)
    await page.locator("#confirm-password").fill(password)
    await page.getByRole("button", { name: "保存" }).click()
    await page.waitForURL(/\/@login/)

    await page.locator("#username").fill("admin")
    await page.locator("#password").fill(password)
    await page.getByRole("button", { name: "登录" }).click()
    await page.waitForFunction(() => !location.pathname.startsWith("/@login"))
    await page.goto(base + "/@settings/profile")
    await page.getByRole("button", { name: "启用双因素身份验证" }).waitFor()
    await page.getByRole("button", { name: "启用双因素身份验证" }).click()
    await page.waitForURL(/\/@settings\/2fa/)
    await page.locator("input[type=password]").fill(password)
    await page.getByRole("button", { name: "确认" }).click()
    await page.waitForFunction(() => /[A-Z2-7]{32}/.test(document.body.innerText))
    const secret = (await page.locator("body").innerText()).match(/[A-Z2-7]{32}/)?.[0]
    assert.ok(secret, "2FA secret is visible")
    await page.locator("input").last().fill(codeFor(secret))
    await page.getByRole("button", { name: "验证" }).click()
    await page.waitForURL(/\/@settings\/profile/)
    await page.getByRole("button", { name: "取消双因素身份验证" }).waitFor()
    await page.getByRole("button", { name: "取消双因素身份验证" }).click()
    await page.locator("input[placeholder='输入您的身份验证器应用中显示的验证码']").waitFor()
    await page.locator("input[placeholder='输入您的身份验证器应用中显示的验证码']").fill(codeFor(secret))
    await page.getByRole("button", { name: "确认" }).click()
    await page.getByRole("button", { name: "启用双因素身份验证" }).waitFor()
    assert.equal(await page.getByRole("button", { name: "取消双因素身份验证" }).count(), 0)
    assert.deepEqual(errors, [])
    process.stdout.write(JSON.stringify({ empty_password_redirect: true, empty_password_navigation_guard: true, password_set_then_relogin: true, two_factor_enabled: true, two_factor_disabled: true, page_errors: 0 }))
  } finally {
    await browser.close()
  }
}

main().catch((error) => { console.error(error); process.exitCode = 1 })
NODE

[[ $(sqlite3 "$data_dir/data.db" "SELECT (SELECT COUNT(*) FROM x_otp_pending WHERE user_id = (SELECT id FROM x_users WHERE username = 'admin')) || '|' || (SELECT length(otp_secret) FROM x_users WHERE username = 'admin');") == '0|0' ]]

jq -n --arg timestamp "$(date -u +%FT%TZ)" --slurpfile browser "$work/browser.json" '{timestamp: $timestamp, checks: ["admin CLI help contains only the token command", "CLI set-password reads and confirms a hidden password", "empty-password login is restricted to profile endpoints", "password update invalidates prior JWT", "admin create/rename/disable/delete are rejected", "user reset invalidates JWT and clears 2FA state", "administrator password set clears reset restriction", "admin reset rotates the admin token", "browser: empty-password login redirects to profile and cannot navigate away", "browser: password set then re-login", "browser: 2FA enable/disable updates profile state and clears pending state"], browser: $browser[0]}' >"$root/verification-admin-flow.json"
