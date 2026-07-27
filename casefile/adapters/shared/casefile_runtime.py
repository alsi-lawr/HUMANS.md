"""Verified selection and probing for a bundled Casefile executable."""
from __future__ import annotations

import hashlib
import json
import os
import platform
import re
import shutil
import struct
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
    "snapshot", "query_tickets", "query_epics", "query_boards", "query_progress",
    "query_strategy_transitions", "preview_record_draft", "apply_record_draft",
    "bootstrap_progress", "preview_progress", "apply_progress",
    "preview_default_delivery_board", "apply_default_delivery_board",
    "preview_strategy_transition", "apply_strategy_transition", "preview_writer_binding",
    "apply_writer_binding",
}


class RuntimeError(ValueError):
    pass


def canonical(value: object) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True, ensure_ascii=True) + "\n").encode("ascii")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def executable_name(target: str) -> str:
    return "casefile.exe" if target.endswith("windows-msvc") else "casefile"


def artifact_path(target: str) -> str:
    return f"bin/{target}/{executable_name(target)}"


def validate_native_executable(path: Path, target: str) -> None:
    data = path.read_bytes()
    if target.endswith("linux-musl"):
        if len(data) < 20 or data[:4] != b"\x7fELF" or data[4:6] != b"\x02\x01":
            raise RuntimeError(f"wrong executable format for {target}: expected 64-bit little-endian ELF")
        expected = 183 if target.startswith("aarch64") else 62
        if struct.unpack_from("<H", data, 18)[0] != expected:
            raise RuntimeError(f"wrong executable architecture for {target}")
    elif target.endswith("apple-darwin"):
        if len(data) < 8 or data[:4] not in {b"\xfe\xed\xfa\xcf", b"\xcf\xfa\xed\xfe"}:
            raise RuntimeError(f"wrong executable format for {target}: expected 64-bit Mach-O")
        endian = ">" if data[:4] == b"\xfe\xed\xfa\xcf" else "<"
        expected = 0x0100000C if target.startswith("aarch64") else 0x01000007
        if struct.unpack_from(f"{endian}I", data, 4)[0] != expected:
            raise RuntimeError(f"wrong executable architecture for {target}")
    else:
        if len(data) < 64 or data[:2] != b"MZ":
            raise RuntimeError(f"wrong executable format for {target}: expected PE")
        offset = struct.unpack_from("<I", data, 0x3C)[0]
        expected = 0xAA64 if target.startswith("aarch64") else 0x8664
        if (
            len(data) < offset + 6
            or data[offset : offset + 4] != b"PE\0\0"
            or struct.unpack_from("<H", data, offset + 4)[0] != expected
        ):
            raise RuntimeError(f"wrong executable architecture for {target}")


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
        raw = (runtime / "artifacts.json").read_bytes()
        raw.decode("ascii")
        manifest = json.loads(raw)
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise RuntimeError(f"invalid Casefile artifact manifest: {error}") from error
    if (
        canonical(manifest) != raw
        or set(manifest) != {"schema_version", "version", "source_commit", "artifacts"}
        or manifest.get("schema_version") != 1
        or not isinstance(manifest.get("source_commit"), str)
        or re.fullmatch(r"[0-9a-f]{40}", manifest["source_commit"]) is None
    ):
        raise RuntimeError("Casefile artifact manifest is not canonical schema 1")
    if manifest.get("version") != version:
        raise RuntimeError("Casefile artifact version differs from plugin version")
    rows = manifest.get("artifacts")
    if not isinstance(rows, list) or len(rows) != 6:
        raise RuntimeError("Casefile artifact matrix is incomplete")
    if [row.get("target") if isinstance(row, dict) else None for row in rows] != sorted(MATRIX):
        raise RuntimeError("Casefile artifact matrix must use the complete sorted target order")
    by_target = {}
    for row in rows:
        if not isinstance(row, dict) or set(row) != {"path", "sha256", "size", "target"}:
            raise RuntimeError("Casefile artifact entry is invalid")
        row_target = row.get("target")
        if row_target not in MATRIX or row_target in by_target:
            raise RuntimeError("Casefile artifact matrix has duplicate or unsupported targets")
        relative = row.get("path")
        if relative != artifact_path(row_target):
            raise RuntimeError(f"Casefile artifact path is invalid for {row_target}")
        pure = PurePosixPath(relative)
        if pure.is_absolute() or ".." in pure.parts or "\\" in relative:
            raise RuntimeError("Casefile artifact path is unsafe")
        candidate = runtime / Path(*pure.parts)
        if candidate.is_symlink() or not candidate.is_file() or candidate.stat().st_size != row.get("size"):
            raise RuntimeError("Casefile artifact is missing, unsafe, or has the wrong size")
        if sha256(candidate) != row.get("sha256"):
            raise RuntimeError("Casefile artifact hash mismatch")
        validate_native_executable(candidate, row_target)
        by_target[row_target] = (row, candidate)
    if set(by_target) != MATRIX:
        raise RuntimeError("Casefile artifact matrix is incomplete")
    target = target or host_target()
    selected_pair = by_target.get(target)
    selected = selected_pair[0] if selected_pair else None
    if selected is None:
        raise RuntimeError(f"Casefile artifact matrix lacks host target {target}")
    expected_files = {
        Path("artifacts.json"),
        *(Path(*PurePosixPath(artifact_path(item)).parts) for item in MATRIX),
    }
    actual_files = {
        path.relative_to(runtime)
        for path in runtime.rglob("*")
        if path.is_file() or path.is_symlink()
    }
    if actual_files != expected_files:
        raise RuntimeError("Casefile artifact root inventory is incomplete or contains extra files")
    source = selected_pair[1]
    return {"target": target, "source": source, "sha256": selected["sha256"], "manifest": manifest}


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
    if compatibility.returncode or contract.get("identity") != "casefile" or contract.get("provider_protocol_version") != 1:
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
