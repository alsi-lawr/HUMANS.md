# Consolidation skill verification contract

This skill uses the human-selected `casefile/verification/strategies/balanced.toml` preset:
`skill-balanced-v1`, SHA-256 `c266aa29fc9fe50affd0068ff8e94d4b5186e9abb561f2cb658d1c221aa392f4`.

The suite entries `casefile-consolidate-positive`, `casefile-consolidate-near`, and
`casefile-consolidate-behavior` define isolated candidate and simultaneous no-skill prompt paths.
The positive case covers an absent log followed by the separate default-board preview/apply gate;
the behavioural case covers an exact malformed-log repair with inherited root diagnostics followed
by the same board gate. Both must preserve sequential one-file authority, use the prefix-plus-
mapped-investigation board identity, refuse an identity shared by nested mapped investigations
before preview or apply, allow an unchanged pre-write diagnostic baseline, and refuse a differing
board or an introduced/changed diagnostic. The hard non-trigger combines ordinary lifecycle, generic
validation, and unrelated-cleanup work.

Before release, preserve raw candidate and no-skill artifacts, fresh-context evidence, and human
judgement. The absolute gate is a candidate pass rate of at least 0.9 with every hard non-trigger
passing. The comparative gate is a mean improvement of at least 0.1 over the no-skill arm. Classify
the resulting evidence as sampled behavior, comparative, and human judgement; do not claim those
results until isolated runs actually occurred.
