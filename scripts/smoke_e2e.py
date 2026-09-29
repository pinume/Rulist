#!/usr/bin/env python3
"""Run a self-contained HTTP smoke test against a built Rulist binary."""

import json
import os
import re
import socket
import sqlite3
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.parse import parse_qs, quote, urlsplit
from urllib.request import Request, urlopen


ROOT = Path(__file__).resolve().parents[1]
BINARY = Path(sys.argv[1]) if len(sys.argv) == 2 else ROOT / "target" / "debug" / "rulist"
CONTENT = b"Rulist E2E smoke test\n"


def pick_port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


def http(url, method="GET", body=None, headers=None):
    request = Request(url, data=body, method=method, headers=headers or {})
    try:
        with urlopen(request, timeout=10) as response:
            return response.status, response.read()
    except HTTPError as error:
        return error.code, error.read()


def api(base, path, payload=None, token=None):
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    body = None if payload is None else json.dumps(payload).encode()
    status, raw = http(f"{base}{path}", "POST", body, headers)
    return status, json.loads(raw)


def expect(status, actual, label):
    if actual != status:
        raise AssertionError(f"{label}: expected HTTP {status}, got {actual}")


def main():
    if not BINARY.is_file():
        raise SystemExit(f"Rulist binary not found: {BINARY}; run cargo build --locked first")

    with tempfile.TemporaryDirectory(prefix="rulist-smoke-") as tmp:
        tmp_path = Path(tmp)
        data_dir = tmp_path / "data"
        storage_dir = tmp_path / "home"
        storage_dir.mkdir()
        env = os.environ.copy()
        env["HOME"] = str(storage_dir.resolve())

        init_output = subprocess.run(
            [str(BINARY), "--data-dir", str(data_dir), "interactive"],
            input="0\n",
            check=True,
            text=True,
            capture_output=True,
            env=env,
        ).stdout
        match = re.search(r"Password: (\S+)", init_output)
        if not match:
            raise AssertionError("interactive initialization did not print the admin password")
        initial_password = match.group(1)

        with sqlite3.connect(data_dir / "data.db") as connection:
            tables = {
                row[0]
                for row in connection.execute(
                    "SELECT name FROM sqlite_master WHERE type = 'table'"
                )
                if not row[0].startswith("sqlite_")
            }
            if tables != {"users", "login_attempts", "revoked_tokens"}:
                raise AssertionError(f"unexpected fresh database tables: {tables}")

        port = pick_port()
        base = f"http://127.0.0.1:{port}"
        log_path = tmp_path / "server.log"
        with log_path.open("w") as log:
            server = subprocess.Popen(
                [str(BINARY), "--data-dir", str(data_dir), "server", "--host", "127.0.0.1", "--port", str(port)],
                stdout=log,
                stderr=subprocess.STDOUT,
                env=env,
            )
            try:
                for _ in range(50):
                    if server.poll() is not None:
                        raise AssertionError(f"server exited early:\n{log_path.read_text()}")
                    try:
                        if http(f"{base}/ping")[0] == 200:
                            break
                    except URLError:
                        time.sleep(0.1)
                else:
                    raise AssertionError(f"server did not become ready:\n{log_path.read_text()}")

                status, raw_settings = http(f"{base}/api/public/settings")
                expect(200, status, "public settings")
                settings = json.loads(raw_settings)["data"]
                if (
                    settings["site_title"] != "Rulist"
                    or settings["package_download"] is not True
                    or not isinstance(settings["hide_files"], list)
                ):
                    raise AssertionError(f"unexpected public settings: {settings}")

                status, _ = http(
                    f"{base}/api/fs/put",
                    "PUT",
                    b"unauthorized",
                    {"File-Path": quote("/unauthorized.txt", safe="")},
                )
                expect(401, status, "unauthorized upload")

                status, login = api(base, "/api/auth/login", {"username": "admin", "password": initial_password})
                expect(200, status, "initial password login")
                token = login["data"]["token"]
                status, raw_me = http(
                    f"{base}/api/me", headers={"Authorization": f"Bearer {token}"}
                )
                expect(200, status, "admin current user")
                admin = json.loads(raw_me)["data"]
                if set(admin) != {"id", "username", "role", "permission", "otp"}:
                    raise AssertionError(f"/api/me exposed fields outside SessionUser: {admin}")

                upload_headers = {
                    "Authorization": f"Bearer {token}",
                    "File-Path": quote("/smoke.txt", safe=""),
                    "Content-Type": "text/plain",
                    "Overwrite": "false",
                }
                status, upload = http(f"{base}/api/fs/put", "PUT", CONTENT, upload_headers)
                expect(200, status, "upload")
                if json.loads(upload)["code"] != 200:
                    raise AssertionError("upload API returned an error")

                status, listing = api(base, "/api/fs/list", {"path": "/"}, token)
                expect(200, status, "list")
                if "smoke.txt" not in [item["name"] for item in listing["data"]["content"]]:
                    raise AssertionError(f"uploaded file is absent from list: {listing}")

                status, preview = api(base, "/api/fs/preview", {"path": "/smoke.txt"}, token)
                expect(200, status, "preview")
                if preview["data"]["content"]["value"].encode() != CONTENT:
                    raise AssertionError(f"unexpected preview content: {preview}")

                status, link = api(base, "/api/fs/link", {"path": "/smoke.txt"}, token)
                expect(200, status, "signed link")
                query = parse_qs(urlsplit(link["data"]["url"]).query)
                if query.get("uid") != [str(admin["id"])] or "sign" not in query:
                    raise AssertionError(f"signed link lacks its user identity: {link}")
                if str(storage_dir) in link["data"]["url"]:
                    raise AssertionError("signed link exposed the server filesystem path")
                status, downloaded = http(f"{base}{link['data']['url']}")
                expect(200, status, "signed download")
                if downloaded != CONTENT:
                    raise AssertionError("signed download bytes differ from uploaded bytes")

                status, _ = api(base, "/api/fs/list", {"path": "/../smoke.txt"}, token)
                expect(403, status, "root traversal rejection")

                unsigned_path = urlsplit(link["data"]["url"]).path
                status, _ = http(f"{base}{unsigned_path}")
                expect(401, status, "unsigned download")
                status, downloaded = http(
                    f"{base}{unsigned_path}", headers={"Authorization": f"Bearer {token}"}
                )
                expect(200, status, "authenticated unsigned download")
                if downloaded != CONTENT:
                    raise AssertionError("authenticated unsigned download bytes differ from uploaded bytes")

                status, _ = http(f"{base}/api/fs/put", "PUT", b"replacement", upload_headers)
                expect(409, status, "duplicate upload")
                if (storage_dir / "smoke.txt").read_bytes() != CONTENT:
                    raise AssertionError("duplicate upload changed the original file")

                status, _ = http(f"{base}/api/auth/logout", headers={"Authorization": f"Bearer {token}"})
                expect(200, status, "logout")
                status, _ = api(base, "/api/fs/list", {"path": "/"}, token)
                expect(401, status, "revoked JWT")

                status, login = api(base, "/api/auth/login", {"username": "admin", "password": initial_password})
                expect(200, status, "fresh login after logout")
                token = login["data"]["token"]

                alternate_root = tmp_path / "alternate-root"
                alternate_root.mkdir()
                with sqlite3.connect(data_dir / "data.db") as connection:
                    connection.execute(
                        "UPDATE users SET local_path = ? WHERE id = ?",
                        (str(alternate_root.resolve()), admin["id"]),
                    )
                status, _ = http(f"{base}{link['data']['url']}")
                expect(403, status, "old signed link after root change")
                with sqlite3.connect(data_dir / "data.db") as connection:
                    connection.execute(
                        "UPDATE users SET local_path = ? WHERE id = ?",
                        (str(storage_dir.resolve()), admin["id"]),
                    )

                status, link = api(base, "/api/fs/link", {"path": "/smoke.txt"}, token)
                expect(200, status, "signed link before password timestamp change")
                with sqlite3.connect(data_dir / "data.db") as connection:
                    connection.execute("UPDATE users SET pwd_ts = pwd_ts + 1 WHERE id = ?", (admin["id"],))
                status, _ = http(f"{base}{link['data']['url']}")
                expect(403, status, "old signed link after password timestamp change")

                status, login = api(base, "/api/auth/login", {"username": "admin", "password": initial_password})
                expect(200, status, "fresh login after password timestamp change")
                token = login["data"]["token"]
                status, link = api(base, "/api/fs/link", {"path": "/smoke.txt"}, token)
                expect(200, status, "signed link before disabling user")
                with sqlite3.connect(data_dir / "data.db") as connection:
                    connection.execute("UPDATE users SET disabled = 1 WHERE id = ?", (admin["id"],))
                status, _ = http(f"{base}{link['data']['url']}")
                expect(403, status, "signed link after user disabled")
            finally:
                server.terminate()
                try:
                    server.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    server.kill()
                    server.wait()

    print("Rulist E2E smoke test passed")


if __name__ == "__main__":
    main()
