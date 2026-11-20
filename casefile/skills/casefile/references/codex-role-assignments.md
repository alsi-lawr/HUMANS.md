# Codex role assignments

Use this flow for the exact governed Codex strategy the human selected. Run it after persisting the
selected matrix with the strategy transition tools, or on re-entry with that matrix already
selected. Assign implementation roles only after the dependency-safe plan is accepted.

1. Call `casefile_project_strategy_assignments` with the exact `target` containing the activated
   investigation path, phase, and selected strategy ID. Use its returned `roles` and preserve its
   `target` and `revisions` for confirmation. A strategy with zero workers needs no assignment
   prompt, discovery command, or assignment write. Claude follows its existing strategy flow.
2. Capture live spawn evidence using the installed `scripts/offer-role-models.py --help`. Use the
   callable tool in this session, including its runtime, parameters, named registrations, locks,
   legal fork values, and model and effort coverage. Use the active provider ID from launch settings
   or current Codex thread metadata. Run `scripts/offer-role-models.py` from the active Casefile
   Codex plugin with the union of the projection's `platform_profiles` as positional arguments and
   the evidence JSON on stdin. Keep evidence in this interaction. Provider discovery alone cannot
   establish eligibility. A role missing from the callable stays unassigned.
3. Run the installed `scripts/recommend-role-assignments.py recommend` with JSON on stdin containing
   `projection` and `offer`, the unwrapped objects returned by the Provider and the offer command.
   Use its class and effort guidance and brief role rationale. The helper checks the same pair
   against every named registration for a role. A saved explicit selection, including an applicable
   legacy writer binding, is reusable only if the current offer contains that exact pair. For a
   `reuse` result, continue without another confirmation unless the human asks to edit.

Show one row per returned role with its name, one concrete model ID and effort, and the reason. Name
alternatives and the unavailable preference. Mark a saved row as previously selected and a row
without an eligible pair as unassigned. When a saved choice loses eligibility, tell the human which
pair needs replacement. The current offer supplies the displayed model IDs. Root model settings and
matrix defaults do not supply a selection.

If `registrations_with_unverified_pairs` is non-empty, say that the callable provided examples and
further eligibility is unknown. A supported exhaustive allowlist remains the limit even if the
provider lists other models. For an additional pair outside native examples, obtain the exact model
and effort from the current spawn validator registry or verify child metadata from an already
authorized named-role launch through the same callable, provider, and runtime. Feed that evidence to
`verified_models` and repeat the live offer. A candidate without either source remains unverified.
Do not launch a worker to guess whether a recommendation is valid.

Present **Use these assignments** and **Change assignments** when every row has a choice. Use the
host's question tool when available or present the options in conversation and wait for a reply. For
an unassigned row, leave confirmation unavailable and explain which role or evidence is missing.
With **Change assignments**, show every `eligible_choices` pair for the roles the human wants to
edit. Accept exact model and effort choices, including pairs outside the preferred classes. Preserve
the other rows and show the resulting complete table for confirmation. If the human wants a
different strategy, return to strategy selection.

Only an affirmative choice of the displayed complete set, or an explicit complete set of eligible
pairs for this strategy, authorizes persistence. Selecting a strategy, opening edits, or seeing a
recommendation supplies no assignment confirmation. Refresh the spawn offer before forming the
request. If the confirmed set loses an eligible pair, explain the change and ask for a replacement.

Run `scripts/recommend-role-assignments.py select` with `projection`, refreshed `offer`, and a
`confirmed_roles` list containing the exact `role`, `model`, and `reasoning_effort` of each
confirmed row. Use the projection underlying the confirmed table. Pass the JSON returned by `select`
to `casefile_preview_strategy_assignments`. Review the envelope and apply its `preview_id` with
`casefile_apply_strategy_assignments` in the same MCP session. The human's selection authorizes the
apply. If the Provider rejects the revision or a dependency changed, reproject the exact strategy
and obtain fresh confirmation before another apply. Re-read the scoped projection after a successful
apply.

An incomplete, failed, or unconfirmed assignment set leaves delegation pending. Strategy rows do not
change the permissions or ownership of the named roles. Ticket progress gates still apply.
