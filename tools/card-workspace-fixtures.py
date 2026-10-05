#!/usr/bin/env python3
"""Export fictional Glance UX cases for the dev-mode glance-fixtures action.

This prepares inputs only: it never connects an account, grants an agent,
contacts a provider, changes model-authored source, or marks a UX check passed.
"""
import argparse
import hashlib
import json
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
PUBLISHERS = {"mail": "os.mail", "calendar": "os.calendar", "news": "os.news",
              "photo": "os.photos", "finance": "test.finance", "youtube": "os.youtube"}
ICONS = {"mail": "mail", "news": "bell", "photos": "image", "maps": "map",
         "camera": "camera", "youtube": "play"}


def notice(family):
    app = "os.calendar" if family in ("agenda", "empty") else "os." + family
    if family in ICONS:
        source = (REPO / "crates/shell/resources/glance/notice.card").read_text()
        source = source.replace("{icon}", ICONS[family]).replace('"{app}"', json.dumps(family.title()))
        data = {"note": {"title": family.title() + " test notice / 测试通知", "as_of": "10:00",
                         "summary": ("This is a fictional notice. Read the whole message before acting. 这是完整通知内容。 " * 5) + "End of notice."}}
    elif family == "calendar":
        source = (REPO / "apps/calendar/host-service/resources/event.card").read_text()
        data = {"ev": {"title": "Design review / 产品体验评审", "as_of": "Event",
                       "metric1_label": "Day", "metric1_value": "Monday 5 October",
                       "metric2_label": "Time", "metric2_value": "14:00–15:30",
                       "subtitle": "Studio B, north entrance, second floor / 二楼北侧",
                       "summary": ("Review card behavior, chat and editing. 核对内容、交互与保存结果。\n" * 8) + "End of notes."}}
    else:
        source = (REPO / "apps/calendar/host-service/resources/agenda.card").read_text()
        empty = family == "empty"
        day = {"title": "Nothing in the next 7 days" if empty else "Your next three events",
               "as_of": "Mon 5 Oct", "summary": "" if empty else "and 3 more in the next 7 days"}
        titles = ["Design review and engineering handoff", "Appointment with Dr. Rivera", "Train to the airport"]
        for i, title in enumerate(titles, 1):
            day[f"pick{i}_title"] = "" if empty else title
            day[f"pick{i}_body"] = "" if empty else "Mon 5 Oct · 14:00–15:30 · Studio B, north entrance, second floor"
        data = {"day": day}
    return app, {"source": source, "data": data}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("family", choices=sorted(set(ICONS) | set(PUBLISHERS) | {"agenda", "empty"}))
    parser.add_argument("--model", choices=["deepseek", "minimax"])
    parser.add_argument("--templates", type=Path, help="Read-only continuations directory from App Design Flow")
    parser.add_argument("--syntax", choices=["l0", "splash"], default="l0")
    parser.add_argument("--source", type=Path, help="Exact model-authored replacement source; requires --model")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.model:
        if args.templates is None or args.family not in PUBLISHERS:
            parser.error("model cases need --templates and a supported template family")
        turn = "turn-12" if args.model == "deepseek" else "turn-14"
        folder = args.templates / args.model / turn / "card-templates" / args.family
        source = (args.source or folder / ("glance.card" if args.syntax == "l0" else "bundle/main.splash")).read_bytes()
        body = {"source" if args.syntax == "l0" else "script": source.decode()}
        if args.syntax == "l0":
            body["data"] = json.loads((folder / "glance.data.json").read_bytes())
        app = PUBLISHERS[args.family]
    else:
        if args.source:
            parser.error("--source requires a model case")
        if args.family not in set(ICONS) | {"calendar", "agenda", "empty"}:
            parser.error("this family has a model prototype only")
        if args.syntax != "l0":
            parser.error("shipping notice templates use L0")
        app, body = notice(args.family)
    fixture = {"app": app, "args": {
        "card_id": "acceptance-" + args.family, "title": args.family.title() + " · UX check",
        "summary": "Fictional UX test · no external action", "priority": 100, "notify": False, **body}}
    encoded = json.dumps([fixture], ensure_ascii=False, indent=2).encode()
    if len(encoded) > 256 * 1024:
        parser.error("fixture exceeds the developer harness size limit")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_bytes(encoded)
    receipt = {"family": args.family, "model": args.model, "syntax": args.syntax, "publisher": app,
               "fixture_sha256": hashlib.sha256(encoded).hexdigest(),
               "source_sha256": hashlib.sha256(body.get("source", body.get("script")).encode()).hexdigest(),
               "external_actions": False, "ux_result": "not evaluated"}
    args.output.with_suffix(".receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps(receipt))


if __name__ == "__main__":
    main()
