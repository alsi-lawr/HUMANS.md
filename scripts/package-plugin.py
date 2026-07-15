#!/usr/bin/env python3
"""Build or check deterministic plugin packages from portable manifests."""
from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import tempfile
import tomllib
from dataclasses import dataclass
from pathlib import Path, PurePosixPath


SEMVER = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$")
TEXT_SUFFIXES = {".json", ".md", ".py", ".toml", ".txt", ".yaml", ".yml", ".in"}
TRANSIENT_DIRECTORY_NAMES = {"__pycache__"}
TRANSIENT_FILE_SUFFIXES = {".pyc", ".pyo"}


class PackageError(ValueError):
    pass


@dataclass(frozen=True)
class FileSpec:
    data: bytes
    mode: int


def safe_relative(value: object, field: str) -> Path:
    if not isinstance(value, str) or not value:
        raise PackageError(f"{field} must be a non-empty relative path")
    pure = PurePosixPath(value)
    if pure.is_absolute() or ".." in pure.parts or "." in pure.parts or "\\" in value:
        raise PackageError(f"unsafe {field}: {value!r}")
    return Path(*pure.parts)


def read_manifest(path: Path, root: Path) -> dict:
    try:
        document = tomllib.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, tomllib.TOMLDecodeError) as error:
        raise PackageError(f"invalid manifest {path}: {error}") from error
    required = {
        "schema_version",
        "plugin_id",
        "name",
        "vendor",
        "version",
        "publisher",
        "repository",
        "license",
        "description",
        "output",
        "copy",
    }
    missing = required - document.keys()
    if missing:
        raise PackageError(f"manifest missing keys: {', '.join(sorted(missing))}")
    if document["schema_version"] != 1:
        raise PackageError("unsupported manifest schema")
    if document["vendor"] not in {"codex", "claude"}:
        raise PackageError(f"unsupported vendor: {document['vendor']}")
    if document["name"] != "humans-md" or not SEMVER.fullmatch(document["version"]):
        raise PackageError("invalid package identity or version")
    if document["publisher"] != "alsi-lawr" or document["repository"] != "alsi-lawr/HUMANS.md" or document["license"] != "MIT":
        raise PackageError("manifest identity does not match the product contract")
    output = safe_relative(document["output"], "output")
    expected = Path("plugins") / document["vendor"] / document["name"]
    if output != expected:
        raise PackageError(f"output must be {expected.as_posix()}")
    return document


def walk_source(root: Path, source: Path) -> list[Path]:
    absolute = root / source
    if not absolute.exists():
        raise PackageError(f"missing source: {source.as_posix()}")
    if absolute.is_symlink():
        raise PackageError(f"symlink source is forbidden: {source.as_posix()}")
    if absolute.is_file():
        return [absolute]
    if not absolute.is_dir():
        raise PackageError(f"unsupported source type: {source.as_posix()}")
    files: list[Path] = []
    for current, directories, names in os.walk(absolute, followlinks=False):
        current_path = Path(current)
        for name in directories:
            if (current_path / name).is_symlink():
                raise PackageError(f"symlink directory is forbidden: {(current_path / name).relative_to(root)}")
        directories[:] = [name for name in directories if name not in TRANSIENT_DIRECTORY_NAMES]
        for name in names:
            candidate = current_path / name
            if candidate.is_symlink():
                raise PackageError(f"symlink file is forbidden: {candidate.relative_to(root)}")
            if not candidate.is_file():
                raise PackageError(f"unsupported source entry: {candidate.relative_to(root)}")
            if candidate.suffix.lower() in TRANSIENT_FILE_SUFFIXES:
                continue
            files.append(candidate)
    if not files:
        raise PackageError(f"empty source directory: {source.as_posix()}")
    return sorted(files, key=lambda item: item.relative_to(absolute).as_posix())


def metadata(document: dict) -> dict[Path, FileSpec]:
    description = document["description"]
    common = {
        "name": document["name"],
        "version": document["version"],
        "description": description,
    }
    if document["vendor"] == "codex":
        plugin = common
        marketplace = {
            "name": "humans-md",
            "plugins": [
                {
                    "name": "humans-md",
                    "description": description,
                    "source": ".",
                }
            ],
        }
        values = {
            Path(".codex-plugin/plugin.json"): plugin,
            Path(".agents/plugins/marketplace.json"): marketplace,
        }
    else:
        plugin = {
            **common,
            "author": {"name": document["publisher"]},
            "repository": f"https://github.com/{document['repository']}",
            "license": document["license"],
        }
        values = {Path(".claude-plugin/plugin.json"): plugin}
    return {
        path: FileSpec(
            (json.dumps(value, indent=2, sort_keys=True, ensure_ascii=True) + "\n").encode("ascii"),
            0o644,
        )
        for path, value in values.items()
    }


