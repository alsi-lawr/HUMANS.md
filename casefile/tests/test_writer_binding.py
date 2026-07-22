from __future__ import annotations

import json
import tempfile
import tomllib
import unittest
from pathlib import Path
from unittest import mock

from _load import ROOT, script


binding = script("casefile/adapters/codex/scripts/resolve-writer-binding.py")
PROFILES_PATH = ROOT / "casefile/adapters/codex/profiles.toml"


def model(
    slug: str,
    efforts: tuple[str, ...],
    *,
    visibility: str = "list",
    selector: str | None = "v2",
) -> dict:
    return {
        "slug": slug,
        "display_name": slug,
        "visibility": visibility,
        "multi_agent_version": selector,
        "supported_reasoning_levels": [{"effort": effort} for effort in efforts],
    }


def scan(
    binding_value: dict | None = None,
    *,
    classification: str = "governed",
    strategy_id: str = "casefile-implement-ticket-batch",
) -> dict:
    investigation = "projects/demo/investigations/sample"
    entries = [
        {
            "path": f"{investigation}/strategy/implementation.toml",
            "classification": "governed",
            "kind": "strategy",
            "summary": {
                "type": "strategy",
                "strategy_id": strategy_id,
                "phase": "implementation",
                "adapter": "codex",
            },
        }
    ]
    if binding_value is not None:
        entries.append(
            {
                "path": f"{investigation}/strategy/bindings.toml",
                "classification": classification,
                "kind": "strategy_binding",
                "summary": {
                    "type": "strategy_binding",
                    "binding": binding_value,
                }
                if classification == "governed"
                else None,
            }
        )
    return {"activation": "active", "snapshot": {"entries": entries}, "diagnostics": []}


class WriterBindingTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.profiles = binding.load_profiles(PROFILES_PATH)

    def test_complete_predicate_filters_visibility_selector_resolution_and_effort(self):
        catalog = {
            "models": [
                model("gpt-5.6-sol", ("low", "high"), selector=None),
                model("gpt-5.6-terra", ("medium",), selector=None),
                model("gpt-5.3-codex-spark", ("low",), selector=None),
                model("gpt-5.5", ("low",), visibility="hide", selector=None),
                model("gpt-5.4", ("low",), selector="v2"),
            ]
        }
        offered = binding.offered_pairs(catalog, self.profiles, "v1")
        self.assertEqual(
            {
                ("gpt-5.6-sol", "low"),
                ("gpt-5.6-sol", "high"),
                ("gpt-5.6-terra", "medium"),
            },
            {(pair["model"], pair["reasoning_effort"]) for pair in offered},
        )
        self.assertNotIn(
            ("gpt-5.3-codex-spark", "low"),
            {(pair["model"], pair["reasoning_effort"]) for pair in offered},
        )

    def test_runtime_is_read_from_the_active_setup_feature_contract(self):
        for expected, features in (
            ("v1", "multi_agent = true\nmulti_agent_v2 = false\n"),
            ("v2", "multi_agent = false\nmulti_agent_v2 = true\n"),
        ):
            with self.subTest(runtime=expected), tempfile.TemporaryDirectory() as temporary:
                home = Path(temporary)
                (home / "config.toml").write_text(
                    f"[features]\n{features}", encoding="ascii"
                )
                self.assertEqual(expected, binding.active_runtime(home))
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            (home / "config.toml").write_text(
                "[features]\nmulti_agent = true\nmulti_agent_v2 = true\n",
                encoding="ascii",
            )
            with self.assertRaisesRegex(binding.BindingError, "exactly one"):
                binding.active_runtime(home)

    def test_v2_offers_visible_optional_and_spark_pairs_but_not_hidden_models(self):
        catalog = {
            "models": [
                model("gpt-5.6-sol", ("high",)),
                model("gpt-5.5", ("low", "xhigh")),
                model("gpt-5.3-codex-spark", ("low",)),
                model("codex-auto-review", ("low",), visibility="hide"),
            ]
        }
        offered = binding.offered_pairs(catalog, self.profiles, "v2")
        self.assertEqual(
            {
                ("gpt-5.6-sol", "high"),
                ("gpt-5.5", "low"),
                ("gpt-5.5", "xhigh"),
                ("gpt-5.3-codex-spark", "low"),
            },
            {(pair["model"], pair["reasoning_effort"]) for pair in offered},
        )
        recommendation = next(pair for pair in offered if pair["recommended"])
        self.assertEqual(
            ("gpt-5.6-sol", "high"),
            (recommendation["model"], recommendation["reasoning_effort"]),
        )

    def test_binding_source_is_the_hmd_021_record_and_requires_explicit_selection(self):
        pair = next(
            pair
            for pair in binding.offered_pairs(
                {"models": [model("gpt-5.3-codex-spark", ("low",))]},
                self.profiles,
                "v2",
            )
            if pair["reasoning_effort"] == "low"
        )
        document = tomllib.loads(binding.binding_source(pair))
        self.assertEqual(1, document["schema_version"])
        self.assertEqual("codex", document["adapter"])
        self.assertEqual("implementation-writer", document["role"])
        self.assertEqual("gpt-5.3-codex-spark", document["model"])
        self.assertEqual("low", document["reasoning_effort"])
        self.assertEqual("runtime_override", document["resolution"]["mode"])

    def test_v1_and_v2_resolve_alternate_binding_for_resume_and_correction(self):
        value = {
            "adapter": "codex",
            "role": "implementation-writer",
            "model": "gpt-5.6-terra",
            "reasoning_effort": "medium",
            "resolution": {"mode": "selected", "value": "selected"},
        }
        catalog_v1 = {
            "models": [
                model("gpt-5.6-sol", ("high",), selector=None),
                model("gpt-5.6-terra", ("medium",), selector=None),
            ]
        }
        catalog_v2 = {
            "models": [
                model("gpt-5.6-sol", ("high",)),
                model("gpt-5.6-terra", ("medium",)),
            ]
        }
        for runtime, catalog in (("v1", catalog_v1), ("v2", catalog_v2)):
            for strategy_id in binding.STRATEGIES:
                with self.subTest(runtime=runtime, strategy=strategy_id), mock.patch.object(
                    binding, "active_runtime", return_value=runtime
                ), mock.patch.object(
                    binding, "active_catalog", return_value=catalog
                ), mock.patch.object(
                    binding,
                    "scan",
                    return_value=scan(value, strategy_id=strategy_id),
                ):
                    first = binding.resolve_spawn(
                        "codex",
                        Path("/home"),
                        PROFILES_PATH,
                        "casefile",
                        Path("/planning"),
                        "projects/demo/investigations/sample",
                        strategy_id,
                    )
                    second = binding.resolve_spawn(
                        "codex",
                        Path("/home"),
                        PROFILES_PATH,
                        "casefile",
                        Path("/planning"),
                        "projects/demo/investigations/sample",
                        strategy_id,
                    )
                    self.assertEqual(first, second)
                    self.assertEqual("binding", first["binding_source"])
                    self.assertEqual(
                        ("gpt-5.6-terra", "medium"),
                        (first["model"], first["reasoning_effort"]),
                    )
                    if runtime == "v1":
                        self.assertNotIn("model", first["spawn"])
                        self.assertIn(
                            "casefile-implementation-writer-gpt-5-6-terra-medium",
                            first["spawn"]["agent_type"],
                        )
                    else:
                        self.assertEqual("gpt-5.6-terra", first["spawn"]["model"])
                        self.assertEqual("medium", first["spawn"]["reasoning_effort"])
                        self.assertEqual("3", first["spawn"]["fork_turns"])

    def test_historical_casefile_uses_matrix_default_and_revalidates_it(self):
        catalog = {"models": [model("gpt-5.6-sol", ("high",))]}
        with mock.patch.object(binding, "active_runtime", return_value="v2"), mock.patch.object(
            binding, "active_catalog", return_value=catalog
        ), mock.patch.object(binding, "scan", return_value=scan()):
            result = binding.resolve_spawn(
                "codex",
                Path("/home"),
                PROFILES_PATH,
                "casefile",
                Path("/planning"),
                "projects/demo/investigations/sample",
                "casefile-implement-ticket-batch",
            )
        self.assertEqual("matrix_default", result["binding_source"])
        self.assertEqual(("gpt-5.6-sol", "high"), (result["model"], result["reasoning_effort"]))

    def test_unavailable_or_invalid_persisted_binding_stops_before_delegation(self):
        unavailable = {
            "adapter": "codex",
            "role": "implementation-writer",
            "model": "gpt-5.3-codex-spark",
            "reasoning_effort": "low",
            "resolution": {"mode": "runtime_override", "value": "route"},
        }
        catalog = {"models": [model("gpt-5.6-sol", ("high",))]}
        with mock.patch.object(binding, "active_runtime", return_value="v2"), mock.patch.object(
            binding, "active_catalog", return_value=catalog
        ), mock.patch.object(binding, "scan", return_value=scan(unavailable)):
            with self.assertRaisesRegex(binding.BindingError, "stop before delegation"):
                binding.resolve_spawn(
                    "codex",
                    Path("/home"),
                    PROFILES_PATH,
                    "casefile",
                    Path("/planning"),
                    "projects/demo/investigations/sample",
                    "casefile-implement-ticket-batch",
                )
        with mock.patch.object(binding, "scan", return_value=scan({}, classification="invalid")):
            with self.assertRaisesRegex(binding.BindingError, "invalid"):
                binding.resolve_spawn(
                    "codex",
                    Path("/home"),
                    PROFILES_PATH,
                    "casefile",
                    Path("/planning"),
                    "projects/demo/investigations/sample",
                    "casefile-implement-ticket-batch",
                )
        unresolved = scan(unavailable)
        unresolved["diagnostics"] = [
            {
                "path": "projects/demo/investigations/sample/strategy/bindings.toml",
                "code": "binding_writer_match",
                "message": "implementation strategy must declare exactly one implementation-writer",
            }
        ]
        with mock.patch.object(binding, "scan", return_value=unresolved):
            with self.assertRaisesRegex(binding.BindingError, "unresolved"):
                binding.resolve_spawn(
                    "codex",
                    Path("/home"),
                    PROFILES_PATH,
                    "casefile",
                    Path("/planning"),
                    "projects/demo/investigations/sample",
                    "casefile-implement-ticket-batch",
                )

    def test_unavailable_selection_never_calls_persistence_bridge(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            with mock.patch.object(
                binding,
                "offer",
                return_value={
                    "multi_agent_version": "v2",
                    "recommendation": {"model": "gpt-5.6-sol", "reasoning_effort": "high"},
                    "pairs": [
                        {
                            "model": "gpt-5.6-sol",
                            "reasoning_effort": "high",
                            "resolution": {"mode": "runtime_override", "value": "route"},
                            "recommended": True,
                        }
                    ],
                },
            ), mock.patch.object(binding, "persist_selection") as persist, mock.patch(
                "sys.argv",
                [
                    "resolve-writer-binding.py",
                    "--codex-home",
                    str(home),
                    "--codex-executable",
                    "codex",
                    "--profiles",
                    str(PROFILES_PATH),
                    "select",
                    "--casefile-executable",
                    "casefile",
                    "--planning-root",
                    str(home),
                    "--investigation",
                    "projects/demo/investigations/sample",
                    "--model",
                    "gpt-5.3-codex-spark",
                    "--reasoning-effort",
                    "low",
                    "--implementation-active",
                    "false",
                ],
            ):
                self.assertEqual(1, binding.main())
                persist.assert_not_called()


if __name__ == "__main__":
    unittest.main()
