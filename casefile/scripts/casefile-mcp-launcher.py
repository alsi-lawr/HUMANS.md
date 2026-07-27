#!/usr/bin/env python3
"""Launch the packaged fixed-root Casefile stdio MCP adapter from locked source."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path


IDENTITY = "casefile"
ADAPTER_PROTOCOL_VERSION = 1
PROVIDER_PROTOCOL_VERSION = 1
REQUIRED_PROVIDER_OPERATIONS = (
    "snapshot",
    "query_tickets",
    "query_epics",
    "query_boards",
    "query_progress",
    "query_strategy_transitions",
    "preview_record_draft",
    "apply_record_draft",
    "bootstrap_progress",
    "preview_progress",
    "apply_progress",
    "preview_default_delivery_board",
    "apply_default_delivery_board",
    "preview_strategy_transition",
    "apply_strategy_transition",
    "preview_writer_binding",
    "apply_writer_binding",
)


class LaunchError(RuntimeError):
    pass


def canonical_directory(raw: str, label: str) -> Path:
    if not raw or not Path(raw).is_absolute():
        raise LaunchError(f"{label} must be one non-empty absolute path")
    try:
        path = Path(raw).resolve(strict=True)
    except OSError as error:
        raise LaunchError(f"{label} cannot be resolved: {error}") from error
    if not path.is_dir():
        raise LaunchError(f"{label} is not a directory: {path}")
    return path


def validate_planning_root(raw: str) -> Path:
    root = canonical_directory(raw, "planning root")
    activation = root / "casefile.toml"
    if not activation.is_file() or activation.is_symlink():
        raise LaunchError(
            "planning root is not an activated Casefile Store; expected a regular casefile.toml"
        )
    return root


def plugin_root() -> Path:
    root = Path(__file__).resolve(strict=True).parent.parent
    manifests = (
        root / ".codex-plugin" / "plugin.json",
        root / ".claude-plugin" / "plugin.json",
    )
    present = [path for path in manifests if path.is_file()]
    if len(present) != 1:
        raise LaunchError("launcher is not inside one generated Casefile plugin package")
    try:
        metadata = json.loads(present[0].read_text(encoding="ascii"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise LaunchError(f"package metadata is invalid: {error}") from error
    if metadata.get("name") != IDENTITY or not isinstance(metadata.get("version"), str):
        raise LaunchError("package metadata identity is not Casefile")
    return root


def is_within(path: Path, boundary: Path) -> bool:
    return path == boundary or path.is_relative_to(boundary)


def controlled_target(root: Path, planning_root: Path) -> Path:
    try:
        home = Path.home().resolve(strict=True)
    except OSError as error:
        raise LaunchError(f"home directory cannot be resolved for Cargo output: {error}") from error
    lockfile = root / "Cargo.lock"
    try:
        digest = hashlib.sha256(lockfile.read_bytes()).hexdigest()[:16]
    except OSError as error:
        raise LaunchError(f"package lockfile is unavailable: {error}") from error
    target = (home / ".cache" / "casefile" / "mcp-target" / digest).resolve()
    if is_within(target, root) or is_within(target, planning_root):
        raise LaunchError("controlled Cargo output would overlap the plugin or planning root")
    try:
        target.mkdir(parents=True, exist_ok=True, mode=0o700)
        probe = target / f".write-probe-{os.getpid()}"
        probe.write_bytes(b"")
        probe.unlink()
        target = target.resolve(strict=True)
    except OSError as error:
        raise LaunchError(f"controlled Cargo output is not writable: {error}") from error
    if is_within(target, root) or is_within(target, planning_root):
        raise LaunchError("canonical Cargo output overlaps the plugin or planning root")
    return target


def package_workspace(root: Path) -> tuple[Path, Path]:
    manifest = root / "Cargo.toml"
    lockfile = root / "Cargo.lock"
    if not manifest.is_file() or manifest.is_symlink():
        raise LaunchError("package workspace Cargo.toml is missing or unsafe")
    if not lockfile.is_file() or lockfile.is_symlink():
        raise LaunchError("package workspace Cargo.lock is missing or unsafe")
    try:
        source = manifest.read_text(encoding="ascii")
        locked = lockfile.read_bytes()
    except (OSError, UnicodeError) as error:
        raise LaunchError(f"package workspace source is invalid: {error}") from error
    if "casefile-cli" not in source or not locked:
        raise LaunchError("package workspace does not contain the locked Casefile CLI source")
    return manifest, lockfile


def checked(command: list[str], label: str) -> subprocess.CompletedProcess[str]:
    try:
        result = subprocess.run(
            command,
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=300,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise LaunchError(f"{label} could not run: {error}") from error
    if result.returncode:
        raise LaunchError(f"{label} failed ({result.returncode}); run that prerequisite directly for details")
    return result


def compatibility(command: list[str], label: str) -> None:
    result = checked(command + ["mcp-compatibility"], label)
    try:
        value = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise LaunchError("Casefile compatibility probe returned invalid JSON") from error
    expected = {
        "identity": IDENTITY,
        "adapter_protocol_version": ADAPTER_PROTOCOL_VERSION,
        "provider_protocol_version": PROVIDER_PROTOCOL_VERSION,
        "required_provider_operations": list(REQUIRED_PROVIDER_OPERATIONS),
    }
    if not isinstance(value, dict) or any(value.get(key) != item for key, item in expected.items()):
        raise LaunchError("Casefile executable identity, protocol, or capabilities are incompatible")


def executable_override(raw: str) -> Path:
    if not raw or not Path(raw).is_absolute():
        raise LaunchError("external executable override must be an explicit absolute path")
    try:
        path = Path(raw).resolve(strict=True)
    except OSError as error:
        raise LaunchError(f"external executable override cannot be resolved: {error}") from error
    if not path.is_file() or not os.access(path, os.X_OK):
        raise LaunchError("external executable override is not an executable file")
    return path


def tool(name: str) -> str:
    executable = shutil.which(name)
    if executable is None:
        raise LaunchError(f"required Rust tool `{name}` is unavailable; install a compatible Rust toolchain")
    checked([executable, "--version"], f"Rust prerequisite `{name}`")
    return executable


def adapter_arguments(planning_root: Path) -> list[str]:
    return [
        "mcp-stdio",
        "--planning-root",
        str(planning_root),
        "--expected-root",
        str(planning_root),
        "--expected-provider-protocol",
        str(PROVIDER_PROTOCOL_VERSION),
        "--required-provider-operations",
        ",".join(REQUIRED_PROVIDER_OPERATIONS),
    ]


def launch(arguments: argparse.Namespace) -> None:
    planning_root = validate_planning_root(arguments.planning_root)
    root = plugin_root()
    if arguments.external_executable is not None:
        executable = executable_override(arguments.external_executable)
        command = [str(executable)]
        compatibility(command, "external Casefile compatibility probe")
        os.execv(str(executable), command + adapter_arguments(planning_root))
        raise AssertionError("exec returned")

    manifest, _ = package_workspace(root)
    target = controlled_target(root, planning_root)
    cargo = tool("cargo")
    tool("rustc")
    cargo_command = [
        cargo,
        "run",
        "--locked",
        "--quiet",
        "--target-dir",
        str(target),
        "--manifest-path",
        str(manifest),
        "-p",
        "casefile-cli",
        "--",
    ]
    compatibility(
        cargo_command,
        "locked source compatibility build (requires Cargo/Rust and registry or Git cache/network)",
    )
    os.execvpe(cargo, cargo_command + adapter_arguments(planning_root), os.environ.copy())
    raise AssertionError("exec returned")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--planning-root", required=True)
    parser.add_argument("--external-executable")
    arguments = parser.parse_args()
    try:
        launch(arguments)
    except LaunchError as error:
        print(f"Casefile MCP launch refused: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