def expected_files(root: Path, document: dict) -> dict[Path, FileSpec]:
    files = metadata(document)
    for item in document["copy"]:
        if not isinstance(item, dict):
            raise PackageError("copy entries must be tables")
        source = safe_relative(item.get("source"), "copy.source")
        destination = safe_relative(item.get("destination"), "copy.destination")
        source_root = root / source
        candidates = walk_source(root, source)
        for candidate in candidates:
            relative = Path() if source_root.is_file() else candidate.relative_to(source_root)
            target = destination / relative
            if target in files:
                raise PackageError(f"duplicate destination: {target.as_posix()}")
            data = candidate.read_bytes()
            if not data:
                raise PackageError(f"empty source file: {candidate.relative_to(root)}")
            if candidate.suffix.lower() in TEXT_SUFFIXES or candidate.name in {"LICENSE"}:
                try:
                    data.decode("ascii")
                except UnicodeDecodeError as error:
                    raise PackageError(f"non-ASCII source: {candidate.relative_to(root)}") from error
            executable = bool(candidate.stat().st_mode & 0o111)
            files[target] = FileSpec(data, 0o755 if executable else 0o644)
    return dict(sorted(files.items(), key=lambda item: item[0].as_posix()))


def actual_files(output: Path) -> dict[Path, FileSpec]:
    if not output.is_dir() or output.is_symlink():
        raise PackageError(f"generated package is missing or unsafe: {output}")
    files: dict[Path, FileSpec] = {}
    for current, directories, names in os.walk(output, followlinks=False):
        current_path = Path(current)
        for name in directories:
            if (current_path / name).is_symlink():
                raise PackageError(f"generated package contains symlink: {current_path / name}")
        for name in names:
            path = current_path / name
            if path.is_symlink() or not path.is_file():
                raise PackageError(f"generated package contains unsafe entry: {path}")
            relative = path.relative_to(output)
            mode = 0o755 if path.stat().st_mode & 0o111 else 0o644
            files[relative] = FileSpec(path.read_bytes(), mode)
    return dict(sorted(files.items(), key=lambda item: item[0].as_posix()))


def compare(expected: dict[Path, FileSpec], actual: dict[Path, FileSpec]) -> list[str]:
    errors = [f"missing generated file: {path}" for path in sorted(expected.keys() - actual.keys())]
    errors += [f"stale generated file: {path}" for path in sorted(actual.keys() - expected.keys())]
    for path in sorted(expected.keys() & actual.keys()):
        if expected[path].data != actual[path].data:
            errors.append(f"byte mismatch: {path}")
        if expected[path].mode != actual[path].mode:
            errors.append(f"mode mismatch: {path}")
    return errors


def build(output: Path, files: dict[Path, FileSpec]) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    staging = Path(tempfile.mkdtemp(prefix=f".{output.name}.build-", dir=output.parent))
    previous: Path | None = None
    try:
        for relative, spec in files.items():
            target = staging / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(spec.data)
            target.chmod(spec.mode)
        if compare(files, actual_files(staging)):
            raise PackageError("staging verification failed")
        if output.exists():
            previous = Path(tempfile.mkdtemp(prefix=f".{output.name}.old-", dir=output.parent))
            previous.rmdir()
            os.replace(output, previous)
        os.replace(staging, output)
        if previous is not None:
            shutil.rmtree(previous)
    except BaseException:
        if staging.exists():
            shutil.rmtree(staging)
        if previous is not None and previous.exists() and not output.exists():
            os.replace(previous, output)
        raise


def manifests(arguments: argparse.Namespace, root: Path) -> list[Path]:
    if arguments.all:
        paths = sorted((root / "packaging/plugins").glob("*.toml"))
        if not paths:
            raise PackageError("no plugin manifests found")
        return paths
    if arguments.manifest is None:
        raise PackageError("pass --all or --manifest")
    return [arguments.manifest.resolve(strict=True)]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("build", "check"))
    selection = parser.add_mutually_exclusive_group(required=True)
    selection.add_argument("--all", action="store_true")
    selection.add_argument("--manifest", type=Path)
    arguments = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    try:
        selected = manifests(arguments, root)
        for manifest_path in selected:
            document = read_manifest(manifest_path, root)
            files = expected_files(root, document)
            output = root / safe_relative(document["output"], "output")
            if arguments.command == "build":
                build(output, files)
                print(f"built {document['plugin_id']}: {len(files)} files")
            else:
                errors = compare(files, actual_files(output))
                if errors:
                    raise PackageError("\n".join(errors))
                print(f"checked {document['plugin_id']}: {len(files)} files")
    except (OSError, PackageError, UnicodeError, tomllib.TOMLDecodeError) as error:
        print(f"package {arguments.command} failed: {error}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
