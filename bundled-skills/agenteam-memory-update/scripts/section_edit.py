#!/usr/bin/env python3
"""Apply section-level edits to a Markdown memory file, safely.

Memory flows (memory update, Gardener, the profile Task) share files such as
the USER profile. Rewriting a whole file loses other flows' edits and lets a
model drop content it did not mean to touch. This tool changes only the
`## ` sections an edit names, refuses to run if the file changed since it was
read, can restrict which sections may change, and writes atomically. The
read, the --base-sha check and the write happen under one OS-level file lock, so two
writers that started from the same version cannot both succeed: the second one
waits, sees the new version and gets exit code 2.

  section_edit.py --file USER.md --show
  section_edit.py --file USER.md --edits edits.json --base-sha <sha> \
      --allow "背景,偏好" --max-bytes 20480

edits.json is a list of {"op": "replace|add|delete|append", "heading": "...",
"content": "..."}.
  replace  rewrite the body of an existing section (its leading <!-- --> hint
           comment is kept)
  add      append a new section at the end (heading must not exist)
  delete   remove an existing section
  append   add lines at the end of an existing section

Exit codes: 0 ok, 2 file changed since --base-sha (or lock busy), 3 section not allowed,
4 result too large, 5 invalid input. Nothing is written unless every edit
passes.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
import tempfile
import time
from contextlib import contextmanager
from pathlib import Path

HEADING = re.compile(r"^## +(.*\S)\s*$")
FENCE = re.compile(r"^ {0,3}(`{3,}|~{3,})")
OPS = ("replace", "add", "delete", "append")
EXIT_CHANGED, EXIT_NOT_ALLOWED, EXIT_TOO_LARGE, EXIT_INVALID = 2, 3, 4, 5


class EditError(Exception):
    def __init__(self, code: int, name: str, message: str) -> None:
        super().__init__(message)
        self.code, self.name, self.message = code, name, message


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def parse(text: str) -> tuple[list[str], list[dict]]:
    """Split into preamble lines and sections; a `## ` line in a code fence is content."""
    preamble: list[str] = []
    sections: list[dict] = []
    fence: str | None = None
    for line in text.split("\n"):
        marker = FENCE.match(line)
        if marker:
            fence = None if fence and marker.group(1).startswith(fence) else (fence or marker.group(1))
        heading = None if fence or marker else HEADING.match(line)
        if heading:
            sections.append({"heading": heading.group(1), "lines": [line]})
        else:
            (sections[-1]["lines"] if sections else preamble).append(line)
    return preamble, sections


def render(preamble: list[str], sections: list[dict]) -> str:
    return "\n".join(preamble + [line for section in sections for line in section["lines"]])


def body_has_heading(content: str) -> bool:
    fence: str | None = None
    for line in content.split("\n"):
        marker = FENCE.match(line)
        if marker:
            fence = None if fence and marker.group(1).startswith(fence) else (fence or marker.group(1))
            continue
        if not fence and HEADING.match(line):
            return True
    return False


def find(sections: list[dict], heading: str) -> int:
    hits = [i for i, section in enumerate(sections) if section["heading"] == heading]
    if len(hits) > 1:
        raise EditError(EXIT_INVALID, "AMBIGUOUS_HEADING", f"more than one section is titled {heading!r}")
    return hits[0] if hits else -1


def trim(lines: list[str]) -> list[str]:
    while lines and not lines[-1].strip():
        lines = lines[:-1]
    return lines


def leading_comment(lines: list[str]) -> list[str]:
    """The `<!-- ... -->` block right after a heading: where a chapter says who writes it."""
    body = lines[1:]
    i = 0
    while i < len(body) and not body[i].strip():
        i += 1
    if i >= len(body) or not body[i].lstrip().startswith("<!--"):
        return []
    j = i
    while j < len(body) and "-->" not in body[j]:
        j += 1
    return body[i : j + 1] if j < len(body) else []


def apply(text: str, edits: list[dict], allow: set[str] | None) -> tuple[str, list[str]]:
    preamble, sections = parse(text)
    changed: list[str] = []
    for edit in edits:
        op, heading, content = edit.get("op"), edit.get("heading"), edit.get("content", "")
        if op not in OPS or not isinstance(heading, str) or not heading.strip() or not isinstance(content, str):
            raise EditError(EXIT_INVALID, "INVALID_EDIT", f"invalid edit: {edit!r}")
        if op == "delete" and content:
            raise EditError(EXIT_INVALID, "INVALID_EDIT", "delete takes no content")
        if allow is not None and heading not in allow:
            raise EditError(EXIT_NOT_ALLOWED, "SECTION_NOT_ALLOWED", f"section {heading!r} may not be changed here")
        if body_has_heading(content):
            raise EditError(EXIT_INVALID, "INVALID_CONTENT", "content must not contain a '## ' heading")
        index = find(sections, heading)
        body = content.strip("\n").split("\n") if content.strip() else []
        if op == "add":
            if index >= 0:
                raise EditError(EXIT_INVALID, "SECTION_EXISTS", f"section {heading!r} already exists")
            if preamble and preamble[-1].strip():
                preamble.append("")
            if sections:
                sections[-1]["lines"] = trim(sections[-1]["lines"]) + [""]
            sections.append({"heading": heading, "lines": [f"## {heading}", "", *body, ""]})
        else:
            if index < 0:
                raise EditError(EXIT_INVALID, "SECTION_MISSING", f"section {heading!r} does not exist")
            section = sections[index]
            if op == "delete":
                del sections[index]
            elif op == "append":
                section["lines"] = trim(section["lines"]) + body + [""]
            else:  # replace
                keep = leading_comment(section["lines"])
                section["lines"] = [section["lines"][0], "", *(keep + [""] if keep else []), *body, ""]
        changed.append(heading)
    return render(preamble, sections), changed


def write_atomic(path: Path, text: str) -> None:
    handle, name = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(handle, "w", encoding="utf-8", newline="") as out:
            out.write(text)
        os.replace(name, path)
    except BaseException:
        if os.path.exists(name):
            os.unlink(name)
        raise


LOCK_WAIT_SECONDS = float(os.environ.get("SECTION_EDIT_LOCK_WAIT", "15"))


def lock_path(path: Path) -> Path:
    """One lock file per target, kept outside the workspace so nothing in it is ever listed or loaded."""
    key = hashlib.sha256(str(path.resolve()).encode("utf-8")).hexdigest()[:24]
    return Path(tempfile.gettempdir()) / f"agenteam-section-edit-{key}.lock"


def try_lock(handle) -> bool:
    try:
        if os.name == "nt":
            import msvcrt

            handle.seek(0)
            msvcrt.locking(handle.fileno(), msvcrt.LK_NBLCK, 1)
        else:
            import fcntl

            fcntl.flock(handle.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        return True
    except OSError:
        return False


def unlock(handle) -> None:
    try:
        if os.name == "nt":
            import msvcrt

            handle.seek(0)
            msvcrt.locking(handle.fileno(), msvcrt.LK_UNLCK, 1)
        else:
            import fcntl

            fcntl.flock(handle.fileno(), fcntl.LOCK_UN)
    except OSError:
        pass


@contextmanager
def file_lock(path: Path):
    """Exclusive OS-level lock (flock / LockFileEx). The kernel drops it when the holder exits or
    crashes, so there is no stale-lock reclaiming and nothing to race on. The lock file itself is
    never deleted: deleting a lock file is what lets two processes hold "the" lock."""
    handle = open(lock_path(path), "a+b")
    try:
        deadline = time.monotonic() + LOCK_WAIT_SECONDS
        while not try_lock(handle):
            if time.monotonic() > deadline:
                raise EditError(EXIT_CHANGED, "LOCK_BUSY", "another edit is still writing this file; read it again and retry")
            time.sleep(0.05)
        try:
            yield
        finally:
            unlock(handle)
    finally:
        handle.close()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--file", required=True)
    parser.add_argument("--edits", help="JSON file with the list of edits")
    parser.add_argument("--base-sha", help="sha256 of the file as you read it; refuse if it changed")
    parser.add_argument("--allow", help="comma-separated section titles that may change")
    parser.add_argument("--max-bytes", type=int, help="largest allowed size of the result")
    parser.add_argument("--show", action="store_true", help="print the file's sha256 and section titles")
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    path = Path(args.file)
    if args.show or args.dry_run:
        return run(args, path)
    try:
        with file_lock(path):
            return run(args, path)
    except EditError as error:
        print(json.dumps({"ok": False, "code": error.name, "message": error.message}, ensure_ascii=False))
        return error.code
    except OSError as error:
        print(json.dumps({"ok": False, "code": "UNREADABLE", "message": str(error)}, ensure_ascii=False))
        return EXIT_INVALID


def run(args: argparse.Namespace, path: Path) -> int:
    try:
        raw = path.read_bytes()
        text = raw.decode("utf-8")
    except (OSError, UnicodeDecodeError) as error:
        print(json.dumps({"ok": False, "code": "UNREADABLE", "message": str(error)}, ensure_ascii=False))
        return EXIT_INVALID
    crlf = "\r\n" in text
    text = text.replace("\r\n", "\n")

    if args.show:
        _, sections = parse(text)
        print(json.dumps({"ok": True, "sha256": sha256(raw), "bytes": len(raw),
                          "sections": [section["heading"] for section in sections]}, ensure_ascii=False))
        return 0

    try:
        if args.base_sha and args.base_sha != sha256(raw):
            raise EditError(EXIT_CHANGED, "FILE_CHANGED", "the file changed since you read it; read it again and redo the edit")
        if not args.edits:
            raise EditError(EXIT_INVALID, "INVALID_EDIT", "--edits is required")
        edits = json.loads(Path(args.edits).read_text(encoding="utf-8"))
        if not isinstance(edits, list) or not edits:
            raise EditError(EXIT_INVALID, "INVALID_EDIT", "edits must be a non-empty list")
        allow = {name.strip() for name in args.allow.split(",") if name.strip()} if args.allow else None
        result, changed = apply(text, edits, allow)
        if crlf:
            result = result.replace("\n", "\r\n")
        data = result.encode("utf-8")
        if args.max_bytes is not None and len(data) > args.max_bytes:
            raise EditError(EXIT_TOO_LARGE, "TOO_LARGE", f"result would be {len(data)} bytes, over the {args.max_bytes} limit")
    except (EditError, json.JSONDecodeError, OSError, UnicodeDecodeError) as error:
        code = error.code if isinstance(error, EditError) else EXIT_INVALID
        name = error.name if isinstance(error, EditError) else "INVALID_EDIT"
        message = error.message if isinstance(error, EditError) else str(error)
        print(json.dumps({"ok": False, "code": name, "message": message}, ensure_ascii=False))
        return code

    if not args.dry_run and data != raw:
        write_atomic(path, result)
    print(json.dumps({"ok": True, "changed": changed, "sha256": sha256(data), "bytes": len(data),
                      "written": not args.dry_run and data != raw}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
