"""Verified selection and probing for a bundled Casefile executable."""
from __future__ import annotations

import json
import os
import platform
import re
import shutil
import subprocess
import tempfile
from pathlib import Path, PurePosixPath


TARGETS = {
    ("Linux", "x86_64"): "x86_64-unknown-linux-musl",
    ("Linux", "aarch64"): "aarch64-unknown-linux-musl",
    ("Darwin", "x86_64"): "x86_64-apple-darwin",
    ("Darwin", "arm64"): "aarch64-apple-darwin",
    ("Windows", "AMD64"): "x86_64-pc-windows-msvc",
    ("Windows", "ARM64"): "aarch64-pc-windows-msvc",
}
MATRIX = set(TARGETS.values())
REQUIRED_OPERATIONS = {
    "snapshot", "record_index", "record_detail", "boards", "strategy_transitions",
    "preview_record_draft", "apply_record_draft",
    "bootstrap_progress", "preview_progress", "apply_progress",
    "preview_default_delivery_board", "apply_default_delivery_board",
    "preview_strategy_transition", "apply_strategy_transition", "preview_writer_binding",
    "apply_writer_binding",
}


class RuntimeError(ValueError):
    pass


def canonical(value: object) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True, ensure_ascii=True) + "\n").encode("ascii")


def executable_name(target: str) -> str:
    return "casefile.exe" if target.endswith("windows-msvc") else "casefile"


def artifact_path(target: str) -> str:
    return f"bin/{target}/{executable_name(target)}"


def normalized_artifact_path(value: object, target: str) -> Path:
    if not isinstance(value, str) or not value or "\0" in value:
        raise RuntimeError(f"Casefile artifact path is invalid for {target}")
    if value.startswith(("/", "\\")) or re.match(r"^[A-Za-z]:", value):
        raise RuntimeError("Casefile artifact path is unsafe")
    parts = [part for part in value.replace("\\", "/").split("/") if part]
    if not parts or any(part in {".", ".."} for part in parts):
        raise RuntimeError("Casefile artifact path is unsafe")
    if PurePosixPath(*parts).as_posix() != artifact_path(target):
        raise RuntimeError(f"Casefile artifact path is invalid for {target}")
    return Path(*parts)


def require_landed(path: Path, root: Path | None = None) -> Path:
    try:
        resolved = path.resolve(strict=True)
        if root is not None:
            resolved.relative_to(root.resolve(strict=True))
    except (OSError, ValueError) as error:
        raise RuntimeError(f"Casefile artifact did not land at {path}") from error
    if path.is_symlink() or not path.is_file() or path.stat().st_size <= 0:
        raise RuntimeError(f"Casefile artifact did not land at {path}")
    return path


def host_target(system: str | None = None, machine: str | None = None) -> str:
    system = system or platform.system()
    machine = machine or platform.machine()
    normalized = machine.lower()
    if normalized in {"amd64", "x86_64"}:
        machine = "AMD64" if system == "Windows" else "x86_64"
    elif normalized in {"arm64", "aarch64"}:
        machine = "ARM64" if system == "Windows" else ("arm64" if system == "Darwin" else "aarch64")
    target = TARGETS.get((system, machine))
    if target is None:
        raise RuntimeError(f"unsupported Casefile host: {system}/{machine}")
    return target


