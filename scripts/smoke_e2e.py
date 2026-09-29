#!/usr/bin/env python3
"""Run a self-contained HTTP smoke test against a built Rulist binary."""

import base64
import hashlib
import hmac
import json
import os
import re
import socket
import sqlite3
import subprocess
import sys
import tempfile
import time
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

        no_home_env = env.copy()
        no_home_env.pop("HOME", None)
        no_home_data = tmp_path / "no-home-data"
        no_home_init = subprocess.run(
            [str(BINARY), "--data-dir", str(no_home_data), "interactive"],
            input="0\n",
            text=True,
            capture_output=True,
            env=no_home_env,
        )
        if no_home_init.returncode == 0 or "HOME is not set" not in no_home_init.stderr:
            raise AssertionError(
                "fresh initialization without HOME should fail clearly: "
                f"stdout={no_home_init.stdout!r} stderr={no_home_init.stderr!r}"
            )

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

        guest_root = tmp_path / "guest-root"
        guest_root.mkdir()
        (storage_dir / "admin-only.txt").write_text("admin\n")
        for folder in ("copy-src", "copy-dst", "move-src", "move-dst"):
            (storage_dir / folder).mkdir()
        (storage_dir / "copy-src" / "exists.txt").write_text("source copy\n")
        (storage_dir / "copy-dst" / "exists.txt").write_text("destination copy\n")
        (storage_dir / "move-src" / "exists.txt").write_text("source move\n")
        (storage_dir / "move-dst" / "exists.txt").write_text("destination move\n")
        (guest_root / "guest-only.txt").write_text("guest\n")
        (guest_root / "signed.txt").write_text("signed\n")
        (guest_root / "copy-source.txt").write_text("copy without write permission\n")
        (guest_root / "copy-target").mkdir()
        subprocess.run(
            [str(BINARY), "--data-dir", str(data_dir), "interactive"],
            input=(
                f"2\nguest\n{guest_root}\n0\n\nGuestPass123!\n"
                "GuestPass123!\ny\n0\n"
            ),
            check=True,
            text=True,
            capture_output=True,
            env=env,
        )
        with sqlite3.connect(data_dir / "data.db") as connection:
            guest = connection.execute(
                "SELECT id, local_path FROM users WHERE username = 'guest'"
            ).fetchone()
            if guest is None or guest[1] != str(guest_root.resolve()):
                raise AssertionError(f"interactive CLI did not create guest root: {guest}")
            guest_id = guest[0]
            connection.execute(
                "UPDATE users SET permission = ? WHERE id = ?",
                (1 << 6, guest_id),
            )
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
                env=no_home_env,
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

                status, _ = http(f"{base}/api/not-exist")
                expect(404, status, "unknown API route")
                status, page = http(f"{base}/unknown-page")
                expect(200, status, "SPA route")
                if b"<html" not in page.lower():
                    raise AssertionError("unknown frontend route did not return HTML")

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
                admin_names = {item["name"] for item in listing["data"]["content"]}
                if "admin-only.txt" not in admin_names or "guest-only.txt" in admin_names:
                    raise AssertionError(f"admin listing crossed user roots: {admin_names}")
                status, _ = api(base, "/api/fs/get", {"path": "/missing.txt"}, token)
                expect(404, status, "missing file")
                status, _ = api(base, "/api/fs/mkdir", {"path": ""}, token)
                expect(400, status, "empty mkdir path")

                status, guest_login = api(
                    base,
                    "/api/auth/login",
                    {"username": "guest", "password": "GuestPass123!"},
                )
                expect(200, status, "guest login")
                guest_token = guest_login["data"]["token"]
                status, guest_listing = api(base, "/api/fs/list", {"path": "/"}, guest_token)
                expect(200, status, "guest list")
                guest_names = {item["name"] for item in guest_listing["data"]["content"]}
                if "guest-only.txt" not in guest_names or "admin-only.txt" in guest_names:
                    raise AssertionError(f"guest listing crossed user roots: {guest_names}")
                status, _ = api(
                    base,
                    "/api/fs/copy",
                    {
                        "src_dir": "/",
                        "dst_dir": "/copy-target",
                        "names": ["copy-source.txt"],
                        "conflict_policy": "cancel",
                    },
                    guest_token,
                )
                expect(200, status, "copy without write permission")
                copied_path = guest_root / "copy-target" / "copy-source.txt"
                if copied_path.read_text() != "copy without write permission\n":
                    raise AssertionError("copy without write permission produced wrong contents")
                status, _ = api(
                    base, "/api/fs/mkdir", {"path": "/not-allowed"}, guest_token
                )
                expect(403, status, "mkdir without write permission")
                status, _ = http(
                    f"{base}/api/fs/put",
                    "PUT",
                    b"not allowed",
                    {
                        "Authorization": f"Bearer {guest_token}",
                        "File-Path": quote("/not-allowed.txt", safe=""),
                    },
                )
                expect(403, status, "upload without write permission")
                status, _ = api(
                    base,
                    "/api/fs/list",
                    {"path": "/../admin-only.txt"},
                    guest_token,
                )
                expect(403, status, "guest root traversal rejection")

                status, guest_link = api(
                    base, "/api/fs/link", {"path": "/signed.txt"}, guest_token
                )
                expect(200, status, "guest signed link before disabling user")
                status, signed_bytes = http(f"{base}{guest_link['data']['url']}")
                expect(200, status, "guest signed download before disabling user")
                if signed_bytes != b"signed\n":
                    raise AssertionError("guest signed download bytes differ")

                for choice in ("disable", "enable"):
                    subprocess.run(
                        [str(BINARY), "--data-dir", str(data_dir), "interactive"],
                        input=f"3\n{guest_id}\n5\ny\n0\n0\n",
                        check=True,
                        text=True,
                        capture_output=True,
                        env=env,
                    )
                    if choice == "disable":
                        status, _ = http(
                            f"{base}/api/me",
                            headers={"Authorization": f"Bearer {guest_token}"},
                        )
                        expect(401, status, "JWT after disabling user")
                        status, _ = http(f"{base}{guest_link['data']['url']}")
                        expect(403, status, "signed link after disabling user")
                    else:
                        status, _ = http(
                            f"{base}/api/me",
                            headers={"Authorization": f"Bearer {guest_token}"},
                        )
                        expect(401, status, "old JWT after re-enabling user")
                        status, _ = http(f"{base}{guest_link['data']['url']}")
                        expect(403, status, "old signed link after re-enabling user")

                status, guest_login = api(
                    base,
                    "/api/auth/login",
                    {"username": "guest", "password": "GuestPass123!"},
                )
                expect(200, status, "fresh guest login after re-enabling user")
                guest_token = guest_login["data"]["token"]
                status, _ = http(
                    f"{base}/api/me",
                    headers={"Authorization": f"Bearer {guest_token}"},
                )
                expect(200, status, "fresh guest JWT after re-enabling user")
                status, guest_link = api(
                    base, "/api/fs/link", {"path": "/signed.txt"}, guest_token
                )
                expect(200, status, "fresh guest signed link after re-enabling user")
                status, signed_bytes = http(f"{base}{guest_link['data']['url']}")
                expect(200, status, "fresh guest signed download after re-enabling user")
                if signed_bytes != b"signed\n":
                    raise AssertionError("fresh guest signed download bytes differ")

                status, preview = api(base, "/api/fs/preview", {"path": "/smoke.txt"}, token)
                expect(200, status, "preview")
                if preview["data"]["content"]["value"].encode() != CONTENT:
                    raise AssertionError(f"unexpected preview content: {preview}")

                status, link = api(base, "/api/fs/link", {"path": "/smoke.txt"}, token)
                expect(200, status, "signed link")
                config = json.loads((data_dir / "config.json").read_text())
                with sqlite3.connect(data_dir / "data.db") as connection:
                    user_context = connection.execute(
                        "SELECT pwd_ts, local_path FROM users WHERE id = ?",
                        (admin["id"],),
                    ).fetchone()
                pwd_ts, local_path = user_context
                expires = int(time.time()) - 1
                context = f"uid={admin['id']}:pwd_ts={pwd_ts}:root={local_path}"
                payload = f"/smoke.txt:{context}:{expires}".encode()
                key = hashlib.sha256(
                    f"{config['jwt_secret']}:rulist-link-signer".encode()
                ).digest()
                digest = hmac.new(key, payload, hashlib.sha256).digest()
                expired_sign = (
                    base64.urlsafe_b64encode(digest).rstrip(b"=").decode()
                    + f":{expires}"
                )
                status, _ = http(
                    f"{base}/d/smoke.txt?sign={quote(expired_sign, safe='')}&uid={admin['id']}"
                )
                expect(403, status, "expired signed link")
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

                for route, src_dir, dst_dir in (
                    ("/api/fs/copy", "/copy-src", "/copy-dst"),
                    ("/api/fs/move", "/move-src", "/move-dst"),
                ):
                    status, _ = api(
                        base,
                        route,
                        {
                            "src_dir": src_dir,
                            "dst_dir": dst_dir,
                            "names": ["exists.txt"],
                            "conflict_policy": "cancel",
                        },
                        token,
                    )
                    expect(409, status, f"{route} existing target conflict")

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
