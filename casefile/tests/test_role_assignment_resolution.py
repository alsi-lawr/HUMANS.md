from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path

from _load import ROOT


ADAPTER = ROOT / "casefile/adapters/codex"
INVESTIGATION = "projects/demo/investigations/sample"
IMPLEMENT = "casefile-implement-ticket-batch"
DIALOGUE = "casefile-review-dialogue"
WRITER = "implementation-writer"
CHALLENGER = "dialogue-review-challenger"
SAVED = "fixture-chosen-sol"
OTHER = "fixture-other-astra"


class RoleResolutionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        subprocess.run(["cargo", "build", "--locked", "-p", "casefile-cli"], cwd=ROOT / "casefile", check=True)
        target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "casefile/target")).resolve()
        cls.casefile = target / "debug" / ("casefile.exe" if os.name == "nt" else "casefile")

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.planning = self.root / "planning"
        shutil.copytree(ROOT / "casefile/casefile-store/tests/fixtures/minimum", self.planning)
        subprocess.run(["git", "init", "-q"], cwd=self.planning, check=True)
        self.investigation = self.planning / INVESTIGATION
        self.scripts = self.root / "package/scripts"
        self.scripts.mkdir(parents=True)
        for name in ("resolve-role-assignment.py", "offer-role-models.py", "list-codex-models.py"):
            shutil.copy2(ADAPTER / "scripts" / name, self.scripts / name)
        shutil.copy2(ADAPTER.parent / "shared/casefile_runtime.py", self.scripts / "casefile_runtime.py")
        source = self.root / "provider.py"
        source.write_text(
            """#!/usr/bin/env python3
import json, os, sys
from pathlib import Path

if sys.argv[1:3] != ['app-server', '-c'] or 'fixture-provider' not in sys.argv[3]:
    raise SystemExit(2)
home = Path(os.environ['CODEX_HOME'])
for line in sys.stdin:
    request = json.loads(line)
    if request['method'] == 'initialize':
        print(json.dumps({'id': request['id'], 'result': {}}), flush=True)
    if request['method'] == 'model/list':
        change = home / 'change-during-discovery.json'
        if change.is_file():
            target, source = json.loads(change.read_text())
            Path(target).write_text(source)
        models = json.loads((home / 'models.json').read_text())
        print(json.dumps({'id': request['id'], 'result': {'data': models}}), flush=True)
""",
            encoding="ascii",
        )
        source.chmod(0o755)
        self.codex = source
        if os.name == "nt":
            self.codex = self.root / "provider.cmd"
            self.codex.write_text(f'@"{sys.executable}" "{source}" %*\r\n', encoding="utf-8")

    def matrix(self, strategy):
        source = (ADAPTER / "matrices" / f"{strategy}.toml").read_text(encoding="ascii")
        matrix = tomllib.loads(source)
        target = self.investigation / "strategy" / f"{matrix['phase']}.toml"
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(source, encoding="ascii")
        return matrix

    def assignments(self, matrix, model):
        path = self.investigation / "strategy/assignments" / matrix["phase"] / f"{matrix['strategy_id']}.toml"
        source = (
            'schema_version = 1\nadapter = "codex"\n'
            f'phase = "{matrix["phase"]}"\nstrategy_id = "{matrix["strategy_id"]}"\n'
        )
        for role in sorted({worker["role"] for worker in matrix["workers"]}):
            source += f'\n[[roles]]\nrole = "{role}"\nmodel = "{model}"\nreasoning_effort = "high"\n'
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(source, encoding="ascii")
        return path

    def progress(self):
        path = self.investigation / "progress/log.toml"
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(
            'schema_version = 1\n\n[[entries]]\nkind = "transition"\nid = "begin"\n'
            'recorded_at = "2026-11-20T00:00:00Z"\nrecorded_by = "root"\n'
            'ticket_id = "HMD-011"\nfrom = "unknown"\nto = "in_progress"\n',
            encoding="ascii",
        )

    def provider_models(self, *models):
        (self.root / "models.json").write_text(json.dumps([
            {"id": model, "model": model, "hidden": False, "supportedReasoningEfforts": [{"reasoningEffort": "high"}]}
            for model in models
        ]), encoding="ascii")

    def evidence(self, matrix, role, runtime="v2"):
        profile = next(worker["platform_profile"] for worker in matrix["workers"] if worker["role"] == role)
        fork = "fork_context" if runtime == "v1" else "fork_turns"
        return {
            "provider": "fixture-provider",
            "runtime": runtime,
            "parameters": ["agent_type", "model", "reasoning_effort", fork],
            "fork_values": [False, True] if runtime == "v1" else ["none", "all"],
            "named_role_overrides": True,
            "models_scope": "exhaustive",
            "roles": {profile: {}},
            "models": {SAVED: ["high"], OTHER: ["high"]},
        }

    def resolve(self, matrix, role, evidence, *, ticket="HMD-011", extra=()):
        arguments = [
            sys.executable, self.scripts / "resolve-role-assignment.py",
            "--casefile-executable", self.casefile, "--planning-root", self.planning,
            "--investigation", INVESTIGATION, "--phase", matrix["phase"],
            "--strategy-id", matrix["strategy_id"], "--role", role,
            "--codex-home", self.root, "--codex-executable", self.codex, *extra,
        ]
        if matrix["phase"] == "implementation" and role == WRITER and ticket is not None:
            arguments.extend(["--ticket-id", ticket])
        return subprocess.run(arguments, input=json.dumps(evidence), capture_output=True, text=True)

    def spawn(self, result):
        self.assertEqual(0, result.returncode, result.stderr)
        return json.loads(result.stdout)["spawn"]

    def blocked(self, result, diagnostic):
        self.assertEqual(1, result.returncode, result.stderr)
        self.assertEqual("", result.stdout)
        self.assertIn(diagnostic, result.stderr)

    def test_current_writer_pair_returns_exact_runtime_arguments_and_requires_ticket_progress(self):
        matrix = self.matrix(IMPLEMENT)
        self.assignments(matrix, SAVED)
        self.provider_models(SAVED, OTHER)
        self.blocked(self.resolve(matrix, WRITER, self.evidence(matrix, WRITER)), "valid canonical progress log")
        self.progress()
        for runtime, fork, fresh in (("v1", "fork_context", False), ("v2", "fork_turns", "none")):
            with self.subTest(runtime=runtime):
                evidence = self.evidence(matrix, WRITER, runtime)
                profile = next(iter(evidence["roles"]))
                expected = {"agent_type": profile, fork: fresh, "model": SAVED, "reasoning_effort": "high"}
                self.assertEqual(expected, self.spawn(self.resolve(matrix, WRITER, evidence)))
                self.blocked(self.resolve(matrix, WRITER, evidence, ticket="HMD-999"), "valid accepted ticket")

    def test_withdrawal_blocks_even_when_an_alternative_remains_until_it_is_explicitly_saved(self):
        matrix = self.matrix(IMPLEMENT)
        self.assignments(matrix, SAVED)
        self.progress()
        evidence = self.evidence(matrix, WRITER)
        self.provider_models(SAVED, OTHER)
        self.assertEqual(SAVED, self.spawn(self.resolve(matrix, WRITER, evidence))["model"])
        self.provider_models(OTHER)
        self.blocked(self.resolve(matrix, WRITER, evidence), f"saved {SAVED} / high")
        self.assignments(matrix, OTHER)
        self.assertEqual(OTHER, self.spawn(self.resolve(matrix, WRITER, evidence))["model"])

    def test_nested_caller_requires_its_own_named_registration(self):
        matrix = self.matrix(DIALOGUE)
        self.assignments(matrix, SAVED)
        self.provider_models(SAVED)
        evidence = self.evidence(matrix, CHALLENGER)
        profile = next(iter(evidence["roles"]))
        self.assertEqual(profile, self.spawn(self.resolve(matrix, CHALLENGER, evidence))["agent_type"])
        unavailable = {**evidence, "parameters": ["fork_turns", "model", "reasoning_effort"]}
        self.blocked(self.resolve(matrix, CHALLENGER, unavailable), "lacks a context-free named role")
        self.blocked(self.resolve(matrix, CHALLENGER, evidence, extra=("--platform-profile", "default")), "selected matrix")

    def test_retired_writer_alias_uses_the_saved_pair_on_the_ordinary_role(self):
        matrix = self.matrix(IMPLEMENT)
        writer = next(worker for worker in matrix["workers"] if worker["role"] == WRITER)
        matrix_path = self.investigation / "strategy/implementation.toml"
        profile_line = f'platform_profile = "{writer["platform_profile"]}"\n'
        matrix_path.write_text(matrix_path.read_text().replace(
            profile_line, profile_line + 'model = "gpt-6-astra"\nreasoning = "high"\n',
        ))
        legacy_model = "gpt-5.6-sol"
        self.provider_models(legacy_model, OTHER)
        self.progress()
        evidence = self.evidence(matrix, WRITER, "v1")
        evidence["models"][legacy_model] = ["high"]
        self.blocked(self.resolve(matrix, WRITER, evidence), "needs an explicit model and effort selection")
        legacy = self.investigation / "strategy/bindings.toml"
        legacy.write_text(
            'schema_version = 1\nadapter = "codex"\nrole = "implementation-writer"\n'
            f'model = "{legacy_model}"\nreasoning_effort = "high"\n[resolution]\n'
            f'mode = "named_profile"\nvalue = "{IMPLEMENT}=casefile-implementation-writer-gpt-5-6-sol-high"\n',
            encoding="ascii",
        )
        before = legacy.read_bytes()
        resolved = self.resolve(matrix, WRITER, evidence)
        self.assertEqual(
            {
                "agent_type": writer["platform_profile"],
                "fork_context": False,
                "model": legacy_model,
                "reasoning_effort": "high",
            },
            self.spawn(resolved),
        )
        self.assertEqual("legacy_writer_binding", json.loads(resolved.stdout)["source"])
        self.provider_models(OTHER)
        self.blocked(self.resolve(matrix, WRITER, evidence), f"saved {legacy_model} / high")
        self.assertEqual(before, legacy.read_bytes())

    def test_assignment_changed_during_provider_discovery_requires_another_check(self):
        matrix = self.matrix(DIALOGUE)
        changed = self.assignments(matrix, OTHER).read_text(encoding="ascii")
        original = self.assignments(matrix, SAVED)
        self.provider_models(SAVED, OTHER)
        (self.root / "change-during-discovery.json").write_text(json.dumps([str(original), changed]))
        self.blocked(self.resolve(matrix, CHALLENGER, self.evidence(matrix, CHALLENGER)), "changed during the spawn check")
