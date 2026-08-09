from __future__ import annotations

import json
import subprocess
import sys
import tomllib
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


class CasefileBoundaryTests(unittest.TestCase):
    def test_codex_writer_binding_waits_for_selected_implementation_matrix(self):
        core_skill = (ROOT / "casefile/skills/casefile/SKILL.md").read_text(
            encoding="ascii"
        )
        for startup_action in (
            "resolve-writer-binding.py offer",
            "casefile_preview_writer_binding",
        ):
            self.assertNotIn(startup_action, core_skill)
        self.assertIn(
            "never pre-create a matrix to satisfy the binding Provider", core_skill
        )

        implement_skill = (
            ROOT / "casefile/skills/casefile-implement/SKILL.md"
        ).read_text(encoding="ascii")
        ordered_gates = (
            "Require the accepted dependency-safe plan",
            "Persist and validate the exact selected matrix",
            "resolve-writer-binding.py offer",
            "casefile_preview_writer_binding",
            "immediately before every implementation-writer spawn",
        )
        positions = [implement_skill.index(gate) for gate in ordered_gates]
        self.assertEqual(sorted(positions), positions)
        self.assertIn(
            "selected implementation matrix already exists without a binding retains",
            " ".join(implement_skill.split()),
        )

        binding_schema = (
            ROOT / "casefile/casefile-workflow/schemas/strategy-binding.md"
        ).read_text(encoding="ascii")
        self.assertIn(
            "must not create a binding before an exact implementation matrix has been "
            "selected and persisted",
            " ".join(binding_schema.split()),
        )

    def test_casefile_setup_is_separate_from_core_contract(self):
        scripts = [
            ROOT / "casefile/adapters/codex/scripts/list-codex-models.py",
            ROOT / "casefile/adapters/codex/scripts/setup-codex.py",
            ROOT / "casefile/adapters/codex/scripts/resolve-writer-binding.py",
        ]
        for script in scripts:
            result = subprocess.run(
                [sys.executable, "-m", "py_compile", str(script)],
                capture_output=True,
                text=True,
            )
            self.assertEqual(0, result.returncode, result.stderr)
        version = tomllib.loads(
            (ROOT / "casefile/packaging/plugin.toml").read_text(encoding="ascii")
        )["version"]
        manifest = json.loads(
            (ROOT / "casefile/adapters/codex/metadata/plugin.json.in")
            .read_text(encoding="ascii")
            .replace("${name_json}", '"casefile"')
            .replace("${publisher_json}", '"alsi-lawr"')
            .replace("${repository_url_json}", '"https://example.test"')
            .replace("${description_json}", '"Casefile"')
            .replace("${license_json}", '"MIT"')
            .replace("${version_json}", json.dumps(version))
        )
        self.assertEqual("casefile", manifest["name"])
        self.assertFalse((ROOT / "casefile/templates/AGENTS.md").exists())
        self.assertTrue(
            (ROOT / "casefile/adapters/codex/config-fragment.toml.in").is_file()
        )

    def test_codex_writer_defaults_profiles_agents_and_runtime_routes_are_coherent(self):
        codex = ROOT / "casefile/adapters/codex"
        profiles = tomllib.loads((codex / "profiles.toml").read_text(encoding="ascii"))
        strategies = {
            "casefile-implement-ticket-batch",
            "casefile-implement-ticket-batch-look-ahead",
            "casefile-implement-pipeline",
        }
        defaults = [
            row
            for row in profiles["matrix_profiles"]
            if row["role"] == "implementation-writer"
        ]
        self.assertEqual(strategies, {row["strategy_id"] for row in defaults})
        self.assertEqual(3, len(defaults))
        for row in defaults:
            matrix = tomllib.loads(
                (codex / "matrices" / f"{row['strategy_id']}.toml").read_text(
                    encoding="ascii"
                )
            )
            worker = next(
                worker
                for worker in matrix["workers"]
                if worker["role"] == "implementation-writer"
            )
            agent = tomllib.loads((codex / row["agent_file"]).read_text(encoding="ascii"))
            self.assertEqual(row["profile"], worker["platform_profile"])
            self.assertEqual(row["model"], worker["model"])
            self.assertEqual(row["reasoning"], worker["reasoning"])
            self.assertEqual(row["model"], agent["model"])
            self.assertEqual(row["reasoning"], agent["model_reasoning_effort"])
            self.assertEqual(("gpt-5.6-sol", "high"), (row["model"], row["reasoning"]))

        targets = {
            target["id"]: target for target in profiles["catalog"]["targets"]
        }
        expected_v1 = {
            (model, effort)
            for model in ("gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna")
            for effort in targets[model]["required_reasoning"]
        }
        actual_v1 = {
            (row["model"], row["reasoning"])
            for row in profiles["writer_profiles"]
        }
        self.assertEqual(expected_v1, actual_v1)
        base_instructions = {
            row["strategy_id"]: tomllib.loads(
                (codex / row["agent_file"]).read_text(encoding="ascii")
            )["developer_instructions"]
            for row in defaults
        }
        for row in profiles["writer_profiles"]:
            self.assertEqual("v1", row["multi_agent_version"])
            self.assertEqual("implementation-writer", row["role"])
            self.assertEqual(strategies, set(row["strategy_ids"]))
            agent = tomllib.loads((codex / row["agent_file"]).read_text(encoding="ascii"))
            self.assertEqual(row["model"], agent["model"])
            self.assertEqual(row["reasoning"], agent["model_reasoning_effort"])
            for invariant in (
                "exclusive write ownership",
                "immutable commit",
                "Corrections preempt forward work",
                "root confirms dependency independence and disjoint write paths",
            ):
                self.assertIn(invariant.lower(), agent["developer_instructions"].lower())

        overrides = profiles["writer_runtime_overrides"]
        self.assertEqual(strategies, {row["strategy_id"] for row in overrides})
        for row in overrides:
            self.assertEqual("v2", row["multi_agent_version"])
            self.assertTrue(row["model_override"])
            self.assertTrue(row["reasoning_override"])
            self.assertGreater(row["fork_turns"], 0)
            agent = tomllib.loads((codex / row["agent_file"]).read_text(encoding="ascii"))
            self.assertNotIn("model", agent)
            self.assertNotIn("model_reasoning_effort", agent)
            self.assertEqual(
                base_instructions[row["strategy_id"]], agent["developer_instructions"]
            )

        fragment = (
            (codex / "config-fragment.toml.in")
            .read_text(encoding="ascii")
            .replace("__HUMANS_MD_PLUGIN_ROOT__", "/plugin")
            .replace("__CASEFILE_MULTI_AGENT_V1__", "true")
            .replace("__CASEFILE_MULTI_AGENT_V2__", "false")
            .replace("__CASEFILE_EXECUTABLE__", '"/runtime/casefile"')
            .replace("__CASEFILE_PLANNING_ROOT__", '"/planning"')
        )
        agents = tomllib.loads(fragment)["agents"]
        for row in [*defaults, *profiles["writer_profiles"], *overrides]:
            self.assertEqual(
                f"/plugin/{row['agent_file']}", agents[row["profile"]]["config_file"]
            )


if __name__ == "__main__":
    unittest.main()
