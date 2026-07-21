from __future__ import annotations

import json
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path
from unittest import mock

from _load import ROOT, script


drift = script("casefile/scripts/check-codex-model-drift.py")
setup = script("casefile/adapters/codex/scripts/setup-codex.py")
PROFILES = ROOT / "casefile/adapters/codex/profiles.toml"


def catalog(profiles: dict, omitted: set[str] | None = None) -> dict:
    omitted = omitted or set()
    selector_values = {
        "gpt-5.6-sol": "v2",
        "gpt-5.6-terra": "v2",
        "gpt-5.6-luna": "v1",
    }
    models = []
    for target in profiles["catalog"]["targets"]:
        model_id = target["id"]
        if model_id in omitted:
            continue
        model = {
            "slug": model_id,
            "supported_reasoning_levels": [
                {"effort": effort} for effort in target["required_reasoning"]
            ],
            **target.get("expected", {}),
        }
        for selector in target.get("null_selectors", []):
            current = model
            parts = selector.split(".")
            for part in parts[:-1]:
                current = current.setdefault(part, {})
            current[parts[-1]] = selector_values.get(model_id, "upstream")
        models.append(model)
    return {"models": models}


class CodexModelDriftTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.profiles = tomllib.loads(PROFILES.read_text(encoding="ascii"))

    def run_check(self, current: dict) -> tuple[int, str]:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            catalog_path = root / "catalog.json"
            output_path = root / "drift.md"
            catalog_path.write_text(json.dumps(current), encoding="utf-8")
            arguments = [
                "check-codex-model-drift.py",
                "--catalog",
                str(catalog_path),
                "--profiles",
                str(PROFILES),
                "--output",
                str(output_path),
            ]
            with mock.patch.object(sys, "argv", arguments):
                code = drift.main()
            return code, output_path.read_text(encoding="ascii")

    def test_required_profile_flags_match_setup_policy(self):
        required = {
            target["id"]
            for target in self.profiles["catalog"]["targets"]
            if target.get("required") is True
        }
        self.assertEqual(setup.REQUIRED_MODELS, required)

    def test_declared_overrides_and_missing_optional_models_are_not_drift(self):
        code, output = self.run_check(catalog(self.profiles, {"gpt-5.5"}))
        self.assertEqual(0, code)
        self.assertIn("No profile-relevant drift detected.", output)

    def test_missing_required_models_and_selector_path_are_drift(self):
        current = catalog(
            self.profiles,
            {"gpt-5.6-sol", "gpt-5.3-codex-spark", "gpt-5.5"},
        )
        terra = next(
            model for model in current["models"] if model["slug"] == "gpt-5.6-terra"
        )
        del terra["multi_agent_version"]
        code, output = self.run_check(current)
        self.assertEqual(1, code)
        self.assertIn("Required model `gpt-5.6-sol` is missing.", output)
        self.assertIn("Required model `gpt-5.3-codex-spark` is missing.", output)
        self.assertIn(
            "Model `gpt-5.6-terra` no longer exposes selector `multi_agent_version`.",
            output,
        )
        self.assertNotIn("gpt-5.5", output)


if __name__ == "__main__":
    unittest.main()
