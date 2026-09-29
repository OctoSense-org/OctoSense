#!/usr/bin/env python3
"""A scripted OpenAI-compatible model that plays the system agent and an app
peer for the question/answer test. No network, no keys; standard library.

Rules, looking at the request's messages and offered tools:
  - after a tool result: finish with text ("TOOL SAID <result>" after a
    CALL_TOOL, "PEER GOT <answer>" when the result carries the system agent's
    answer, else "OK");
  - "CALL_TOOL:<model name> <json args>": call that tool when it is offered
    (a host-registered peer tool, UPCR-2026-035), else say "NO TOOL <name>
    AMONG <offered tools>";
  - a user text "TELL_PEER:<slug>" with peer_send_input offered: send the peer
    "QUESTION_ME" ("TELL_PEER_HOLD:<slug>": "QUESTION_HOLD", never answered);
  - "TELL_PEER_AGAIN:<slug>": send the peer "SECOND_INPUT";
  - "TELL_PEER_SUDO:<slug>": send the peer "RUN_SUDO", on which the peer runs
    a shell command when it is offered octos's `shell` (the kernel profile's
    policy denies it: ADR 0004 §12), else says "NO SHELL OFFERED";
  - "APPROVE_PEER:<slug>": try to approve a peer's tool with peer_respond;
  - "TELL_PEER_TOOL:<slug>": send the peer "CALL_APP_TOOL", on which the
    peer calls the host-registered app tool `rinx_echo` when it is offered
    (UPCR-2026-035), else says "NO APP TOOL OFFERED";
  - "TELL_PEER_SHOW:<slug>": send the peer "SHOW_SHARED";
  - "SHOW_SHARED": answer "SHARED " and the rows of the read-only
    <shared_history> block the kernel showed (octos UPCR-2026-034, the
    parallel person context), joined by " | ", or "SHARED NONE";
  - "RUN_TERMINAL": the system agent calls the host tool `terminal_run`
    registered on its session, else says "NO TERMINAL OFFERED";
  - "CALL_TOOL:<function>:<json args>": the same, spelled with a colon (the
    shell's relay test, G3), else say "NO TOOL <function>";
  - a user text "QUESTION_ME" with ask_user_question offered: ask one question;
  - a message naming a waiting peer with peer_respond offered: answer "42";
  - otherwise echo.
Prints its port, then serves until killed. Logs each decision to stderr and,
with MOCK_LLM_TOOLS_LOG set, appends {"user": <last user text>, "tools":
[<offered tool names>]} per request to that file.
"""
import itertools
import json
import os
import re
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

# MOCK_CALL_IDS=unique gives every tool call a distinct id, as most providers
# do. The default, "fixed", reuses "call_1" on every response, as scripted and
# some OpenAI-compatible servers do; the kernel must not lose inputs to it.
UNIQUE_IDS = os.environ.get("MOCK_CALL_IDS", "fixed") == "unique"
CALL_SEQ = itertools.count(1)


def text_of(message):
    content = message.get("content")
    if isinstance(content, list):
        return " ".join(p.get("text", "") for p in content if isinstance(p, dict))
    return content or ""


