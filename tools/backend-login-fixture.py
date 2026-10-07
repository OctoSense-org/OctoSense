#!/usr/bin/env python3
"""Disposable real HTTP authorization server for native backend-login acceptance.

Only fictional accounts belong here. No users, codes or tokens are pre-seeded;
the browser must register/sign in and the host must exchange a PKCE-bound code.
HTTP loopback requires the host's non-default acceptance-fixtures build.
"""
import argparse
import base64
import hashlib
import hmac
import html
import json
import os
from pathlib import Path
import secrets
import socket
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from http.cookies import SimpleCookie
from urllib.parse import parse_qs, urlencode, urlsplit


WEBVIEW_CALLBACK_URL = "https://octosense.invalid/auth/callback"


def callback_origin(value):
    """Only the existing host loopback or the byte-exact embedded callback."""
    if value == WEBVIEW_CALLBACK_URL:
        return "https://octosense.invalid"
    try:
        redirect = urlsplit(value)
        if (redirect.scheme == "http" and redirect.hostname == "127.0.0.1"
            and redirect.port and redirect.path == "/oauth/callback"
            and not redirect.username and not redirect.password
            and not redirect.query and not redirect.fragment):
            return f"http://127.0.0.1:{redirect.port}"
    except ValueError:
        pass
    return None


class State:
    def __init__(self, directory, app_id, token_ttl_seconds=3600):
        self.directory, self.app_id = directory, app_id
        self.token_ttl_seconds = token_ttl_seconds
        self.fail_me = 0
        self.navigation_available = True
        self.lock = threading.Lock()
        self.users, self.flows, self.codes, self.access, self.refresh = {}, {}, {}, {}, {}
        self.sequence = 0
        self.web_sessions = set()

    def event(self, event, status):
        # Never record query strings, forms, passwords, codes or bearer tokens.
        self.sequence += 1
        with (self.directory / "events.jsonl").open("a") as f:
            f.write(json.dumps({"sequence": self.sequence, "event": event, "status": status}) + "\n")

    def token_pair(self, subject, family=None):
        access, refresh = secrets.token_urlsafe(32), secrets.token_urlsafe(32)
        family = family or secrets.token_urlsafe(24)
        self.access[access] = (subject, time.monotonic() + self.token_ttl_seconds, family)
        self.refresh[refresh] = (subject, family)
        return {"access_token": access, "refresh_token": refresh, "token_type": "Bearer",
                "expires_in": self.token_ttl_seconds, "scope": "app.session"}


