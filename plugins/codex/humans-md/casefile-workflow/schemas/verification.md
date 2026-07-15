# Skill Verification Schema

A verification strategy is TOML with schema version, strategy ID, mode, required evidence classes, baseline kind, absolute thresholds, comparative thresholds, and isolation rules. A separate suite names stable cases, partitions, prompt files, and rubrics. Run records identify strategy and suite hashes, runtime, candidate and baseline artifacts, immutable baseline reference where required, and one result per case and arm.

Evidence classes are `mechanical`, `sampled_behavior`, `comparative`, `model_judgement`, `human_judgement`, and `unverified`. Absolute acceptance is evaluated before comparative deltas. Prompts contain neither rubrics nor expected answers.
