#!/usr/bin/env python3
"""Prepare an assignment conversation or a confirmed Casefile Provider request."""
from __future__ import annotations

import argparse
import json
import re
import sys


GUIDANCE = {
    "implementation-writer": (
        "Astra", "high",
        "Implement, verify, and preserve reviewable changes under exclusive write ownership.",
    ),
    "look-ahead-investigator": (
        "Luna", "medium",
        "Preflight one bounded next ticket and report the likely surfaces and risks.",
    ),
    "detective": (
        "Terra", "medium", "Trace one assigned question to repository evidence.",
    ),
    "verification-reviewer": (
        "Terra", "medium", "Check a specific finding or verification obligation.",
    ),
    "inspector": (
        "Terra", "xhigh",
        "Reconcile evidence across investigation branches and resolve competing explanations.",
    ),
    "atomic-ticket-reviewer": (
        "Terra", "xhigh", "Test ticket claims or an exact commit against its accepted boundary.",
    ),
    "dialogue-review-chair": (
        "Terra", "xhigh", "Reconcile independent reviews and resolve disagreements.",
    ),
    "dialogue-review-challenger": (
        "Terra", "xhigh", "Challenge findings with counterevidence and test the competing conclusions.",
    ),
}
MODEL_CLASSES = {"Astra", "Luna", "Sol", "Terra"}
Pair = tuple[str, str]


def model_class(model: str) -> str | None:
    words = set(re.split(r"[^a-z0-9]+", model.casefold()))
    matches = [model_class for model_class in MODEL_CLASSES if model_class.casefold() in words]
    return matches[0] if len(matches) == 1 else None


def pair_values(row: dict) -> Pair:
    return row["model"], row["reasoning_effort"]


def pair_row(pair: Pair) -> dict:
    model, effort = pair
    return {"model": model, "reasoning_effort": effort}


def eligible_pairs(profiles: list[str], offers: dict) -> tuple[set[Pair], list[str], list[str]]:
    choices = []
    unavailable = []
    unverified = []
    for profile in profiles:
        offer = offers.get(profile, {})
        pairs = {
            pair_values(pair) for pair in offer.get("pairs", [])
            if pair["spawn"]["agent_type"] == profile
        }
        choices.append(pairs)
        if not pairs:
            reason = offer.get("unavailable_reason", "No current offer for this named registration.")
            unavailable.append(f"{profile}: {reason}")
        if offer.get("unverified_pair_count", 0) > 0:
            unverified.append(profile)
    return (set.intersection(*choices) if choices else set()), unavailable, unverified


def recommend(
    role: str, choices: set[Pair], saved: dict | None, unavailable: list[str], unknown: bool,
) -> tuple[Pair | None, str, str]:
    if saved is not None and pair_values(saved) in choices:
        return pair_values(saved), "saved", "Your previously selected pair is eligible in this session."

    guidance = GUIDANCE.get(role)
    if not choices:
        detail = " ".join(unavailable) or "The named registrations have no shared eligible pair."
        if guidance is not None:
            preferred_class, effort, _ = guidance
            detail = f"Preference is {preferred_class} / {effort}. {detail}"
        return None, "unassigned", detail

    if guidance is None:
        return min(choices), "recommended", "This pair is eligible for the named role."

    preferred_class, preferred_effort, reason = guidance

    def rank(pair: Pair) -> tuple[bool, bool, bool, Pair]:
        candidate_class = model_class(pair[0])
        return (
            candidate_class != preferred_class,
            pair[1] != preferred_effort,
            candidate_class != "Sol",
            pair,
        )

    choice = min(choices, key=rank)
    model, effort = choice
    candidate_class = model_class(model)
    if candidate_class == preferred_class and effort == preferred_effort:
        return choice, "recommended", reason

    preference = f"Preferred {preferred_class} / {preferred_effort}"
    availability = (
        "has no verified match in this session" if unknown else "is unavailable in this session"
    )
    fallback = candidate_class or model
    return choice, "alternative", f"{preference} {availability}. {fallback} / {effort} is eligible."


def recommendation_rows(projection: dict, offer: dict) -> dict:
    if projection["adapter"] != "codex" or not projection["roles"]:
        return {"state": "skip", "actions": [], "rows": []}

    rows = []
    for role, details in projection["roles"].items():
        choices, unavailable, unverified = eligible_pairs(details["platform_profiles"], offer["roles"])
        saved = details["selection"]
        if saved is not None and saved.get("source") not in {
            "strategy_assignment", "legacy_writer_binding",
        }:
            raise ValueError(f"The saved selection for {role} lacks an explicit selection source.")
        pair, status, reason = recommend(role, choices, saved, unavailable, bool(unverified))
        if saved is not None and status != "saved":
            saved_model, saved_effort = pair_values(saved)
            reason = f"Your saved {saved_model} / {saved_effort} has no verified match. {reason}"
        rows.append({
            "role": role,
            "choice": None if pair is None else pair_row(pair),
            "status": status,
            "reason": reason,
            "eligible_choices": [pair_row(choice) for choice in sorted(choices)],
            "registrations_with_unverified_pairs": unverified,
        })

    if all(row["status"] == "saved" for row in rows):
        return {"state": "reuse", "actions": [], "rows": rows}
    if any(row["choice"] is None for row in rows):
        return {"state": "blocked", "actions": ["Change assignments"], "rows": rows}
    return {"state": "confirm", "actions": ["Use these assignments", "Change assignments"], "rows": rows}


def confirmed_request(projection: dict, offer: dict, confirmed_roles: list[dict]) -> dict:
    roles = projection["roles"]
    if projection["adapter"] != "codex" or not roles:
        raise ValueError("This strategy needs no Codex role assignments.")
    chosen_roles = {row["role"] for row in confirmed_roles}
    if chosen_roles != roles.keys() or len(confirmed_roles) != len(roles):
        raise ValueError("Confirm exactly one pair for every role in the selected strategy.")
    for row in confirmed_roles:
        role = row["role"]
        choices, _, _ = eligible_pairs(roles[role]["platform_profiles"], offer["roles"])
        if pair_values(row) not in choices:
            raise ValueError(f"The confirmed model and effort for {role} need current eligibility evidence.")
    return {"request": {
        "target": projection["target"],
        "expected_revisions": projection["revisions"],
        "roles": [{"role": row["role"], **pair_row(pair_values(row))} for row in confirmed_roles],
    }}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("recommend", "select"))
    parser.epilog = (
        "Pass the exact Provider projection and current named-role offer as projection and offer "
        "in a JSON object on stdin. For select also pass confirmed_roles after human confirmation."
    )
    arguments = parser.parse_args()
    try:
        inputs = json.load(sys.stdin)
        if arguments.command == "recommend":
            result = recommendation_rows(inputs["projection"], inputs["offer"])
        else:
            result = confirmed_request(
                inputs["projection"], inputs["offer"], inputs["confirmed_roles"]
            )
    except (ValueError, KeyError, TypeError) as error:
        print(f"Casefile role assignments failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps(result, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
