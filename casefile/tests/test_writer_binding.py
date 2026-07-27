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


def projection(
    state: str,
    model_name: str | None = None,
    effort: str | None = None,
    source: str | None = None,
    *,
    strategy_id: str = "casefile-implement-ticket-batch",
) -> dict:
    binding_state = {"state": state}
    if state in {"absent", "resolved"}:
        binding_state["effective"] = {
            "model": model_name,
            "reasoning_effort": effort,
            "source": source,
        }
    return {
        "strategy_id": strategy_id,
        "adapter": "codex",
        "binding": binding_state,
    }


class WriterBindingTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.profiles = binding.load_profiles(PROFILES_PATH)

    def setUp(self):
        progress = mock.patch.object(binding, "require_writer_progress")
        progress.start()
        self.addCleanup(progress.stop)

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

    def test_offer_keeps_every_valid_alternative_when_recommendation_is_unavailable(self):
        catalog = {"models": [model("gpt-5.6-terra", ("medium",))]}
        with mock.patch.object(binding, "active_runtime", return_value="v2"), mock.patch.object(
            binding, "active_catalog", return_value=catalog
        ):
            result = binding.offer("codex", Path("/home"), PROFILES_PATH)
        self.assertEqual(
            {
                "model": "gpt-5.6-sol",
                "reasoning_effort": "high",
                "available": False,
            },
            result["recommendation"],
        )
        self.assertEqual(
            [("gpt-5.6-terra", "medium")],
            [(pair["model"], pair["reasoning_effort"]) for pair in result["pairs"]],
        )
        self.assertEqual(
            "gpt-5.6-terra",
            binding.selected_pair(result, "gpt-5.6-terra", "medium")["model"],
        )

        with mock.patch.object(binding, "active_runtime", return_value="v2"), mock.patch.object(
            binding, "active_catalog", return_value={"models": []}
        ), self.assertRaisesRegex(binding.BindingError, "no model/effort pair"):
            binding.offer("codex", Path("/home"), PROFILES_PATH)

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

    def test_persistence_uses_typed_preview_apply_and_has_no_activity_attestation(self):
        pair = {
            "model": "gpt-5.6-sol",
            "reasoning_effort": "high",
            "resolution": {"mode": "runtime_override", "value": "route"},
        }
        with mock.patch.object(
            binding,
            "checked",
            side_effect=[json.dumps({"operation": "writer_binding"}), json.dumps({"no_op": False})],
        ) as checked:
            result = binding.persist_selection(
                "casefile",
                Path("/planning"),
                "projects/demo/investigations/sample",
                pair,
            )
        self.assertTrue(result["persisted"])
        commands = [call.args[0] for call in checked.call_args_list]
        self.assertIn("writer-binding-preview", commands[0])
        self.assertIn("writer-binding-apply", commands[1])
        self.assertNotIn("implementation-active", " ".join(sum(commands, [])))

    def test_resolve_requires_canonical_ticket_progress_before_projection(self):
        with mock.patch.object(
            binding,
            "require_writer_progress",
            side_effect=binding.BindingError("ticket is not in_progress"),
        ), mock.patch.object(binding, "binding_projection") as projection:
            with self.assertRaisesRegex(binding.BindingError, "not in_progress"):
                binding.resolve_spawn(
                    "codex",
                    Path("/home"),
                    PROFILES_PATH,
                    "casefile",
                    Path("/planning"),
                    "projects/demo/investigations/sample",
                    "casefile-implement-ticket-batch",
                    "HMD-011",
                )
            projection.assert_not_called()

    def test_v1_and_v2_resolve_alternate_binding_for_resume_and_correction(self):
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
                    "binding_projection",
                    return_value=projection(
                        "resolved",
                        "gpt-5.6-terra",
                        "medium",
                        "binding",
                        strategy_id=strategy_id,
                    ),
                ):
                    first = binding.resolve_spawn(
                        "codex",
                        Path("/home"),
                        PROFILES_PATH,
                        "casefile",
                        Path("/planning"),
                        "projects/demo/investigations/sample",
                        strategy_id,
                        "HMD-011",
                    )
                    second = binding.resolve_spawn(
                        "codex",
                        Path("/home"),
                        PROFILES_PATH,
                        "casefile",
                        Path("/planning"),
                        "projects/demo/investigations/sample",
                        strategy_id,
                        "HMD-011",
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

    def test_v1_and_v2_use_store_derived_historical_matrix_pair_and_revalidate(self):
        catalogs = {
            "v1": {
                "models": [
                    model("gpt-5.6-sol", ("high",), selector=None),
                    model("gpt-5.6-terra", ("high",), selector=None),
                ]
            },
            "v2": {
                "models": [
                    model("gpt-5.6-sol", ("high",)),
                    model("gpt-5.6-terra", ("high",)),
                ]
            },
        }
        for runtime, catalog in catalogs.items():
            with self.subTest(runtime=runtime), mock.patch.object(
                binding, "active_runtime", return_value=runtime
            ), mock.patch.object(
                binding, "active_catalog", return_value=catalog
            ) as active_catalog, mock.patch.object(
                binding,
                "binding_projection",
                return_value=projection(
                    "absent", "gpt-5.6-terra", "high", "matrix"
                ),
            ):
                first = binding.resolve_spawn(
                    "codex",
                    Path("/home"),
                    PROFILES_PATH,
                    "casefile",
                    Path("/planning"),
                    "projects/demo/investigations/sample",
                    "casefile-implement-ticket-batch",
                    "HMD-011",
                )
                second = binding.resolve_spawn(
                    "codex",
                    Path("/home"),
                    PROFILES_PATH,
                    "casefile",
                    Path("/planning"),
                    "projects/demo/investigations/sample",
                    "casefile-implement-ticket-batch",
                    "HMD-011",
                )
            self.assertEqual(first, second)
            self.assertEqual(2, active_catalog.call_count)
            self.assertEqual("matrix", first["binding_source"])
            self.assertEqual(
                ("gpt-5.6-terra", "high"),
                (first["model"], first["reasoning_effort"]),
            )

    def test_unavailable_or_invalid_persisted_binding_stops_before_delegation(self):
        catalog = {"models": [model("gpt-5.6-sol", ("high",))]}
        with mock.patch.object(binding, "active_runtime", return_value="v2"), mock.patch.object(
            binding, "active_catalog", return_value=catalog
        ), mock.patch.object(
            binding,
            "binding_projection",
            return_value=projection(
                "resolved", "gpt-5.3-codex-spark", "low", "binding"
            ),
        ):
            with self.assertRaisesRegex(binding.BindingError, "stop before delegation"):
                binding.resolve_spawn(
                    "codex",
                    Path("/home"),
                    PROFILES_PATH,
                    "casefile",
                    Path("/planning"),
                    "projects/demo/investigations/sample",
                    "casefile-implement-ticket-batch",
                    "HMD-011",
                )
        for state in ("pending", "unresolved", "invalid"):
            with self.subTest(state=state), mock.patch.object(
                binding, "binding_projection", return_value=projection(state)
            ):
                with self.assertRaisesRegex(binding.BindingError, state):
                    binding.resolve_spawn(
                        "codex",
                        Path("/home"),
                        PROFILES_PATH,
                        "casefile",
                        Path("/planning"),
                        "projects/demo/investigations/sample",
                        "casefile-implement-ticket-batch",
                        "HMD-011",
                    )

        for malformed in (
            {"strategy_id": "wrong", "adapter": "codex", "binding": {"state": "absent"}},
            projection("absent", None, "high", "matrix"),
            projection("resolved", "gpt-5.6-sol", "high", "matrix"),
        ):
            with self.subTest(malformed=malformed), mock.patch.object(
                binding, "binding_projection", return_value=malformed
            ):
                with self.assertRaises(binding.BindingError):
                    binding.resolve_spawn(
                        "codex",
                        Path("/home"),
                        PROFILES_PATH,
                        "casefile",
                        Path("/planning"),
                        "projects/demo/investigations/sample",
                        "casefile-implement-ticket-batch",
                        "HMD-011",
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
                ],
            ):
                self.assertEqual(1, binding.main())
                persist.assert_not_called()

    def test_catalog_drift_offers_and_persists_only_an_explicit_valid_reselection(self):
        catalog = {"models": [model("gpt-5.6-terra", ("medium",))]}
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with mock.patch.object(
                binding, "active_runtime", return_value="v2"
            ), mock.patch.object(
                binding, "active_catalog", return_value=catalog
            ), mock.patch.object(
                binding,
                "binding_projection",
                return_value=projection(
                    "resolved", "gpt-5.6-sol", "high", "binding"
                ),
            ), mock.patch.object(
                binding,
                "persist_selection",
                return_value={"persisted": True},
            ) as persist:
                with self.assertRaisesRegex(binding.BindingError, "stop before delegation"):
                    binding.resolve_spawn(
                        "codex",
                        root,
                        PROFILES_PATH,
                        "casefile",
                        root,
                        "projects/demo/investigations/sample",
                        "casefile-implement-ticket-batch",
                        "HMD-011",
                    )
                persist.assert_not_called()

                current = binding.offer("codex", root, PROFILES_PATH)
                self.assertFalse(current["recommendation"]["available"])
                self.assertEqual(
                    [("gpt-5.6-terra", "medium")],
                    [
                        (pair["model"], pair["reasoning_effort"])
                        for pair in current["pairs"]
                    ],
                )
                persist.assert_not_called()

                with mock.patch(
                    "sys.argv",
                    [
                        "resolve-writer-binding.py",
                        "--codex-home",
                        str(root),
                        "--codex-executable",
                        "codex",
                        "--profiles",
                        str(PROFILES_PATH),
                        "select",
                        "--casefile-executable",
                        "casefile",
                        "--planning-root",
                        str(root),
                        "--investigation",
                        "projects/demo/investigations/sample",
                        "--model",
                        "gpt-5.6-terra",
                        "--reasoning-effort",
                        "medium",
                    ],
                ):
                    self.assertEqual(0, binding.main())
                persist.assert_called_once()


if __name__ == "__main__":
    unittest.main()
