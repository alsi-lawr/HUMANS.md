#!/usr/bin/env python3
"""Read Codex's stable model projection through app-server JSON-RPC."""
from __future__ import annotations

import argparse
import json
import os
import queue
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path


DEFAULT_TIMEOUT = 20.0
PAGE_LIMIT = 100
MAX_PAGES = 100


class AppServerError(RuntimeError):
    pass


def _reader(stream, output: queue.Queue[str | UnicodeError | None]) -> None:
    try:
        for line in stream:
            output.put(line)
    except UnicodeError as error:
        output.put(error)
    finally:
        output.put(None)


class AppServer:
    def __init__(
        self,
        executable: str,
        environment: dict[str, str] | None = None,
        timeout: float = DEFAULT_TIMEOUT,
        cwd: Path | None = None,
    ) -> None:
        if timeout <= 0:
            raise AppServerError("app-server timeout must be positive")
        self.timeout = timeout
        self.process = subprocess.Popen(
            [executable, "app-server", "--stdio"],
            env=environment,
            cwd=cwd,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            encoding="utf-8",
            errors="strict",
            bufsize=1,
        )
        if self.process.stdin is None or self.process.stdout is None or self.process.stderr is None:
            self.close()
            raise AppServerError("app-server stdio was not created")
        self.output: queue.Queue[str | UnicodeError | None] = queue.Queue()
        self.stderr: list[str] = []
        self._stdout_thread = threading.Thread(
            target=_reader, args=(self.process.stdout, self.output), daemon=True
        )
        self._stderr_thread = threading.Thread(
            target=self._read_stderr, daemon=True
        )
        self._stdout_thread.start()
        self._stderr_thread.start()
        self.next_id = 1

    def _read_stderr(self) -> None:
        assert self.process.stderr is not None
        for line in self.process.stderr:
            self.stderr.append(line.rstrip("\r\n"))

    def _diagnostic(self) -> str:
        return "diagnostic output suppressed" if self.stderr else "no diagnostic output"

    def send(self, message: dict) -> None:
        if self.process.poll() is not None:
            raise AppServerError(
                f"app-server exited with {self.process.returncode}: {self._diagnostic()}"
            )
        assert self.process.stdin is not None
        try:
            self.process.stdin.write(json.dumps(message, ensure_ascii=True) + "\n")
            self.process.stdin.flush()
        except (BrokenPipeError, OSError) as error:
            raise AppServerError(f"app-server input failed: {self._diagnostic()}") from error

    def request(self, method: str, params: dict) -> dict:
        request_id = self.next_id
        self.next_id += 1
        self.send({"id": request_id, "method": method, "params": params})
        deadline = time.monotonic() + self.timeout
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise AppServerError(f"app-server timed out waiting for {method}")
            try:
                line = self.output.get(timeout=remaining)
            except queue.Empty as error:
                raise AppServerError(f"app-server timed out waiting for {method}") from error
            if line is None:
                code = self.process.poll()
                raise AppServerError(
                    f"app-server exited with {code}: {self._diagnostic()}"
                )
            if isinstance(line, UnicodeError):
                raise AppServerError("app-server returned invalid UTF-8") from line
            try:
                message = json.loads(line)
            except json.JSONDecodeError as error:
                raise AppServerError("app-server returned invalid JSON") from error
            if not isinstance(message, dict):
                raise AppServerError("app-server returned a non-object message")
            if "id" not in message:
                # Notifications may arrive between any two responses.
                continue
            if message.get("id") != request_id:
                raise AppServerError(
                    f"app-server returned an unexpected response ID while waiting for {method}"
                )
            if "error" in message:
                raise AppServerError(
                    f"app-server {method} failed: "
                    + json.dumps(message["error"], ensure_ascii=True, sort_keys=True)
                )
            result = message.get("result")
            if not isinstance(result, dict):
                raise AppServerError(f"app-server {method} returned a non-object result")
            return result

    def initialize(self) -> None:
        self.request(
            "initialize",
            {
                "clientInfo": {
                    "name": "casefile",
                    "title": "Casefile",
                    "version": "0.4.1",
                },
                "capabilities": {},
            },
        )
        self.send({"method": "initialized"})

    def close(self) -> None:
        process = getattr(self, "process", None)
        if process is None:
            return
        if process.stdin is not None and not process.stdin.closed:
            try:
                process.stdin.close()
            except OSError:
                pass
        if process.poll() is None:
            try:
                process.wait(timeout=1.0)
            except subprocess.TimeoutExpired:
                process.terminate()
                try:
                    process.wait(timeout=1.0)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=1.0)
        stdout_thread = getattr(self, "_stdout_thread", None)
        stderr_thread = getattr(self, "_stderr_thread", None)
        if stdout_thread is not None:
            stdout_thread.join(timeout=1.0)
        if stderr_thread is not None:
            stderr_thread.join(timeout=1.0)
        for stream in (process.stdout, process.stderr):
            if stream is not None and not stream.closed:
                stream.close()

    def __enter__(self) -> AppServer:
        return self

    def __exit__(self, *_: object) -> None:
        self.close()