def select(plugin_root: Path, version: str, target: str | None = None) -> dict:
    runtime = plugin_root / "runtime"
    try:
        manifest_path = runtime / "artifacts.json"
        if manifest_path.is_symlink() or not manifest_path.is_file() or manifest_path.stat().st_size <= 0:
            raise RuntimeError("Casefile artifact manifest did not land")
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise RuntimeError(f"invalid Casefile artifact manifest: {error}") from error
    if (
        not isinstance(manifest, dict)
        or not {"schema_version", "version", "source_commit", "artifacts"}.issubset(manifest)
        or manifest.get("schema_version") != 1
        or not isinstance(manifest.get("source_commit"), str)
        or re.fullmatch(r"[0-9a-f]{40}", manifest["source_commit"]) is None
    ):
        raise RuntimeError("Casefile artifact manifest is not complete schema 1")
    if manifest.get("version") != version:
        raise RuntimeError("Casefile artifact version differs from plugin version")
    rows = manifest.get("artifacts")
    if not isinstance(rows, list) or len(rows) != 6:
        raise RuntimeError("Casefile artifact matrix is incomplete")
    by_target = {}
    for row in rows:
        if not isinstance(row, dict) or not {"path", "target"}.issubset(row):
            raise RuntimeError("Casefile artifact entry is invalid")
        row_target = row.get("target")
        if row_target not in MATRIX or row_target in by_target:
            raise RuntimeError("Casefile artifact matrix has duplicate or unsupported targets")
        relative = normalized_artifact_path(row.get("path"), row_target)
        candidate = require_landed(runtime / relative, runtime)
        by_target[row_target] = (row, candidate)
    if set(by_target) != MATRIX:
        raise RuntimeError("Casefile artifact matrix is incomplete")
    target = target or host_target()
    selected_pair = by_target.get(target)
    selected = selected_pair[0] if selected_pair else None
    if selected is None:
        raise RuntimeError(f"Casefile artifact matrix lacks host target {target}")
    source = selected_pair[1]
    return {"target": target, "source": source, "manifest": manifest}


def destination(home: Path, version: str, target: str) -> Path:
    name = "casefile.exe" if target.endswith("windows-msvc") else "casefile"
    return home / "casefile" / "runtime" / version / target / name


def atomic_copy(source: Path, target: Path) -> None:
    target.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix=f".{target.name}.", dir=target.parent)
    temporary_path = Path(temporary)
    os.close(descriptor)
    try:
        shutil.copyfile(source, temporary_path)
        if os.name == "posix":
            temporary_path.chmod(0o755)
        os.replace(temporary_path, target)
    except BaseException:
        temporary_path.unlink(missing_ok=True)
        raise


def planning_root(path: Path) -> Path:
    if not path.expanduser().is_absolute():
        raise RuntimeError("planning root must be absolute")
    root = path.expanduser().resolve(strict=True)
    if not root.is_dir():
        raise RuntimeError("planning root must be a directory")
    return root


def probe(binary: Path, version: str, root: Path) -> None:
    version_result = subprocess.run([binary, "--version"], capture_output=True, text=True, timeout=15)
    if version_result.returncode or version not in version_result.stdout:
        raise RuntimeError("Casefile executable version probe failed")
    compatibility = subprocess.run(
        [binary, "mcp-compatibility"], capture_output=True, text=True, timeout=15
    )
    try:
        contract = json.loads(compatibility.stdout)
    except json.JSONDecodeError as error:
        raise RuntimeError("Casefile executable returned invalid compatibility JSON") from error
    if compatibility.returncode or contract.get("identity") != "casefile" or contract.get("provider_protocol_version") != 2:
        raise RuntimeError("Casefile executable compatibility probe failed")
    if set(contract.get("required_provider_operations", [])) != REQUIRED_OPERATIONS:
        raise RuntimeError("Casefile executable capability contract is incomplete")
    requests = "\n".join(json.dumps(value, separators=(",", ":")) for value in (
        {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}},
        {"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}},
    )) + "\n"
    process = subprocess.run(
        [binary, "mcp-package", "--planning-root", root], input=requests,
        capture_output=True, text=True, timeout=30,
    )
    try:
        responses = [json.loads(line) for line in process.stdout.splitlines() if line]
    except json.JSONDecodeError as error:
        raise RuntimeError("Casefile MCP probe returned invalid JSON") from error
    if process.returncode or len(responses) != 2:
        raise RuntimeError("Casefile MCP probe failed")
    if responses[0].get("result", {}).get("serverInfo", {}).get("name") != "casefile":
        raise RuntimeError("Casefile MCP identity probe failed")
    if len(responses[1].get("result", {}).get("tools", [])) != 12:
        raise RuntimeError("Casefile MCP tool probe failed")
