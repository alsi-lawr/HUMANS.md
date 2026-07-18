# Research use and citation

## Artifact status

`humans-md` is a research-adjacent software artifact. It brings a stated
argument, an executable instruction system, reproducible vendor packages, and
an evidenced development record into one inspectable repository.

It is not a peer-reviewed paper, a human-subject dataset, or a completed
benchmark. Version `0.1.1` has deterministic and machine-local runtime evidence.
Live use of version `0.1.2` exposed a missing active model-catalog override: its
feature flags loaded, but fresh Sol sessions retained the bundled V2 agent API.
Version `0.1.3` replaces that setup path with a deterministic transaction that
generates and activates the V1 catalog override and has isolated mechanical
coverage. Version `0.1.4` adds bounded implementation look-ahead and ticket
pipelining with adapter-specific low-tier workers; its source and package
contracts are mechanically validated, but its behavioural speed and quality
effects are unverified. Version `0.1.5` simplifies the portable install and
uninstall lifecycle, adds recoverable Claude `CLAUDE.md` setup, and preserves
legacy Codex receipts; Claude skill execution remains unverified. A
fresh-process behavioural replication is still required. No claim of general
effectiveness, causal improvement, or cross-model replication is made.

## Research surfaces

| Surface | Research use |
| --- | --- |
| [`HUMANS.md`](../HUMANS.md) | Thesis, design vocabulary, and cited motivation. |
| [`AGENTS.md`](../AGENTS.md) | The standing behaviour contract under study. |
| [`skills/`](../skills/) | Portable task models that can be compared or revised independently. |
| [`casefile-workflow/`](../casefile/casefile-workflow/) | Roles, schemas, and governance machinery for traced work. |
| [`verification/`](../verification/) | Stable prompts, hidden rubrics, suites, and strategy presets. |
| [`packaging/`](../packaging/) | Product manifests and marketplace catalogs used to reproduce releases. |
| [`humans-md-marketplace`](https://github.com/alsi-lawr/humans-md-marketplace) | Tagged Codex and Claude release trees generated from this source. |
| [Migration report](2026-07-15-casefile-plugin-workflow.md) | Verbatim human inputs, summarized agent turns, decisions, review, and verification provenance. |

The migration report is an authored process record. Its prompts were included
with the author's explicit instruction; it is not offered as a consented corpus
of independent research participants.

## Evidence model

The repository distinguishes six evidence classes:

- `mechanical`: deterministic validation, generation, or configuration checks;
- `sampled_behavior`: observed model behaviour under a recorded task;
- `comparative`: candidate-versus-baseline evidence;
- `model_judgement`: an explicitly identified model assessment;
- `human_judgement`: an explicitly identified human assessment;
- `unverified`: a designed or claimed surface without executed evidence.

Absolute candidate acceptance is evaluated before comparative improvement.
Passing package validation does not establish skill effectiveness, and a model
review does not become deterministic evidence by being recorded.

## Reproduction and evaluation

For a reproducible study or case comparison:

1. Pin the repository commit, package version, runtime version, model, effort,
   feature flags, and selected Casefile matrix.
2. Regenerate both packages and compare their paths, modes, and bytes with the
   selected marketplace tag.
3. Select and record a compatible verification strategy before changing a
   skill; do not silently choose the most favourable strategy after results.
4. Hash the candidate, immutable baseline, suite, prompts, rubrics, and raw run
   artifacts.
5. Run candidate and baseline arms in isolated contexts without exposing
   diagnoses, expected answers, or rubrics to the evaluated model.
6. Report absolute results before candidate-minus-baseline deltas, separated by
   skill and case partition.
7. Preserve failures, missing runs, runtime mismatches, and human judgements as
   first-class outcomes.

The repository's balanced candidate/baseline suite is specified but has not
been executed. Claude loading, triggering, role routing, and behavioural
execution are also unverified. These are open replication surfaces, not
positive findings.

## Citation

Machine-readable citation metadata is in [`CITATION.cff`](../CITATION.cff),
using [Citation File Format 1.2.0](https://github.com/citation-file-format/citation-file-format/blob/main/schema-guide.md).
Until an archival DOI exists, include the evaluated commit in any methods or
artifact statement.

Suggested human-readable citation:

> alsi-lawr. (2026). *humans-md: Portable Behaviour Contracts and Casefile
> Workflows for Coding Agents* (Version 0.1.5) [Computer software]. GitHub.
> https://github.com/alsi-lawr/HUMANS.md

The MIT licence governs software reuse. Citation records provenance and credit;
it does not replace the licence.