def normalize(models: list[object]) -> list[dict]:
    result: list[dict] = []
    identifiers: set[str] = set()
    for item in models:
        if not isinstance(item, dict):
            raise AppServerError("app-server model/list contains a non-object model")
        identifier = item.get("id")
        selector = item.get("model")
        if not isinstance(identifier, str) or not identifier:
            raise AppServerError("app-server model/list contains a model without an ID")
        if identifier in identifiers:
            raise AppServerError(f"app-server model/list duplicated model ID {identifier!r}")
        if selector != identifier:
            raise AppServerError(
                f"app-server model/list model selector differs from ID {identifier!r}"
            )
        display_name = item.get("displayName")
        hidden = item.get("hidden")
        efforts = item.get("supportedReasoningEfforts")
        if not isinstance(display_name, str) or not isinstance(hidden, bool) or not isinstance(
            efforts, list
        ):
            raise AppServerError(f"app-server model/list model {identifier!r} is incomplete")
        normalized_efforts: list[dict[str, str]] = []
        seen_efforts: set[str] = set()
        for effort in efforts:
            value = effort.get("reasoningEffort") if isinstance(effort, dict) else None
            if not isinstance(value, str) or not value:
                raise AppServerError(
                    f"app-server model/list model {identifier!r} has an invalid reasoning effort"
                )
            if value in seen_efforts:
                raise AppServerError(
                    f"app-server model/list model {identifier!r} duplicated reasoning effort "
                    f"{value!r}"
                )
            seen_efforts.add(value)
            normalized_efforts.append({"effort": value})
        identifiers.add(identifier)
        result.append(
            {
                "slug": identifier,
                "display_name": display_name,
                "visibility": "hide" if hidden else "list",
                "supported_reasoning_levels": normalized_efforts,
            }
        )
    return result


def _model_projection(server: AppServer) -> dict:
    models: list[object] = []
    cursor: str | None = None
    seen_cursors: set[str] = set()
    for _ in range(MAX_PAGES):
        result = server.request(
            "model/list",
            {"cursor": cursor, "includeHidden": True, "limit": PAGE_LIMIT},
        )
        page = result.get("data")
        next_cursor = result.get("nextCursor")
        if not isinstance(page, list) or not (
            next_cursor is None or isinstance(next_cursor, str) and next_cursor
        ):
            raise AppServerError("app-server model/list returned an invalid page")
        models.extend(page)
        if next_cursor is None:
            break
        if next_cursor in seen_cursors:
            raise AppServerError("app-server model/list repeated a pagination cursor")
        seen_cursors.add(next_cursor)
        cursor = next_cursor
    else:
        raise AppServerError("app-server model/list exceeded the pagination limit")
    return {"models": normalize(models)}


def _raw_catalog(path: Path) -> dict:
    if path.is_symlink() or not path.is_file():
        raise AppServerError("authenticated model/list did not produce a safe model cache")
    try:
        value = json.loads(path.read_bytes())
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise AppServerError("authenticated model/list produced an invalid model cache") from error
    if not isinstance(value, dict) or not isinstance(value.get("models"), list):
        raise AppServerError("authenticated model/list produced an unsupported model cache")
    return value


