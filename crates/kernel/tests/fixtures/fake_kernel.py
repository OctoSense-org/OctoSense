#!/usr/bin/env python3
"""A stand-in for `octos serve --stdio` in octos-core's tests.

NDJSON JSON-RPC on stdin/stdout. Every reply carries this process's pid, so
a test can tell kernels apart. `session/open` answers `opened` and then
sends one `session/ping` notification for the session; `session/list`
answers `{sessions: [{pid}]}`; `test/notify`
sends a notification for `params.session_id`; `test/exit` exits with 3.
Ends on stdin EOF. Writes `argv`, `cwd` and selected env to
`$FAKE_KERNEL_LOG` (if set) once at start.
"""
import json
import os
import sys

pid = os.getpid()
log = os.environ.get("FAKE_KERNEL_LOG")
if log:
    with open(log, "a") as f:
        f.write(json.dumps({"pid": pid, "argv": sys.argv[1:], "cwd": os.getcwd(),
                            "OCTOS_HOME": os.environ.get("OCTOS_HOME")}) + "\n")
print("fake kernel up", file=sys.stderr, flush=True)


def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    try:
        msg = json.loads(line)
    except ValueError:
        # `serve --host-managed` gets its two token lines first; skip them.
        continue
    method, rid, params = msg.get("method"), msg.get("id"), msg.get("params") or {}
    if method == "test/exit":
        print("fake kernel: asked to exit", file=sys.stderr, flush=True)
        sys.exit(3)
    if rid is None:
        continue
    if method == "session/open":
        sid = params.get("session_id")
        send({"jsonrpc": "2.0", "id": rid,
              "result": {"opened": {"session_id": sid, "active_profile_id": "_main"}, "pid": pid}})
        send({"jsonrpc": "2.0", "method": "session/ping", "params": {"session_id": sid, "pid": pid}})
    elif method == "session/list":
        send({"jsonrpc": "2.0", "id": rid, "result": {"sessions": [{"pid": pid}], "pid": pid}})
    elif method == "test/notify":
        send({"jsonrpc": "2.0", "id": rid, "result": {"pid": pid}})
        send({"jsonrpc": "2.0", "method": "session/ping", "params": {"session_id": params.get("session_id"), "pid": pid}})
    else:
        send({"jsonrpc": "2.0", "id": rid, "result": {"pid": pid, "method": method}})
