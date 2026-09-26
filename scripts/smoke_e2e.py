#!/usr/bin/env python3
"""Run a self-contained HTTP smoke test against a built Rulist binary."""

import json
import socket
import sqlite3
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.parse import quote
from urllib.request import Request, urlopen


ROOT = Path(__file__).resolve().parents[1]
BINARY = Path(sys.argv[1]) if len(sys.argv) == 2 else ROOT / "target" / "debug" / "rulist"
CONTENT = b"Rulist E2E smoke test\n"
PASSWORD = "smoke-pass-123"


def pick_port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


def http(url, method="GET", body=None, headers=None):
    request = Request(url, data=body, method=method, headers=headers or {})
    try:
        with urlopen(request, timeout=2) as response:
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
        storage_dir = tmp_path / "storage"
        storage_dir.mkdir()

        token_command = [str(BINARY), "--data-dir", str(data_dir), "admin", "token"]
        token_output = subprocess.run(token_command, check=True, text=True, capture_output=True).stdout
        if not any(line.startswith("Admin token: ") for line in token_output.splitlines()):
            raise AssertionError("admin token command did not initialize the database")

        with sqlite3.connect(data_dir / "data.db") as connection:
            connection.execute(
                "INSERT INTO x_storages (mount_path, driver, addition) VALUES (?, ?, ?)",
                ("/", "Local", json.dumps({"root_folder_path": str(storage_dir)})),
            )

        port = pick_port()
        base = f"http://127.0.0.1:{port}"
        log_path = tmp_path / "server.log"
        with log_path.open("w") as log:
            server = subprocess.Popen(
                [str(BINARY), "--data-dir", str(data_dir), "server", "--host", "127.0.0.1", "--port", str(port)],
                stdout=log,
                stderr=subprocess.STDOUT,
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

                status, _ = http(
                    f"{base}/api/fs/put",
                    "PUT",
                    b"unauthorized",
                    {"File-Path": quote("/unauthorized.txt", safe="")},
                )
                expect(401, status, "unauthorized upload")

                status, login = api(base, "/api/auth/login", {"username": "admin", "password": ""})
                expect(200, status, "initial empty-password login")
                setup_token = login["data"]["token"]

                status, update = api(base, "/api/me/update", {"password": PASSWORD}, setup_token)
                expect(200, status, "set password")
                if update["code"] != 200:
                    raise AssertionError(f"set password API error: {update}")

                status, login = api(base, "/api/auth/login", {"username": "admin", "password": PASSWORD})
                expect(200, status, "password login")
                token = login["data"]["token"]

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
                status, downloaded = http(f"{base}{link['data']['url']}")
                expect(200, status, "signed download")
                if downloaded != CONTENT:
                    raise AssertionError("signed download bytes differ from uploaded bytes")

                status, _ = http(f"{base}/api/fs/put", "PUT", b"replacement", upload_headers)
                expect(409, status, "duplicate upload")
                if (storage_dir / "smoke.txt").read_bytes() != CONTENT:
                    raise AssertionError("duplicate upload changed the original file")
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
