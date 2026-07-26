#!/usr/bin/env python3
"""Preview and explicitly apply the sole supported Casefile progress mutation workflow."""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tempfile
from datetime import UTC, datetime
from pathlib import Path


STATUSES = {"unknown", "in_progress", "in_review", "verifying", "blocked", "complete"}
CATEGORIES = {"deviation", "quirk"}


def timestamp(value: str | None) -> str:
    return value or datetime.now(UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def invoke(casefile: str, root: Path, command: list[str], payload: object | None = None) -> dict:
    temporary: tempfile.NamedTemporaryFile[str] | None = None
    try:
        if payload is not None:
            temporary = tempfile.NamedTemporaryFile("w", encoding="utf-8", suffix=".json", delete=False)
            json.dump(payload, temporary, indent=2)
            temporary.write("\n")
            temporary.close()
            command += ["--request", temporary.name]
        result = subprocess.run([casefile, "--root", str(root), *command], text=True, capture_output=True, check=False)
        if result.returncode:
            raise ValueError((result.stderr or result.stdout).strip() or "canonical Casefile command failed")
        return json.loads(result.stdout)
    finally:
        if temporary is not None:
            Path(temporary.name).unlink(missing_ok=True)


def request_from(args: argparse.Namespace) -> dict:
    recorded_at = timestamp(args.recorded_at)
    if args.action == "transition":
        return {"investigation": args.investigation, "entries": [{
            "kind": "transition", "id": args.operation_id, "recorded_at": recorded_at,
            "recorded_by": args.recorded_by, "ticket_id": args.ticket, "from": args.from_status, "to": args.to,
        }]}
    if args.action == "note":
        return {"investigation": args.investigation, "entries": [{
            "kind": "note", "id": args.operation_id, "recorded_at": recorded_at,
            "recorded_by": args.recorded_by, "ticket_id": args.ticket, "category": args.category, "message": args.message,
        }]}
    if args.action == "replace":
        # Read bytes only; the canonical Rust parser owns TOML validation and rendering.
        return {"investigation": args.investigation, "entries": [], "replacement_source": args.replacement.read_text(encoding="utf-8")}
    raise AssertionError(args.action)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", required=True, type=Path)
    parser.add_argument("--casefile", default="casefile")
    parser.add_argument("--preview-file", required=True, type=Path, help="immutable canonical preview JSON")
    parser.add_argument("--apply", action="store_true", help="apply the existing preview after validating it")
    subparsers = parser.add_subparsers(dest="action", required=True)
    common = argparse.ArgumentParser(add_help=False)
    common.add_argument("--investigation", required=True)
    common.add_argument("--recorded-by", required=True)
    common.add_argument("--recorded-at")
    transition = subparsers.add_parser("transition", parents=[common])
    transition.add_argument("--ticket", required=True)
    transition.add_argument("--from", dest="from_status", required=True, choices=sorted(STATUSES))
    transition.add_argument("--to", required=True, choices=sorted(STATUSES))
    transition.add_argument("--operation-id", required=True)
    note = subparsers.add_parser("note", parents=[common])
    note.add_argument("--ticket", required=True)
    note.add_argument("--category", required=True, choices=sorted(CATEGORIES))
    note.add_argument("--message", required=True)
    note.add_argument("--operation-id", required=True)
    subparsers.add_parser("bootstrap-unknown").add_argument("--investigation", required=True)
    replace = subparsers.add_parser("replace", parents=[common])
    replace.add_argument("--replacement", required=True, type=Path)
    args = parser.parse_args()
    if args.preview_file.resolve().is_relative_to(args.root.resolve()):
        raise ValueError("--preview-file must be outside --root so it cannot change the saved Store revision")

    if args.apply:
        if not args.preview_file.is_file():
            raise ValueError("--apply requires the immutable --preview-file created by a prior preview")
        preview = json.loads(args.preview_file.read_text(encoding="utf-8"))
        with tempfile.NamedTemporaryFile("w", encoding="utf-8", suffix=".json") as handle:
            json.dump(preview, handle)
            handle.flush()
            result = invoke(args.casefile, args.root, ["progress-apply", "--preview", handle.name])
        print(json.dumps(result, indent=2))
        return 0

    if args.action == "bootstrap-unknown":
        request = invoke(args.casefile, args.root, ["progress-bootstrap", "--investigation", args.investigation])
    else:
        request = request_from(args)
    preview = invoke(args.casefile, args.root, ["progress-preview"], request)
    args.preview_file.parent.mkdir(parents=True, exist_ok=True)
    args.preview_file.write_text(json.dumps(preview, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(preview, indent=2))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"transition-ticket-progress: {error}", file=sys.stderr)
        raise SystemExit(2)
