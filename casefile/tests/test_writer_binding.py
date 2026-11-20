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

    def test_active_catalog_uses_app_server_only_for_availability(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            packaged_script = home / "plugin/scripts/resolve-writer-binding.py"
            packaged_script.parent.mkdir(parents=True)
            packaged_script.touch()
            packaged_profiles = home / "plugin/config/profiles.toml"
            packaged_profiles.parent.mkdir()
            packaged_profiles.write_bytes(PROFILES_PATH.read_bytes())
            catalog_path = home / "models-casefile-v2.json"
            catalog_path.write_text(
                json.dumps(
                    {
                        "models": [
                            {
                                "slug": "gpt-5.6-sol",
                                "multi_agent_version": "v2",
                                "visibility": "hide",
                                "supported_reasoning_levels": [{"effort": "low"}],
                            },
                            {
                                "slug": "gpt-5.2",
                                "multi_agent_version": "v2",
                                "visibility": "list",
                                "supported_reasoning_levels": [{"effort": "xhigh"}],
                            }
                        ]
                    }
                ),
                encoding="ascii",
            )
            (home / "config.toml").write_text(
                f'model_catalog_json = {json.dumps(str(catalog_path))}\n'
                "[features]\nmulti_agent = false\nmulti_agent_v2 = true\n",
                encoding="ascii",
            )
            projection = {
                "models": [
                    {
                        "slug": "gpt-5.6-sol",
                        "display_name": "Sol",
                        "visibility": "list",
                        "supported_reasoning_levels": [{"effort": "high"}],
                    }
                ]
            }
            with mock.patch.object(binding, "__file__", str(packaged_script)), mock.patch.object(
                binding.list_codex_models, "listing", return_value=projection
            ) as model_list:
                active = binding.active_catalog("codex", home)
            self.assertEqual(("codex", packaged_profiles), model_list.call_args.args)
            self.assertEqual(str(home), model_list.call_args.kwargs["environment"]["CODEX_HOME"])
            self.assertEqual(["gpt-5.6-sol"], [model["slug"] for model in active["models"]])
            self.assertEqual("hide", active["models"][0]["visibility"])
            self.assertEqual(
                [{"effort": "low"}],
                active["models"][0]["supported_reasoning_levels"],
            )
            self.assertEqual("v2", active["models"][0]["multi_agent_version"])

            projection["models"].append(
                {
                    "slug": "gpt-5.6-sol-wm",
                    "display_name": "Sol WM",
                    "visibility": "hide",
                    "supported_reasoning_levels": [{"effort": "high"}],
                }
            )
            with mock.patch.object(
                binding.list_codex_models, "listing", return_value=projection
            ):
                active = binding.active_catalog("codex", home)
            self.assertEqual(["gpt-5.6-sol"], [model["slug"] for model in active["models"]])

    def test_v2_offers_visible_models_but_not_hidden_models(self):
        catalog = {
            "models": [
                model("gpt-6-astra", ("high",)),
                model("gpt-5.5", ("low", "xhigh")),
                model("gpt-6-luna", ("low",)),
                model("codex-auto-review", ("low",), visibility="hide"),
            ]
        }
        offered = binding.offered_pairs(catalog, self.profiles, "v2")
        self.assertEqual(
            {
                ("gpt-6-astra", "high"),
                ("gpt-5.5", "low"),
                ("gpt-5.5", "xhigh"),
                ("gpt-6-luna", "low"),
            },
            {(pair["model"], pair["reasoning_effort"]) for pair in offered},
        )
        recommendation = next(pair for pair in offered if pair["recommended"])
        self.assertEqual(
            ("gpt-6-astra", "high"),
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
                "model": "gpt-6-astra",
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
                {"models": [model("gpt-6-luna", ("low",))]},
                self.profiles,
                "v2",
            )
            if pair["reasoning_effort"] == "low"
        )
        document = tomllib.loads(binding.binding_source(pair))
        self.assertEqual(1, document["schema_version"])
        self.assertEqual("codex", document["adapter"])
        self.assertEqual("implementation-writer", document["role"])
        self.assertEqual("gpt-6-luna", document["model"])
        self.assertEqual("low", document["reasoning_effort"])
        self.assertEqual("runtime_override", document["resolution"]["mode"])

    def test_selection_returns_typed_provider_request_without_a_second_approval(self):
        pair = {
            "model": "gpt-5.6-sol",
            "reasoning_effort": "high",
            "resolution": {"mode": "runtime_override", "value": "route"},
        }
        result = binding.selection_request("projects/demo/investigations/sample", pair)
        self.assertFalse(result["persisted"])
        self.assertFalse(result["approval_required"])
        self.assertEqual("casefile_preview_writer_binding", result["provider_preview_tool"])
        self.assertEqual("casefile_apply_writer_binding", result["provider_apply_tool"])
        self.assertEqual(
            "projects/demo/investigations/sample",
            result["request"]["investigation"],
        )
        self.assertNotIn("implementation_active", result["request"])

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
            ), mock.patch.object(binding, "selection_request") as persist, mock.patch(
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
                "selection_request",
                return_value={"persisted": True},
            ) as persist:
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