def decide(body):
    messages = body.get("messages", [])
    tools = {t.get("function", {}).get("name") for t in body.get("tools", []) or []}
    last = messages[-1] if messages else {}
    everything = "\n".join(text_of(m) for m in messages)
    if last.get("role") == "tool":
        result = text_of(last)
        if "CALL_TOOL:" in everything:
            return {"text": "TOOL SAID " + result}
        if "42" in result and "QUESTION_ME" in everything:
            return {"text": "PEER GOT 42"}
        return {"text": "OK"}
    last_user = ""
    for m in reversed(messages):
        if m.get("role") == "user":
            last_user = text_of(m)
            break
    host_tool = re.search(r"CALL_TOOL:([a-z0-9_]+) (\{.*\})", last_user)
    if host_tool:
        if host_tool.group(1) in tools:
            return {"tool": host_tool.group(1), "args": json.loads(host_tool.group(2))}
        return {"text": "NO TOOL " + host_tool.group(1) + " AMONG " + ",".join(sorted(t for t in tools if t))}
    again = re.search(r"TELL_PEER_AGAIN:([a-z0-9-]+)", last_user)
    if again and "peer_send_input" in tools:
        return {"tool": "peer_send_input", "args": {"slug": again.group(1), "message": "SECOND_INPUT"}}
    sudo = re.search(r"TELL_PEER_SUDO:([a-z0-9-]+)", last_user)
    if sudo and "peer_send_input" in tools:
        return {"tool": "peer_send_input", "args": {"slug": sudo.group(1), "message": "RUN_SUDO"}}
    if "RUN_SUDO" in last_user and "shell" in tools:
        return {"tool": "shell", "args": {"command": "rm -rf ./approval-probe && echo APPROVED_RAN"}}
    if "RUN_SUDO" in last_user:
        return {"text": "NO SHELL OFFERED"}
    show = re.search(r"TELL_PEER_SHOW:([a-z0-9-]+)", last_user)
    if show and "peer_send_input" in tools:
        return {"tool": "peer_send_input", "args": {"slug": show.group(1), "message": "SHOW_SHARED"}}
    if "SHOW_SHARED" in last_user:
        shared = [text_of(m) for m in messages if text_of(m).startswith("<shared_history")]
        if not shared:
            return {"text": "SHARED NONE"}
        rows = [line for line in shared[-1].splitlines() if line.startswith("- ")]
        return {"text": "SHARED " + " | ".join(rows)}
    tool = re.search(r"TELL_PEER_TOOL:([a-z0-9-]+)", last_user)
    if tool and "peer_send_input" in tools:
        return {"tool": "peer_send_input", "args": {"slug": tool.group(1), "message": "CALL_APP_TOOL"}}
    if "CALL_APP_TOOL" in last_user and "rinx_echo" in tools:
        return {"tool": "rinx_echo", "args": {"text": "ping"}}
    if "CALL_APP_TOOL" in last_user:
        return {"text": "NO APP TOOL OFFERED"}
    call = re.search(r"CALL_TOOL:([A-Za-z0-9_]+):(\{.*\})", last_user)
    if call:
        if call.group(1) in tools:
            return {"tool": call.group(1), "args": json.loads(call.group(2))}
        return {"text": "NO TOOL " + call.group(1)}
    if "RUN_TERMINAL" in last_user and "terminal_run" in tools:
        return {"tool": "terminal_run", "args": {"command": "ls"}}
    if "RUN_TERMINAL" in last_user:
        return {"text": "NO TERMINAL OFFERED"}
    approve = re.search(r"APPROVE_PEER:([a-z0-9-]+)", last_user)
    if approve and "peer_respond" in tools:
        return {"tool": "peer_respond", "args": {"slug": approve.group(1), "decision": "approve"}}
    match = re.search(r"TELL_PEER(_HOLD)?:([a-z0-9-]+)", last_user)
    if match and "peer_send_input" in tools:
        message = "QUESTION_HOLD" if match.group(1) else "QUESTION_ME"
        return {"tool": "peer_send_input", "args": {"slug": match.group(2), "message": message}}
    if ("QUESTION_ME" in last_user or "QUESTION_HOLD" in last_user) and "ask_user_question" in tools:
        return {"tool": "ask_user_question", "args": {"questions": [{
            "header": "Number", "question": "Which number should I use?",
            "options": [{"label": "42", "description": "the answer"}, {"label": "7"}]}]}}
    if "peer_respond" in tools and "QUESTION_HOLD" not in everything and "TELL_PEER_HOLD" not in everything:
        waiting = re.search(r"\b(rinx-[0-9a-f]{8})\b", last_user)
        if waiting and ("await" in last_user.lower() or "question" in last_user.lower() or "input" in last_user.lower()):
            return {"tool": "peer_respond", "args": {"slug": waiting.group(1), "answer": "42"}}
    return {"text": "ECHO: " + (last_user.strip().splitlines()[-1] if last_user.strip() else "")}


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers.get("content-length", "0"))) or b"{}")
        decision = decide(body)
        log = os.environ.get("MOCK_LLM_TOOLS_LOG")
        if log:
            user = next((text_of(m) for m in reversed(body.get("messages", [])) if m.get("role") == "user"), "")
            tools = [t.get("function", {}).get("name") for t in body.get("tools", []) or []]
            with open(log, "a") as f:
                f.write(json.dumps({"user": user, "tools": tools}) + "\n")
        last = (body.get("messages") or [{}])[-1]
        print("decision:", json.dumps(decision), "after:", text_of(last)[:300].replace("\n", " "), file=sys.stderr, flush=True)
        if "tool" in decision:
            call_id = f"call_{next(CALL_SEQ)}" if UNIQUE_IDS else "call_1"
            call = {"id": call_id, "type": "function",
                    "function": {"name": decision["tool"], "arguments": json.dumps(decision["args"])}}
            message = {"role": "assistant", "content": None, "tool_calls": [call]}
            finish = "tool_calls"
        else:
            message = {"role": "assistant", "content": decision["text"]}
            finish = "stop"
        if body.get("stream"):
            self.send_response(200)
            self.send_header("content-type", "text/event-stream")
            self.end_headers()
            delta = {"role": "assistant"}
            if "tool" in decision:
                delta["tool_calls"] = [{"index": 0, **call}]
            else:
                delta["content"] = decision["text"]
            for chunk in [
                {"choices": [{"index": 0, "delta": delta, "finish_reason": None}]},
                {"choices": [{"index": 0, "delta": {}, "finish_reason": finish}],
                 "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}},
            ]:
                chunk.update({"id": "c1", "object": "chat.completion.chunk", "model": "mock-model"})
                self.wfile.write(b"data: " + json.dumps(chunk).encode() + b"\n\n")
            self.wfile.write(b"data: [DONE]\n\n")
            self.wfile.flush()
            return
        data = json.dumps({"id": "c1", "object": "chat.completion", "model": "mock-model",
                           "choices": [{"index": 0, "message": message, "finish_reason": finish}],
                           "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}}).encode()
        self.send_response(200)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
print(server.server_address[1], flush=True)
server.serve_forever()
