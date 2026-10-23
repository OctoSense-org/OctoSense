#!/usr/bin/env python3
"""Extract tool inputs/results and committed assistant finals from local octos evidence.

Accepts UI-ledger snapshot JSON, arrays of ledger entries, ledger JSONL, and
session-message JSONL. Reasoning, streamed deltas, user prompts and provider
metadata are excluded by an allowlist. Credential-like fields are redacted.
A retained ring can be incomplete: this tool records that limitation and
never treats a requested view_image call as proof of image delivery.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re

SESSION = "_main:api:octosense#system"
SENSITIVE = {"api_key", "apikey", "access_token", "refresh_token", "authorization",
             "proxy_authorization", "password", "secret", "credentials", "token"}


def clean(value):
    if isinstance(value, dict):
        return {key: "[redacted]" if key.lower().replace("-", "_") in SENSITIVE else clean(item)
                for key, item in value.items()
                if "reasoning" not in key.lower() and key.lower() not in ("thinking", "thought_signature")}
    if isinstance(value, list):
        return [clean(item) for item in value]
    if not isinstance(value, str):
        return value
    text = re.sub(r"(?i)\bBearer\s+[^\s\"']+", "Bearer [redacted]", value)
    text = re.sub(r"\bsk-[A-Za-z0-9_-]{16,}\b", "[redacted-api-key]", text)
    text = re.sub(r'(?i)(["\'](?:api_key|access_token|refresh_token|password|authorization|secret)["\']\s*:\s*)["\'][^"\']*["\']',
                  r'\1"[redacted]"', text)
    text = re.sub(r"(?i)([?&](?:api_key|access_token|token|key)=)[^&\s]+", r"\1[redacted]", text)
    return text


def load_records(path):
    raw = path.read_bytes()
    text = raw.decode("utf-8")
    try:
        root = json.loads(text)
        if isinstance(root, list):
            return root, raw
        if isinstance(root, dict) and isinstance(root.get("entries"), list):
            return root["entries"], raw
        return [root], raw
    except json.JSONDecodeError:
        records = []
        for index, line in enumerate(text.splitlines()):
            if not line.strip():
                continue
            try:
                records.append(json.loads(line))
            except json.JSONDecodeError as error:
                raise ValueError(f"{path.name}: invalid JSONL line {index + 1}; copy after writer flush") from error
        return records, raw


def parse_output(text):
    if not isinstance(text, str):
        return text if isinstance(text, dict) else None
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        return None


def extract(records, session=SESSION):
    calls, persisted, terminals, finals, errors = {}, {}, {}, [], []
    def call_for(turn, call_id):
        key = (turn or "unknown", call_id)
        return calls.setdefault(key, {"turn_id": turn, "tool_call_id": call_id})
    for record in records:
        if not isinstance(record, dict):
            continue
        event = record.get("event", {})
        if isinstance(event, dict) and event.get("session_id") == session:
            turn = event.get("turn_id")
            kind = event.get("kind")
            if kind in ("tool_started", "tool_completed"):
                item = call_for(turn, event.get("tool_call_id"))
                item["name"] = event.get("tool_name")
                if kind == "tool_started":
                    item.update(arguments=clean(event.get("arguments")), input_complete=True, start_seq=record.get("seq"))
                else:
                    item.update(success=event.get("success"), output=clean(event.get("output_preview")),
                                output_is_preview=True, end_seq=record.get("seq"))
            elif kind == "turn_completed":
                identity = event.get("session_result") or {}
                terminals[turn] = identity.get("message_id")
            elif kind == "turn_error":
                errors.append({"turn_id": turn, "code": event.get("code"), "message": clean(event.get("message"))})
                partial = event.get("partial_result") or {}
                identity = partial.get("session_result") or {}
                if identity.get("message_id"):
                    terminals[turn] = identity["message_id"]
            elif kind == "envelope_v2":
                envelope = event.get("envelope", {})
                turn = envelope.get("turn_id", turn)
                payload = envelope.get("payload", {})
                data = payload.get("data", {})
                payload_type = payload.get("type")
                if payload_type == "assistant_persisted":
                    message_id = (data.get("meta") or {}).get("message_id")
                    if message_id:
                        persisted[message_id] = {"turn_id": turn, "message_id": message_id, "text": clean(data.get("text", ""))}
                elif payload_type == "tool_start":
                    item = call_for(turn, data.get("tool_call_id"))
                    item.setdefault("name", data.get("name"))
                    if "arguments" not in item:
                        item.update(arguments=clean(data.get("arguments_preview")), input_complete=False)
                elif payload_type == "tool_end":
                    item = call_for(turn, data.get("tool_call_id"))
                    item.setdefault("success", data.get("status") == "complete")
                    item.setdefault("output", clean(data.get("output_preview")))
                    item.setdefault("output_is_preview", True)
            continue
        # Session JSONL stores Message directly; trust only the selected input
        # file's scope. Metadata, prompts and reasoning_content are never copied.
        role = record.get("role")
        turn = record.get("thread_id")
        if role == "assistant":
            for tool in record.get("tool_calls") or []:
                item = call_for(turn, tool.get("id"))
                item.update(name=tool.get("name"), arguments=clean(tool.get("arguments")), input_complete=True)
            # No terminal identity in legacy Message JSONL: omit its prose.
        elif role == "tool" and record.get("tool_call_id"):
            item = call_for(turn, record["tool_call_id"])
            item.update(output=clean(record.get("content")), output_is_preview=False)
    for turn, message_id in terminals.items():
        if message_id in persisted:
            finals.append({**persisted[message_id], "terminal_verified": True, "source": "ledger_terminal_message_id"})
    result = list(calls.values())
    for item in result:
        if item.get("name") == "view_image":
            output = parse_output(item.get("output"))
            shown = output.get("shown_to_model") if isinstance(output, dict) else None
            item["image_delivery"] = "reported_shown_to_model" if shown is True else "reported_not_shown" if shown is False else "unverified"
    return {"tool_calls": result, "assistant_finals": finals, "turn_errors": errors}


def merge_extractions(parts):
    calls, finals, errors = {}, {}, {}
    for part in parts:
        for call in part.get("tool_calls", []):
            key = (call.get("turn_id"), call.get("tool_call_id"))
            old = calls.setdefault(key, {})
            # Never replace complete input/output with a later ring preview.
            incoming = dict(call)
            if old.get("input_complete") and not incoming.get("input_complete"):
                incoming.pop("arguments", None)
                incoming.pop("input_complete", None)
            if old.get("output_is_preview") is False and incoming.get("output_is_preview"):
                incoming.pop("output", None)
                incoming.pop("output_is_preview", None)
            old.update(incoming)
        for final in part.get("assistant_finals", []):
            key = (final.get("turn_id"), final.get("message_id"), final.get("text"))
            finals[key] = final
        for error in part.get("turn_errors", []):
            errors[(error.get("turn_id"), error.get("code"), error.get("message"))] = error
    return {"tool_calls": list(calls.values()), "assistant_finals": list(finals.values()), "turn_errors": list(errors.values())}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, action="append", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--merge", type=Path, help="previous sanitized output, preserving tool events evicted from a ledger ring")
    parser.add_argument("--session", default=SESSION)
    args = parser.parse_args()
    sources, parts, all_records = [], [], []
    if args.merge:
        previous = json.loads(args.merge.read_text())
        parts.append(previous)
        sources.extend(previous.get("sources", []))
    for path in args.input:
        records, raw = load_records(path)
        all_records.extend(records)
        sources.append({"name": path.name, "sha256": hashlib.sha256(raw).hexdigest()})
    parts.append(extract(all_records, args.session))
    output = {"schema": 1, "session": args.session, "sources": sources, **merge_extractions(parts),
              "limitations": ["Ledger rings and output previews may omit events or truncate results.",
                              "Legacy assistant prose without a terminal identity is omitted.",
                              "shown_to_model is runtime delivery evidence, not a visual quality judgment.",
                              "Reasoning, user prompts and provider metadata are excluded; credential-like fields are redacted."]}
    calls = output["tool_calls"]
    for call in calls:
        if call.get("name") == "view_image":
            payload = parse_output(call.get("output"))
            shown = payload.get("shown_to_model") if isinstance(payload, dict) else None
            call["image_delivery"] = "reported_shown_to_model" if shown is True else "reported_not_shown" if shown is False else "unverified"
    output["summary"] = {"tool_calls": len(calls), "assistant_finals": len(output["assistant_finals"]),
        "turn_errors": len(output["turn_errors"]),
        "authoring_calls_observed": sum(c.get("name") in ("write_file", "edit_file", "apply_patch") for c in calls),
        "view_image_calls": sum(c.get("name") == "view_image" for c in calls),
        "images_reported_shown": sum(c.get("image_delivery") == "reported_shown_to_model" for c in calls)}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(clean(output), indent=2, ensure_ascii=False) + "\n")
    print(json.dumps(output["summary"], indent=2))


if __name__ == "__main__":
    main()
