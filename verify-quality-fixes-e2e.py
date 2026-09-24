#!/usr/bin/env python3
"""Run the quality-fix acceptance flow against a temporary Rulist instance."""

import json
import os
import secrets
import socket
import sqlite3
import stat
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path


ROOT = Path(__file__).resolve().parent
BIN = ROOT / "target/debug/rulist"
ARTIFACT = ROOT / "verification-quality-fixes.json"


def api(base, path, token=None, payload=None):
    headers = {"content-type": "application/json"}
    if token:
        headers["authorization"] = f"Bearer {token}"
    request = urllib.request.Request(
        base + path,
        data=json.dumps(payload).encode() if payload is not None else None,
        headers=headers,
    )
    try:
        with urllib.request.urlopen(request, timeout=5) as response:
            return response.status, json.load(response)
    except urllib.error.HTTPError as error:
        return error.code, json.load(error)


def check(condition, message):
    if not condition:
        raise AssertionError(message)


with tempfile.TemporaryDirectory(prefix="rulist-quality-e2e-") as temp:
    work = Path(temp)
    data = work / "data"
    data.mkdir(mode=0o755)
    os.chmod(data, 0o755)
    files = work / "files"
    for name in ("current", "source", "dest"):
        (files / name).mkdir(parents=True)

    subprocess.run(
        [BIN, "--data-dir", data, "admin", "token"],
        check=True,
        stdout=subprocess.DEVNULL,
    )
    with sqlite3.connect(data / "data.db") as db:
        db.execute(
            "INSERT INTO x_storages (mount_path, driver, addition) VALUES (?, ?, ?)",
            ("/files", "Local", json.dumps({"root_folder_path": str(files)})),
        )
        token = db.execute(
            "SELECT value FROM x_setting_items WHERE key = 'token'"
        ).fetchone()[0]

    # Existing installations are tightened at startup too.
    for path, mode in (
        (data, 0o755),
        (data / "config.json", 0o644),
        (data / "data.db", 0o644),
    ):
        os.chmod(path, mode)
    subprocess.run(
        [BIN, "--data-dir", data, "admin", "token"],
        check=True,
        stdout=subprocess.DEVNULL,
    )
    modes = {
        name: stat.S_IMODE((data / name).stat().st_mode)
        for name in ("config.json", "data.db")
    }
    modes["data"] = stat.S_IMODE(data.stat().st_mode)
    check(
        modes == {"data": 0o700, "config.json": 0o600, "data.db": 0o600},
        f"unsafe modes: {modes}",
    )

    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    base = f"http://127.0.0.1:{port}"
    server = subprocess.Popen(
        [BIN, "--data-dir", data, "server", "--host", "127.0.0.1", "--port", str(port)],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    try:
        for _ in range(100):
            try:
                with urllib.request.urlopen(base + "/ping", timeout=1) as response:
                    if response.read() == b"pong":
                        break
            except (urllib.error.URLError, TimeoutError):
                time.sleep(0.1)
        else:
            raise AssertionError("server did not start")

        blank = {"username": "  ", "password": "", "directory_path": "/files/current"}
        status, response = api(base, "/api/admin/user/create", token, blank)
        check(
            status == 400 and response["code"] == 400,
            "blank username was accepted on create",
        )

        status, response = api(
            base, "/api/admin/user/create", token, {**blank, "username": "member"}
        )
        check(status == 200 and response["code"] == 200, "valid user creation failed")
        with sqlite3.connect(data / "data.db") as db:
            user_id = db.execute("SELECT id FROM x_users WHERE username = 'member'").fetchone()[0]
        status, response = api(base, f"/api/admin/user/get?id={user_id}", token)
        check(
            status == 200 and response["data"]["directory_path"] == "/files/current",
            "user directory path did not round-trip",
        )
        status, response = api(
            base, "/api/admin/user/update", token, {"id": user_id, "username": "\t"}
        )
        check(
            status == 400 and response["code"] == 400,
            "blank username was accepted on update",
        )

        source, dest = files / "source", files / "dest"
        (source / "move.txt").write_text("move")
        status, response = api(base, "/api/fs/move", token, {
            "src_dir": "/files/source", "dst_dir": "/files/dest",
            "names": ["move.txt", "missing.txt"], "conflict_policy": "cancel",
        })
        check(
            status == 500 and "1 item(s) already moved" in response["message"]
            and (dest / "move.txt").exists(),
            "partial move was not reported",
        )

        (source / "copy.txt").write_text("copy")
        status, response = api(base, "/api/fs/copy", token, {
            "src_dir": "/files/source", "dst_dir": "/files/dest",
            "names": ["copy.txt", "missing.txt"], "conflict_policy": "cancel",
        })
        check(
            status == 500 and "1 item(s) already copied" in response["message"]
            and (dest / "copy.txt").exists(),
            "partial copy was not reported",
        )

        (source / "delete.txt").write_text("delete")
        status, response = api(base, "/api/fs/remove", token, {
            "dir": "/files/source", "names": ["delete.txt", "missing.txt"],
        })
        check(
            status == 500 and "1 item(s) already deleted" in response["message"]
            and not (source / "delete.txt").exists(),
            "partial delete was not reported",
        )

        password = secrets.token_urlsafe(18)
        status, response = api(base, "/api/admin/user/update", token, {
            "id": 1, "username": "admin", "password": password,
        })
        check(status == 200 and response["code"] == 200, "temporary admin password setup failed")
        browser_script = r'''
const assert = require("node:assert/strict")
const { chromium } = require("playwright")
async function main() {
  const browser = await chromium.launch({ executablePath: process.env.RULIST_CHROMIUM_BIN, headless: true, args: ["--no-sandbox"] })
  const page = await browser.newPage()
  const errors = []
  page.on("pageerror", (error) => errors.push(error.message))
  try {
    await page.goto(process.env.RULIST_E2E_URL + "/@login")
    await page.locator("#username").fill("admin")
    await page.locator("#password").fill(process.env.RULIST_E2E_PASSWORD)
    await page.getByRole("button", { name: "登录" }).click()
    await page.waitForFunction(() => !location.pathname.startsWith("/@login"))
    await page.goto(process.env.RULIST_E2E_URL + "/@settings/users/edit/" + process.env.RULIST_E2E_USER_ID)
    await page.locator("#directory_path").waitFor()
    await page.locator("#directory_path").locator("..").getByRole("button").click()
    await page.getByRole("dialog").getByRole("button", { name: "确定" }).click()
    assert.equal(await page.locator("#directory_path").inputValue(), "/files/current")
    assert.deepEqual(errors, [])
    process.stdout.write(JSON.stringify({ picker_preserved_current_directory: true, page_errors: 0 }))
  } finally {
    await browser.close()
  }
}
main().catch((error) => { console.error(error); process.exitCode = 1 })
'''
        env = os.environ.copy()
        env.update({
            "RULIST_E2E_URL": base,
            "RULIST_E2E_PASSWORD": password,
            "RULIST_E2E_USER_ID": str(user_id),
            "NODE_PATH": env.get(
                "RULIST_PLAYWRIGHT_ROOT",
                "/home/ubuntu/.npm/_npx/e41f203b7505f1fb/node_modules",
            ),
            "RULIST_CHROMIUM_BIN": env.get(
                "RULIST_CHROMIUM_BIN",
                "/home/ubuntu/.cache/ms-playwright/chromium-1243/chrome-linux-arm64/chrome",
            ),
        })
        browser = subprocess.run(
            ["node", "-e", browser_script],
            env=env, text=True, capture_output=True, timeout=60,
        )
        check(browser.returncode == 0, f"browser verification failed: {browser.stderr}")
        browser_result = json.loads(browser.stdout)

        report = {
            "result": "pass",
            "reproduce": "Build frontend and Rust binary, then run python3 verify-quality-fixes-e2e.py",
            "data": "temporary SQLite and Local storage",
            "permissions_octal": {key: oct(value) for key, value in modes.items()},
            "checks": {
                "blank_username_create_rejected": True,
                "blank_username_update_rejected": True,
                "directory_path_round_trip": True,
                "partial_move_reported": True,
                "partial_copy_reported": True,
                "partial_delete_reported": True,
                **browser_result,
            },
        }
        ARTIFACT.write_text(json.dumps(report, indent=2) + "\n")
        print(ARTIFACT.read_text(), end="")
    finally:
        server.terminate()
        try:
            server.wait(timeout=5)
        except subprocess.TimeoutExpired:
            server.kill()
            server.wait()
