# Codex delegation

Use this check immediately before each governed Codex delegation. It applies to the root and to
authorized nested callers, including initial, repeated, resumed, look-ahead, review, and correction
work. The selected matrix still determines permissions, capacity, ownership, and the work assigned.

The caller collects live spawn evidence in that session using `scripts/offer-role-models.py --help`.
Use the callable actually available to the caller. A nested caller's tool can omit named roles or
overrides, and the host can refuse further spawns at its depth limit. Report the limitation and keep
that delegation pending.

Run the installed `scripts/resolve-role-assignment.py` with fresh evidence JSON on stdin. Supply the
absolute `--casefile-executable` configured for this Casefile and the absolute `--planning-root`.
Pass the scoped `--investigation`, selected `--phase`, exact `--strategy-id`, and matrix `--role`.
If the matrix has several named registrations for the role, pass the intended one as
`--platform-profile`. The resolver uses the caller's Codex executable and active home. Supply
`--codex-executable` or `--codex-home` when the session uses a different executable or home.

An applicable historical writer binding supplies its saved model and effort for the selected
matrix's ordinary writer registration, including when the historical record names a retired alias.

For an implementation writer, pass `--ticket-id` for the exact accepted ticket. Confirm role
assignments before starting ticket progress. Set the ticket to canonical `in_progress` through the
existing progress workflow, including any required correction transition. An already active ticket
can use its existing `in_progress` transition. The resolver independently checks this state before
returning spawn arguments.

On success, pass every field in `spawn` to the current native tool and add the required task input
and task name where applicable. Keep the named registration and fork settings. A context-free child
needs the repository boundary, objective, authority, and evidence required to do its task in the
handoff. For a nested caller, include the exact strategy target, allowed child role, installed
script and executable locations, and planning root so it can run this same check independently.

For a follow-up or resumed worker, also compare the verified effective role, model, and effort of
that worker with the newly resolved assignment. Continue an existing worker only if these match. If
the match cannot be verified, use a fresh resolved spawn only after the root releases or reassigns
ownership under the strategy's rules. Where the host exposes effective child settings, check them
against the resolved pair before relying on the worker's output.

An unresolved check leaves the affected delegation pending. Report the exact role and pair. When
eligibility is unknown, obtain current callable evidence and rerun the resolver for the same saved
choice. When the selection is absent or ineligible, the root uses the
[role assignment flow](codex-role-assignments.md) to obtain a replacement from the human. Re-run the
resolver before relaunch, including when the ticket already has valid `in_progress` state. Never
reuse another caller's evidence or a previous resolver result. A refusal by the native spawn tool
stops that launch.

A scratch-only matrix has no governed selection target for this check. A no-worker strategy needs no
check.
