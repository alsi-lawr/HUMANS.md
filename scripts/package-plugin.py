#!/usr/bin/env python3
"""Build or check deterministic multi-plugin packages from portable manifests."""
from __future__ import annotations

import argparse
import importlib.util
import json
import os
import re
import shutil
import string
import tempfile
import tomllib
from dataclasses import dataclass
from pathlib import Path, PurePosixPath

try:
    from casefile_artifacts import ArtifactError, load as load_casefile_artifacts
except ModuleNotFoundError:
    _artifact_path = Path(__file__).resolve().with_name("casefile_artifacts.py")
    _artifact_spec = importlib.util.spec_from_file_location("casefile_artifacts", _artifact_path)
    if _artifact_spec is None or _artifact_spec.loader is None:
        raise
    _artifact_module = importlib.util.module_from_spec(_artifact_spec)
    _artifact_spec.loader.exec_module(_artifact_module)
    ArtifactError = _artifact_module.ArtifactError
    load_casefile_artifacts = _artifact_module.load


NAME = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")
SEMVER = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$")
TEXT_SUFFIXES = {".json", ".md", ".py", ".toml", ".txt", ".yaml", ".yml", ".in"}
TRANSIENT_DIRECTORY_NAMES = {"__pycache__"}
TRANSIENT_FILE_SUFFIXES = {".pyc", ".pyo"}
IDENTITY_FIELDS = (
    "name",
    "version",
    "publisher",
    "repository",
    "repository_url",
    "license",
    "description",
)


class PackageError(ValueError):
    pass


@dataclass(frozen=True)
class FileSpec:
    data: bytes
    mode: int


@dataclass(frozen=True)
class PackageSpec:
    manifest: Path
    plugin: str
    vendor: str
    output: Path
    files: dict[Path, FileSpec]


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
    required = {"schema_version", *IDENTITY_FIELDS, "shared", "vendors"}
    missing = required - document.keys()
    if missing:
        raise PackageError(f"manifest missing keys: {', '.join(sorted(missing))}")
    if document["schema_version"] != 1:
        raise PackageError("unsupported manifest schema")
    if not isinstance(document["name"], str) or not NAME.fullmatch(document["name"]):
        raise PackageError("invalid plugin name")
    if not isinstance(document["version"], str) or not SEMVER.fullmatch(document["version"]):
        raise PackageError("invalid plugin version")
    for field in IDENTITY_FIELDS[2:]:
        if not isinstance(document[field], str) or not document[field].strip():
            raise PackageError(f"plugin {field} must be a non-empty string")
    if not isinstance(document["shared"], list):
        raise PackageError("shared resources must be an array")
    vendors = document["vendors"]
    if not isinstance(vendors, dict) or not vendors:
        raise PackageError("manifest must declare at least one vendor adapter")
    for vendor, adapter in vendors.items():
        if not isinstance(vendor, str) or not NAME.fullmatch(vendor) or not isinstance(adapter, dict):
            raise PackageError(f"invalid vendor adapter: {vendor!r}")
        safe_relative(adapter.get("output"), f"vendors.{vendor}.output")
        for field in ("copy", "template"):
            if field in adapter and not isinstance(adapter[field], list):
                raise PackageError(f"vendors.{vendor}.{field} must be an array")
    artifacts = document.get("casefile_artifacts")
    if artifacts is not None:
        if not isinstance(artifacts, dict) or set(artifacts) != {"destination"}:
            raise PackageError("casefile_artifacts must contain only destination")
        safe_relative(artifacts["destination"], "casefile_artifacts.destination")
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
                raise PackageError(
                    f"symlink directory is forbidden: {(current_path / name).relative_to(root)}"
                )
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


def add_file(files: dict[Path, FileSpec], target: Path, spec: FileSpec) -> None:
    if target in files:
        raise PackageError(f"duplicate destination: {target.as_posix()}")
    files[target] = spec


def copy_resources(root: Path, files: dict[Path, FileSpec], entries: list, field: str) -> None:
    for item in entries:
        if not isinstance(item, dict):
            raise PackageError(f"{field} entries must be tables")
        source = safe_relative(item.get("source"), f"{field}.source")
        destination = safe_relative(item.get("destination"), f"{field}.destination")
        source_root = root / source
        for candidate in walk_source(root, source):
            relative = Path() if source_root.is_file() else candidate.relative_to(source_root)
            target = destination / relative
            data = candidate.read_bytes()
            if not data:
                raise PackageError(f"empty source file: {candidate.relative_to(root)}")
            if candidate.suffix.lower() in TEXT_SUFFIXES or candidate.name == "LICENSE":
                try:
                    data.decode("ascii")
                except UnicodeDecodeError as error:
                    raise PackageError(f"non-ASCII source: {candidate.relative_to(root)}") from error
            executable = bool(candidate.stat().st_mode & 0o111)
            add_file(files, target, FileSpec(data, 0o755 if executable else 0o644))


def template_context(document: dict, vendor: str) -> dict[str, str]:
    values = {field: document[field] for field in IDENTITY_FIELDS}
    values["vendor"] = vendor
    context = {key: str(value) for key, value in values.items()}
    context.update(
        {f"{key}_json": json.dumps(value, ensure_ascii=True) for key, value in values.items()}
    )
    return context