def _identifiers(document: dict, field: str, label: str) -> set[str]:
    identifiers: list[str] = []
    for model in document["models"]:
        identifier = model.get(field) if isinstance(model, dict) else None
        if not isinstance(identifier, str) or not identifier:
            raise AppServerError(f"{label} contains a model without an ID")
        identifiers.append(identifier)
    if len(identifiers) != len(set(identifiers)):
        raise AppServerError(f"{label} contains duplicate model IDs")
    return set(identifiers)


def _protected_bytes(path: Path) -> bytes | None:
    if path.is_symlink():
        raise AppServerError("selected Codex configuration is unsafe")
    return path.read_bytes() if path.is_file() else None


def authenticated_model_catalog(
    executable: str,
    selected_home: Path,
    environment: dict[str, str] | None = None,
    timeout: float = DEFAULT_TIMEOUT,
) -> dict:
    selected_home = selected_home.expanduser().resolve(strict=True)
    source_auth = selected_home / "auth.json"
    if source_auth.exists() and (source_auth.is_symlink() or not source_auth.is_file()):
        raise AppServerError("selected Codex authentication state is unsafe")
    config_path = selected_home / "config.toml"
    protected_config = _protected_bytes(config_path)
    try:
        with tempfile.TemporaryDirectory(prefix="casefile-codex-models-") as temporary:
            acquisition_home = Path(temporary)
            os.chmod(acquisition_home, 0o700)
            base_environment = dict(environment or os.environ)
            if source_auth.is_file():
                refresh_environment = dict(base_environment)
                refresh_environment["CODEX_HOME"] = str(selected_home)
                refresh_environment["PWD"] = str(acquisition_home)
                with AppServer(
                    executable,
                    refresh_environment,
                    timeout,
                    cwd=acquisition_home,
                ) as server:
                    server.initialize()
                    account = server.request("account/read", {"refreshToken": False})
                    if not isinstance(account.get("account"), dict):
                        account = server.request("account/read", {"refreshToken": True})
                    if not isinstance(account.get("account"), dict):
                        raise AppServerError(
                            "selected Codex file authentication is unavailable"
                        )
                if not source_auth.is_file() or source_auth.is_symlink():
                    raise AppServerError("selected Codex file authentication became unsafe")
            elif not base_environment.get("OPENAI_API_KEY"):
                raise AppServerError(
                    "authenticated acquisition requires safe Codex file auth or OPENAI_API_KEY"
                )
            if source_auth.is_file():
                target_auth = acquisition_home / "auth.json"
                shutil.copyfile(source_auth, target_auth)
                os.chmod(target_auth, 0o600)
            isolated_environment = dict(base_environment)
            isolated_environment["CODEX_HOME"] = str(acquisition_home)
            isolated_environment["PWD"] = str(acquisition_home)
            with AppServer(
                executable,
                isolated_environment,
                timeout,
                cwd=acquisition_home,
            ) as server:
                server.initialize()
                account = server.request("account/read", {"refreshToken": False})
                if not isinstance(account.get("account"), dict):
                    raise AppServerError("authenticated Codex model acquisition is unavailable")
                projection = _model_projection(server)
                raw = _raw_catalog(acquisition_home / "models_cache.json")
            projection_ids = _identifiers(projection, "slug", "Codex model projection")
            raw_ids = _identifiers(raw, "slug", "Codex model cache")
            if projection_ids != raw_ids:
                raise AppServerError(
                    "authenticated model projection IDs differ from the fresh Codex model cache"
                )
            result = {"projection": projection, "raw": raw}
    finally:
        if _protected_bytes(config_path) != protected_config:
            raise AppServerError("authenticated acquisition changed selected Codex configuration")
    return result


def canonical(value: object) -> str:
    return json.dumps(value, indent=2, sort_keys=True, ensure_ascii=True) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--codex-executable", default="codex")
    parser.add_argument("--codex-home", type=Path, required=True)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--timeout", type=float, default=DEFAULT_TIMEOUT)
    arguments = parser.parse_args()
    environment = dict(os.environ)
    try:
        acquired = authenticated_model_catalog(
            arguments.codex_executable,
            arguments.codex_home,
            environment,
            arguments.timeout,
        )
        output = canonical(acquired["projection"])
        if arguments.output is None:
            sys.stdout.write(output)
        else:
            arguments.output.write_text(output, encoding="ascii")
        return 0
    except (AppServerError, OSError, UnicodeError, ValueError) as error:
        print(f"Codex model discovery failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
