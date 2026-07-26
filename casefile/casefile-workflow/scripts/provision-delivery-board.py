#!/usr/bin/env python3
"""Preview and explicitly apply the canonical Casefile delivery board."""
from __future__ import annotations

import argparse
import json
import os
import stat
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path


COLUMNS = (
    ("Unknown", "unknown"),
    ("In progress", "in_progress"),
    ("In review", "in_review"),
    ("Verifying", "verifying"),
    ("Blocked", "blocked"),
    ("Complete", "complete"),
)


class DifferentTarget(ValueError):
    def __init__(self, preview: dict):
        super().__init__("delivery.toml already differs; the existing board was preserved")
        self.preview = preview


def invoke(casefile: str, root: Path, command: list[str], payload: object | None = None) -> dict:
    temporary: tempfile.NamedTemporaryFile[str] | None = None
    try:
        if payload is not None:
            temporary = tempfile.NamedTemporaryFile("w", encoding="utf-8", suffix=".json", delete=False)
            json.dump(payload, temporary, indent=2)
            temporary.write("\n")
            temporary.close()
            command += ["--request", temporary.name]
        result = subprocess.run(
            [casefile, "--root", str(root), *command],
            text=True,
            capture_output=True,
            check=False,
        )
        if result.returncode:
            raise ValueError((result.stderr or result.stdout).strip() or "canonical Casefile command failed")
        return json.loads(result.stdout)
    finally:
        if temporary is not None:
            Path(temporary.name).unlink(missing_ok=True)


def board_identity(root: Path, investigation: str) -> str:
    document = tomllib.loads((root / "casefile.toml").read_text(encoding="utf-8"))
    projects = document.get("projects")
    if not isinstance(projects, dict):
        raise ValueError("activation must contain project mappings")
    matches: list[str] = []
    identity_counts: dict[str, int] = {}
    for project in projects.values():
        prefix = project.get("prefix") if isinstance(project, dict) else None
        investigations = project.get("investigations") if isinstance(project, dict) else None
        if not isinstance(prefix, str) or not isinstance(investigations, list):
            raise ValueError("activation project mappings must contain a prefix and investigations")
        for activated in investigations:
            if not isinstance(activated, str):
                raise ValueError("activated investigation mappings must be strings")
            identity = identity_for_mapping(prefix, activated)
            identity_counts[identity] = identity_counts.get(identity, 0) + 1
            if activated == investigation:
                matches.append(identity)
    if len(matches) != 1:
        raise ValueError("investigation must have exactly one activated project-prefix mapping")
    identity = matches[0]
    if identity_counts[identity] != 1:
        raise ValueError("default board identity must map to exactly one activated investigation")
    return identity


def identity_for_mapping(prefix: str, mapped_investigation: str) -> str:
    directory_name = mapped_investigation.rsplit("/", 1)[-1]
    if not directory_name or directory_name in {".", ".."}:
        raise ValueError("activated investigation must have a safe directory name")
    return f"{prefix}-{directory_name}-delivery"


def request_for(path: str, identity: str, operation: str) -> dict:
    return {
        "operation": operation,
        "path": path,
        "draft": {
            "kind": "board",
            "id": identity,
            "title": "Delivery",
            "status_source": "progress",
            "filter_statuses": None,
            "filter_kinds": ["ticket"],
            "columns": [
                {"name": name, "statuses": [status]}
                for name, status in COLUMNS
            ],
        },
    }


def planning_root(path: Path) -> Path:
    root = Path(os.path.abspath(path))
    try:
        metadata = root.lstat()
    except FileNotFoundError as error:
        raise ValueError("planning root must be an existing directory") from error
    if stat.S_ISLNK(metadata.st_mode):
        raise ValueError("planning root must not be a symlink")
    if not stat.S_ISDIR(metadata.st_mode):
        raise ValueError("planning root must be an existing directory")
    return root


def require_directory_ancestors(root: Path, path: str) -> None:
    parent = Path(path).parent
    if parent.is_absolute() or ".." in parent.parts:
        raise ValueError("delivery board path must remain beneath the planning root")
    current = root
    for component in parent.parts:
        current /= component
        try:
            metadata = current.lstat()
        except FileNotFoundError:
            break
        if stat.S_ISLNK(metadata.st_mode):
            raise ValueError("delivery board path ancestors must not be symlinks")
        if not stat.S_ISDIR(metadata.st_mode):
            raise ValueError("delivery board path ancestors must be directories")


