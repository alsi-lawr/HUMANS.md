# Skill Verification Schema

A verification strategy is TOML with schema version, strategy ID, mode, required evidence classes,
baseline kind, absolute thresholds, comparative thresholds, and isolation rules. A separate suite
names stable cases, per-skill partitions, realistic prompt files, and hidden rubrics.

A run record binds its canonical payload, strategy, suite, runtime metadata, runtime artifact,
candidate artifact, baseline artifact, isolation artifact, and every raw result artifact by SHA-256.
It records one execution window, UTC bounds, simultaneous-arm and fresh-context claims, one unique
context per case and arm, and one result per required case and arm. Immutable-old-skill baselines
include an immutable reference. No-skill baselines still retain hash-bound raw artifacts. A run
record is evidence only when referenced files exist and hashes match.

Evidence classes are `mechanical`, `sampled_behavior`, `comparative`, `model_judgement`,
`human_judgement`, and `unverified`. Required classes must be present. `unverified` status and class
occur together and contribute no score. Absolute acceptance is evaluated before comparative deltas,
with results reported overall, per skill, and per partition. Prompts contain neither rubrics nor
expected answers. Do not create a run record when execution did not occur.
