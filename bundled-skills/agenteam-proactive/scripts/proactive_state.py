#!/usr/bin/env python3
"""Restraint and clue bookkeeping for proactive reminders.

State lives in <workspace>/memory/proactive-state.json. Rules live in
<workspace>/memory/PROACTIVE_RULES.md (written with section_edit.py, not here).
Every subcommand prints one JSON object. Exit code is 0 unless usage is wrong.

  status                      enabled / quiet hours / daily budget / learn due
  judged --key K              was K already judged today? (records it if not)
  sent                        count one delivered reminder against today's cap
  clue --area A --decision D [--direct]   record a learning clue; says promote
  learned                     mark today's learning pass as done
"""
import argparse
import json
import os
import re
import sys
import tempfile
from datetime import datetime

RULES = os.path.join("memory", "PROACTIVE_RULES.md")
STATE = os.path.join("memory", "proactive-state.json")
DAILY_CAP = 5
QUIET_START, QUIET_END = 22, 8
PROMOTE_AT = 2
DECISIONS = {"tell_now", "prepare", "follow_up", "silent"}
MAX_KEYS = 500


def read_front_matter(path):
    try:
        text = open(path, encoding="utf-8-sig").read()
    except OSError:
        return {}
    match = re.match(r"---\r?\n(.*?)\r?\n---", text, re.S)
    out = {}
    for line in (match.group(1).splitlines() if match else []):
        key, _, value = line.partition(":")
        if _:
            out[key.strip()] = value.strip().strip("\"'")
    return out


def load(ws):
    try:
        data = json.load(open(os.path.join(ws, STATE), encoding="utf-8"))
        return data if isinstance(data, dict) else {}
    except (OSError, ValueError):
        return {}


def save(ws, state):
    path = os.path.join(ws, STATE)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    fd, tmp = tempfile.mkstemp(dir=os.path.dirname(path), suffix=".tmp")
    with os.fdopen(fd, "w", encoding="utf-8") as handle:
        json.dump(state, handle, ensure_ascii=False, indent=1)
    os.replace(tmp, path)


def today_state(state, now):
    day = now.strftime("%Y-%m-%d")
    if state.get("day") != day:
        state["day"] = day
        state["sent"] = 0
        state["judged"] = []
    return state


def quiet(now, front):
    start = int(front.get("quietStart", QUIET_START) or QUIET_START)
    end = int(front.get("quietEnd", QUIET_END) or QUIET_END)
    hour = now.hour
    return hour >= start or hour < end if start > end else start <= hour < end


def cap(front):
    try:
        return max(0, int(front.get("dailyCap", DAILY_CAP)))
    except ValueError:
        return DAILY_CAP


def main(argv=None, now=None):
    parser = argparse.ArgumentParser()
    parser.add_argument("--workspace", default=".")
    parser.add_argument("command", choices=["status", "judged", "sent", "clue", "learned"])
    parser.add_argument("--key")
    parser.add_argument("--area")
    parser.add_argument("--decision")
    parser.add_argument("--direct", action="store_true")
    args = parser.parse_args(argv)
    now = now or datetime.now()
    ws = args.workspace
    front = read_front_matter(os.path.join(ws, RULES))
    enabled = front.get("enabled", "false").lower() == "true"
    state = today_state(load(ws), now)

    if args.command == "status":
        out = {
            "enabled": enabled,
            "quiet": quiet(now, front),
            "sentToday": state["sent"],
            "remaining": max(0, cap(front) - state["sent"]),
            "learnDue": state.get("learnedDay") != state["day"],
        }
    elif args.command == "judged":
        if not args.key:
            parser.error("--key required")
        seen = args.key in state["judged"]
        if not seen:
            state["judged"] = (state["judged"] + [args.key])[-MAX_KEYS:]
            save(ws, state)
        out = {"alreadyJudged": seen}
    elif args.command == "sent":
        state["sent"] += 1
        save(ws, state)
        out = {"sentToday": state["sent"], "remaining": max(0, cap(front) - state["sent"])}
    elif args.command == "clue":
        if not args.area or args.decision not in DECISIONS:
            parser.error("--area and --decision (%s) required" % "/".join(sorted(DECISIONS)))
        clues = state.setdefault("clues", {})
        k = "%s|%s" % (args.area.strip().lower(), args.decision)
        entry = clues.setdefault(k, {"count": 0, "promoted": False})
        entry["count"] += 1
        promote = not entry["promoted"] and (args.direct or entry["count"] >= PROMOTE_AT)
        if promote:
            entry["promoted"] = True
        save(ws, state)
        out = {"count": entry["count"], "promote": promote}
    else:
        state["learnedDay"] = state["day"]
        save(ws, state)
        out = {"learnedDay": state["day"]}
    print(json.dumps(out, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
