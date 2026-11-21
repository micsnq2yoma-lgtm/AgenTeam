"""Compare two complete cargo clippy JSON streams from the same checkout path."""

import json
import sys
from pathlib import Path


def warnings(log, root):
    result = set()
    complete = False
    with log.open(encoding="utf-8") as stream:
        for line in stream:
            event = json.loads(line)
            if event["reason"] == "build-finished":
                complete = event["success"]
            if event["reason"] != "compiler-message":
                continue
            diagnostic = event["message"]
            if diagnostic["level"] == "error":
                raise ValueError(f"{log.name}: {diagnostic['message']}")
            if diagnostic["level"] != "warning":
                continue
            lint = (diagnostic.get("code") or {}).get("code", "")
            manifest = Path(event["manifest_path"])
            names = [span["file_name"] for span in diagnostic["spans"] if span["is_primary"]]
            for name in names or [manifest]:
                path = (manifest.parent / name).resolve()
                if not path.exists() and (root / name).exists():
                    path = (root / name).resolve()
                if path.is_relative_to(root):
                    path = path.relative_to(root)
                result.add((lint, path.as_posix(), diagnostic["message"]))
    if not complete:
        raise ValueError(f"{log.name}: missing successful cargo build-finished event")
    return result


def main():
    root = Path(__file__).resolve().parents[1]
    baseline, candidate = (warnings(Path(arg), root) for arg in sys.argv[1:])
    added = candidate - baseline
    knowledge = {row for row in candidate if row[1].startswith("src-tauri/src/knowledge/")}
    print(f"baseline={len(baseline)} candidate={len(candidate)} "
          f"added={len(added)} knowledge={len(knowledge)}")
    for row in sorted(added | knowledge):
        print(json.dumps(row, ensure_ascii=False))
    return bool(added or knowledge)


if __name__ == "__main__":
    sys.exit(main())