def handler(state):
    class Handler(BaseHTTPRequestHandler):
        server_version = "OctoSenseAcceptance/1"

        def log_message(self, *_):
            pass

        def respond(self, status, payload, kind="application/json", headers=None):
            raw = json.dumps(payload).encode() if kind == "application/json" else payload.encode()
            self.send_response(status)
            self.send_header("Content-Type", kind + "; charset=utf-8")
            self.send_header("Content-Length", str(len(raw)))
            self.send_header("Cache-Control", "no-store")
            self.send_header("Referrer-Policy", "no-referrer")
            self.send_header("X-Content-Type-Options", "nosniff")
            # Browsers apply form-action to the post-login redirect too. Allow
            # only this flow's validated host callback origin, never a wildcard.
            callback_origin = getattr(self, "callback_origin", "")
            form_actions = "'self'" + (" " + callback_origin if callback_origin else "")
            self.send_header("Content-Security-Policy", "default-src 'none'; style-src 'unsafe-inline'; form-action " + form_actions + "; frame-ancestors 'none'; base-uri 'none'")
            for key, value in (headers or {}).items():
                self.send_header(key, value)
            self.end_headers()
            self.wfile.write(raw)

        def has_browser_cookie(self):
            cookies = SimpleCookie()
            try:
                cookies.load(self.headers.get("Cookie", ""))
                marker = cookies.get("fixture_browser")
                return marker is not None and marker.value in state.web_sessions
            except Exception:
                return False

        def form(self, flow_id, notice=""):
            return """<!doctype html><html lang="en"><meta charset="utf-8"><title>OctoSense synthetic backend</title>
<style>body{font:18px system-ui;max-width:540px;margin:60px auto;padding:24px;color:#172033;background:#f6f8fc}main{padding:28px;background:white;border-radius:16px}label{display:block;margin-top:18px}input{display:block;width:94%;padding:12px;font:inherit}button{margin:24px 10px 0 0;padding:12px;font:inherit}p{line-height:1.5}</style>
<main><h1>Backend account</h1><p>Fictional acceptance server. Use a new fictional username and password. Never use a real account.</p>
<p><a id="information" href="/information">About this fictional backend</a></p>
<p><a id="connection_check" href="/navigation-check">Check fictional connection</a></p>
<p id="notice" role="status">""" + html.escape(notice) + """</p><form method="post" action="/session">
<input type="hidden" name="flow" value=""" + '"' + html.escape(flow_id, quote=True) + '"' + """>
<label for="username">Fictional username</label><input id="username" name="username" autocomplete="off" required maxlength="64">
<label for="password">Fictional password</label><input id="password" name="password" type="password" autocomplete="off" required minlength="8" maxlength="128">
<button id="register" name="action" value="register">Create account</button><button id="login" name="action" value="login">Sign in</button></form></main></html>"""

        def do_GET(self):
            with state.lock:
                path = urlsplit(self.path)
                if path.path == "/health":
                    return self.respond(200, {"fixture": True})
                if path.path == "/navigation-check":
                    if not state.navigation_available:
                        state.event("navigation_error", 503)
                        self.close_connection = True
                        self.connection.shutdown(socket.SHUT_RDWR)
                        self.connection.close()
                        return
                    state.event("navigation_recovered", 200)
                    return self.respond(200, '<!doctype html><html><title>Connection restored</title><h1 id="connection-restored">Connection restored</h1><p>Use the host Back control to return.</p></html>', "text/html")
                if path.path == "/information":
                    state.event("information", 200)
                    return self.respond(200, '<!doctype html><html><title>Fictional backend information</title><h1 id="information-title">Fictional backend information</h1><p>Use the host Back control to return to the login form.</p></html>', "text/html")
                if path.path == "/me":
                    if state.fail_me:
                        state.fail_me -= 1
                        state.event("me", 503)
                        return self.respond(503, {"error": "synthetic_temporary_outage"})
                    token = self.headers.get("Authorization", "").removeprefix("Bearer ")
                    record = state.access.get(token)
                    valid = record and record[1] > time.monotonic()
                    state.event("me", 200 if valid else 401)
                    if not valid:
                        return self.respond(401, {"error": "unauthorized"})
                    user = next(v for v in state.users.values() if v["sub"] == record[0])
                    return self.respond(200, {"sub": record[0], "label": user["label"]})
                if path.path != "/authorize":
                    return self.respond(404, {"error": "not_found"})
                q = parse_qs(path.query, keep_blank_values=True)
                required = ["client_id", "redirect_uri", "response_type", "state", "code_challenge", "code_challenge_method", "scope"]
                if any(len(q.get(k, [])) != 1 for k in required):
                    return self.respond(400, {"error": "invalid_request"})
                q = {k: v[0] for k, v in q.items()}
                origin = callback_origin(q["redirect_uri"])
                if (q["client_id"] != "octosense-fixture" or q["response_type"] != "code"
                    or q["code_challenge_method"] != "S256" or q["scope"] != "app.session"
                    or not 20 <= len(q["state"]) <= 256 or len(q["code_challenge"]) != 43
                    or origin is None):
                    return self.respond(400, {"error": "invalid_request"})
                flow = secrets.token_urlsafe(32)
                self.callback_origin = origin
                state.flows[flow] = dict(q, expires=time.monotonic() + 600)
                state.event("authorize_cookie", 200 if self.has_browser_cookie() else 204)
                marker = secrets.token_urlsafe(24)
                state.web_sessions.add(marker)
                state.event("authorize", 200)
                self.respond(200, self.form(flow), "text/html", {
                    "Set-Cookie": f"fixture_browser={marker}; Path=/; HttpOnly; SameSite=Lax"})

        def do_POST(self):
            length = int(self.headers.get("Content-Length", "0"))
            if not 0 < length <= 16384:
                return self.respond(400, {"error": "invalid_request"})
            body = self.rfile.read(length)
            path = urlsplit(self.path).path
            with state.lock:
                # Fault control belongs only to this disposable acceptance
                # server. It changes availability, never identity or tokens.
                if path == "/fixture/navigation-availability":
                    try:
                        available = json.loads(body)["available"]
                        if type(available) is not bool:
                            raise ValueError()
                    except (ValueError, KeyError, TypeError):
                        return self.respond(400, {"error": "invalid_fault_control"})
                    state.navigation_available = available
                    state.event("navigation_availability", 200)
                    return self.respond(200, {"available": available})
                if path == "/fixture/fail-next-me":
                    try:
                        count = json.loads(body)["count"]
                        if type(count) is not int or not 1 <= count <= 5:
                            raise ValueError()
                    except (ValueError, KeyError, TypeError):
                        return self.respond(400, {"error": "invalid_fault_control"})
                    state.fail_me = count
                    state.event("fault_me", 200)
                    return self.respond(200, {"scheduled_failures": count})
                if path == "/logout":
                    token = self.headers.get("Authorization", "").removeprefix("Bearer ")
                    record = state.access.pop(token, None)
                    if record:
                        family = record[2]
                        state.access = {t: r for t, r in state.access.items() if r[2] != family}
                        state.refresh = {t: r for t, r in state.refresh.items() if r[1] != family}
                    state.event("logout", 200)
                    return self.respond(200, {"logged_out": True})
                if self.headers.get_content_type() != "application/x-www-form-urlencoded":
                    return self.respond(400, {"error": "invalid_request"})
                fields = parse_qs(body.decode(), keep_blank_values=True)
                if any(len(v) != 1 for v in fields.values()):
                    return self.respond(400, {"error": "invalid_request"})
                q = {k: v[0] for k, v in fields.items()}
                if path == "/session":
                    flow = state.flows.get(q.get("flow"))
                    if not flow or flow["expires"] < time.monotonic():
                        return self.respond(400, {"error": "expired_flow"})
                    self.callback_origin = callback_origin(flow["redirect_uri"])
                    state.event("session_cookie", 200 if self.has_browser_cookie() else 204)
                    username, password = q.get("username", ""), q.get("password", "")
                    if not 1 <= len(username) <= 64 or not 8 <= len(password) <= 128:
                        return self.respond(400, self.form(q["flow"], "Use a fictional username and a password of at least 8 characters."), "text/html")
                    user = state.users.get(username)
                    if q.get("action") == "register":
                        if user:
                            return self.respond(409, self.form(q["flow"], "Username already registered. Sign in."), "text/html")
                        salt = secrets.token_bytes(16)
                        state.users[username] = {"sub": "synthetic-" + secrets.token_hex(16), "label": username,
                            "salt": salt, "hash": hashlib.pbkdf2_hmac("sha256", password.encode(), salt, 120000)}
                        state.event("register", 201)
                        return self.respond(201, self.form(q["flow"], "Account created. Sign in to continue."), "text/html")
                    if (q.get("action") != "login" or not user or not hmac.compare_digest(user["hash"],
                            hashlib.pbkdf2_hmac("sha256", password.encode(), user["salt"], 120000))):
                        state.event("login", 401)
                        return self.respond(401, self.form(q["flow"], "Username or password incorrect."), "text/html")
                    del state.flows[q["flow"]]
                    code = secrets.token_urlsafe(32)
                    state.codes[code] = dict(flow, sub=user["sub"], expires=time.monotonic() + 60)
                    state.event("login", 303)
                    return self.respond(303, "Return to OctoSense.", "text/plain", {
                        "Location": flow["redirect_uri"] + "?" + urlencode({"code": code, "state": flow["state"]})})
                if path != "/token" or q.get("client_id") != "octosense-fixture":
                    return self.respond(400, {"error": "invalid_client"})
                if q.get("grant_type") == "refresh_token":
                    record = state.refresh.pop(q.get("refresh_token"), None)
                    state.event("refresh", 200 if record else 400)
                    return self.respond(200, state.token_pair(*record)) if record else self.respond(400, {"error": "invalid_grant"})
                code = state.codes.pop(q.get("code"), None)
                challenge = base64.urlsafe_b64encode(hashlib.sha256(q.get("code_verifier", "").encode()).digest()).decode().rstrip("=")
                valid = (q.get("grant_type") == "authorization_code" and code
                         and code["expires"] > time.monotonic()
                         and q.get("redirect_uri") == code["redirect_uri"]
                         and hmac.compare_digest(challenge, code["code_challenge"]))
                state.event("exchange", 200 if valid else 400)
                return self.respond(200, state.token_pair(code["sub"])) if valid else self.respond(400, {"error": "invalid_grant"})
    return Handler


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--port", type=int, default=0)
    parser.add_argument("--app-id", default="org.octosense.samples.backend")
    parser.add_argument("--token-ttl-seconds", type=int, default=3600)
    args = parser.parse_args()
    if not 1 <= args.token_ttl_seconds <= 3600:
        parser.error("--token-ttl-seconds must be between 1 and 3600")
    os.umask(0o077)
    args.directory.mkdir(mode=0o700, parents=True, exist_ok=False)
    state = State(args.directory, args.app_id, args.token_ttl_seconds)
    server = ThreadingHTTPServer(("127.0.0.1", args.port), handler(state))
    origin = f"http://127.0.0.1:{server.server_port}"
    registration = {"id": "fixture", "app_id": args.app_id, "client_id": "octosense-fixture",
        "authorization_url": origin + "/authorize", "token_url": origin + "/token",
        "me_url": origin + "/me", "logout_url": origin + "/logout", "scopes": ["app.session"]}
    (args.directory / "metadata.json").write_text(json.dumps({"fixture": True, "origin": origin,
        "token_ttl_seconds": args.token_ttl_seconds, "registration": registration}, indent=2))
    print(json.dumps({"ready": True, "origin": origin}), flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()


if __name__ == "__main__":
    main()
