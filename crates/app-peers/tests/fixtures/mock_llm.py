#!/usr/bin/env python3
"""A scripted OpenAI-compatible model for kernel tests. No network, no keys.

POST /v1/chat/completions answers "ECHO: <last user text>". A user text that
contains SLOW waits 20 s first (for revocation tests). Prints its port on the
first line of stdout, then serves until killed. Standard library only.
"""
import json
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


def last_user_text(body):
    for message in reversed(body.get("messages", [])):
        if message.get("role") == "user":
            content = message.get("content")
            if isinstance(content, list):
                return " ".join(p.get("text", "") for p in content if isinstance(p, dict))
            return content or ""
    return ""


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_GET(self):
        if self.path.endswith("/models"):
            self._json({"object": "list", "data": [{"id": "mock-model", "object": "model"}]})
        else:
            self.send_error(404)

    def do_POST(self):
        length = int(self.headers.get("content-length", "0"))
        body = json.loads(self.rfile.read(length) or b"{}")
        text = last_user_text(body)
        if "SLOW" in text:
            time.sleep(20)
        reply = "ECHO: " + text.strip().splitlines()[-1] if text.strip() else "ECHO:"
        if body.get("stream"):
            self.send_response(200)
            self.send_header("content-type", "text/event-stream")
            self.end_headers()
            for chunk in [reply[: len(reply) // 2], reply[len(reply) // 2 :]]:
                event = {"id": "c1", "object": "chat.completion.chunk", "model": "mock-model",
                         "choices": [{"index": 0, "delta": {"role": "assistant", "content": chunk}, "finish_reason": None}]}
                self.wfile.write(b"data: " + json.dumps(event).encode() + b"\n\n")
            done = {"id": "c1", "object": "chat.completion.chunk", "model": "mock-model",
                    "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}],
                    "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}}
            self.wfile.write(b"data: " + json.dumps(done).encode() + b"\n\n")
            self.wfile.write(b"data: [DONE]\n\n")
            self.wfile.flush()
        else:
            self._json({"id": "c1", "object": "chat.completion", "model": "mock-model",
                        "choices": [{"index": 0, "message": {"role": "assistant", "content": reply}, "finish_reason": "stop"}],
                        "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}})

    def _json(self, value):
        data = json.dumps(value).encode()
        self.send_response(200)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
print(server.server_address[1], flush=True)
server.serve_forever()