def target_operation(root: Path, path: str) -> str:
    target = root / path
    try:
        target.lstat()
    except FileNotFoundError:
        return "create"
    if target.is_symlink():
        raise ValueError("delivery.toml must not be a symlink")
    if not target.is_file():
        raise ValueError("delivery.toml must be a regular file or absent")
    return "replace"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", required=True, type=Path)
    parser.add_argument("--casefile", default="casefile")
    parser.add_argument("--preview-file", required=True, type=Path, help="immutable canonical preview JSON")
    parser.add_argument("--investigation", required=True)
    parser.add_argument("--apply", action="store_true", help="apply the existing preview after validating it")
    args = parser.parse_args()
    root = planning_root(args.root)
    if args.preview_file.resolve().is_relative_to(root):
        raise ValueError("--preview-file must be outside --root so it cannot change the saved Store revision")

    if args.apply:
        if not args.preview_file.is_file():
            raise ValueError("--apply requires the immutable --preview-file created by a prior preview")
        preview = json.loads(args.preview_file.read_text(encoding="utf-8"))
        investigation = args.investigation.rstrip("/")
        request = preview.get("request")
        operation = request.get("operation") if isinstance(request, dict) else None
        expected_path = f"{investigation}/boards/delivery.toml"
        if operation not in {"create", "replace"}:
            raise ValueError("saved preview is not a delivery-board create or replace")
        expected = request_for(expected_path, board_identity(root, investigation), operation)
        if request != expected or preview.get("diagnostics"):
            raise ValueError("saved preview is not the canonical delivery-board preview")
        require_directory_ancestors(root, expected_path)
        no_op = not preview.get("diff")
        if no_op:
            current = invoke(args.casefile, root, ["preview"], request)
            if (
                current.get("diagnostics")
                or current.get("diff")
                or current.get("expected_store_revision") != preview.get("expected_store_revision")
                or current.get("expected_target_revision") != preview.get("expected_target_revision")
            ):
                raise ValueError("saved delivery-board preview is stale")
            print(json.dumps({
                "path": expected_path,
                "resulting_target_revision": current.get("expected_target_revision"),
                "resulting_store_revision": current.get("expected_store_revision"),
                "diff": "",
                "no_op": True,
            }, indent=2))
            return 0
        with tempfile.NamedTemporaryFile("w", encoding="utf-8", suffix=".json") as handle:
            json.dump(preview, handle)
            handle.flush()
            result = invoke(args.casefile, root, ["apply", "--preview", handle.name])
        result["no_op"] = False
        print(json.dumps(result, indent=2))
        return 0

    investigation = args.investigation.rstrip("/")
    path = f"{investigation}/boards/delivery.toml"
    require_directory_ancestors(root, path)
    invoke(
        args.casefile,
        root,
        ["check", "--require-activation", "--investigation", investigation],
    )
    identity = board_identity(root, investigation)
    operation = target_operation(root, path)
    preview = invoke(args.casefile, root, ["preview"], request_for(path, identity, operation))
    if preview.get("diagnostics"):
        messages = "; ".join(item.get("message", "invalid board preview") for item in preview["diagnostics"])
        raise ValueError(messages)
    if operation == "replace" and preview.get("diff"):
        raise DifferentTarget(preview)
    args.preview_file.parent.mkdir(parents=True, exist_ok=True)
    args.preview_file.write_text(json.dumps(preview, indent=2) + "\n", encoding="utf-8")
    output = dict(preview)
    output["no_op"] = not preview.get("diff")
    print(json.dumps(output, indent=2))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except DifferentTarget as error:
        print(json.dumps(error.preview, indent=2))
        print(f"provision-delivery-board: {error}", file=sys.stderr)
        raise SystemExit(2)
    except (OSError, ValueError, json.JSONDecodeError, tomllib.TOMLDecodeError) as error:
        print(f"provision-delivery-board: {error}", file=sys.stderr)
        raise SystemExit(2)