def render_templates(
    root: Path,
    files: dict[Path, FileSpec],
    entries: list,
    document: dict,
    vendor: str,
) -> None:
    context = template_context(document, vendor)
    for item in entries:
        if not isinstance(item, dict):
            raise PackageError(f"vendors.{vendor}.template entries must be tables")
        source = safe_relative(item.get("source"), f"vendors.{vendor}.template.source")
        destination = safe_relative(
            item.get("destination"), f"vendors.{vendor}.template.destination"
        )
        candidates = walk_source(root, source)
        if len(candidates) != 1 or not (root / source).is_file():
            raise PackageError(f"template source must be one file: {source.as_posix()}")
        try:
            rendered = string.Template(candidates[0].read_text(encoding="ascii")).substitute(context)
        except (UnicodeError, KeyError, ValueError) as error:
            raise PackageError(f"invalid template {source}: {error}") from error
        format_name = item.get("format", "text")
        if format_name == "json":
            try:
                value = json.loads(rendered)
            except json.JSONDecodeError as error:
                raise PackageError(f"rendered JSON is invalid for {source}: {error}") from error
            data = (json.dumps(value, indent=2, sort_keys=True, ensure_ascii=True) + "\n").encode("ascii")
        elif format_name == "text":
            data = rendered.encode("ascii")
        else:
            raise PackageError(f"unsupported template format: {format_name!r}")
        if not data:
            raise PackageError(f"rendered template is empty: {source}")
        add_file(files, destination, FileSpec(data, 0o644))


def overlay_casefile_artifacts(
    files: dict[Path, FileSpec],
    document: dict,
    artifact_root: Path | None,
    source_commit: str | None,
) -> None:
    config = document.get("casefile_artifacts")
    if config is None:
        return
    if artifact_root is None:
        raise PackageError("Casefile package requires --casefile-artifact-root")
    if source_commit is None:
        raise PackageError("Casefile package requires --casefile-source-commit")
    try:
        manifest = load_casefile_artifacts(
            artifact_root, document["version"], source_commit
        )
    except ArtifactError as error:
        raise PackageError(str(error)) from error
    destination = safe_relative(config["destination"], "casefile_artifacts.destination")
    add_file(
        files,
        destination / "artifacts.json",
        FileSpec((artifact_root / "artifacts.json").read_bytes(), 0o644),
    )
    for row in manifest["artifacts"]:
        relative = safe_relative(row["path"], "artifact.path")
        mode = 0o644 if row["target"].endswith("windows-msvc") else 0o755
        add_file(
            files,
            destination / relative,
            FileSpec((artifact_root / relative).read_bytes(), mode),
        )


def expected_files(
    root: Path,
    document: dict,
    vendor: str,
    artifact_root: Path | None = None,
    source_commit: str | None = None,
) -> dict[Path, FileSpec]:
    adapter = document["vendors"][vendor]
    files: dict[Path, FileSpec] = {}
    copy_resources(root, files, document["shared"], "shared")
    copy_resources(root, files, adapter.get("copy", []), f"vendors.{vendor}.copy")
    render_templates(root, files, adapter.get("template", []), document, vendor)
    overlay_casefile_artifacts(files, document, artifact_root, source_commit)
    return dict(sorted(files.items(), key=lambda item: item[0].as_posix()))


def paths_overlap(left: Path, right: Path) -> bool:
    return left == right or left in right.parents or right in left.parents


def package_specs(
    root: Path,
    manifest_paths: list[Path],
    artifact_root: Path | None = None,
    source_commit: str | None = None,
) -> list[PackageSpec]:
    specs: list[PackageSpec] = []
    outputs: list[tuple[Path, str]] = []
    for manifest in manifest_paths:
        document = read_manifest(manifest, root)
        for vendor in sorted(document["vendors"]):
            output = safe_relative(
                document["vendors"][vendor]["output"], f"vendors.{vendor}.output"
            )
            label = f"{document['name']}:{vendor}"
            for previous, previous_label in outputs:
                if paths_overlap(output, previous):
                    raise PackageError(
                        f"package output collision: {previous_label}:{previous} and {label}:{output}"
                    )
            outputs.append((output, label))
            specs.append(
                PackageSpec(
                    manifest=manifest,
                    plugin=document["name"],
                    vendor=vendor,
                    output=output,
                    files=expected_files(root, document, vendor, artifact_root, source_commit),
                )
            )
    return specs


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
        paths += sorted(root.glob("*/packaging/plugin.toml"))
        if not paths:
            raise PackageError("no plugin manifests found")
        return paths
    if arguments.manifest is None:
        raise PackageError("pass --all or --manifest")
    path = arguments.manifest if arguments.manifest.is_absolute() else root / arguments.manifest
    return [path.resolve(strict=True)]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("build", "check"))
    selection = parser.add_mutually_exclusive_group(required=True)
    selection.add_argument("--all", action="store_true")
    selection.add_argument("--manifest", type=Path)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument("--casefile-artifact-root", type=Path)
    parser.add_argument("--casefile-source-commit")
    arguments = parser.parse_args()
    root = arguments.root.resolve()
    try:
        artifact_root = (
            arguments.casefile_artifact_root.expanduser().resolve(strict=True)
            if arguments.casefile_artifact_root is not None
            else None
        )
        for spec in package_specs(
            root,
            manifests(arguments, root),
            artifact_root,
            arguments.casefile_source_commit,
        ):
            output = root / spec.output
            if arguments.command == "build":
                build(output, spec.files)
                print(f"built {spec.plugin}:{spec.vendor}: {len(spec.files)} files")
            else:
                errors = compare(spec.files, actual_files(output))
                if errors:
                    raise PackageError("\n".join(errors))
                print(f"checked {spec.plugin}:{spec.vendor}: {len(spec.files)} files")
    except (OSError, PackageError, UnicodeError, tomllib.TOMLDecodeError) as error:
        print(f"package {arguments.command} failed: {error}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
