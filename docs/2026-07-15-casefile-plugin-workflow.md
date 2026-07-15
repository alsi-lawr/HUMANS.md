# Casefile migration: evidenced session report

## Evidence basis

This report reconstructs the `humans-md` migration session of 2026-07-15 from four evidence layers:

1. The persisted Codex session `019f6669-5094-7aa0-aaf8-13676411e5f7` at `~/.codex/sessions/2026/07/15/rollout-2026-07-15T16-33-25-019f6669-5094-7aa0-aaf8-13676411e5f7.jsonl`.
2. The public [repository commits](https://github.com/alsi-lawr/HUMANS.md/commits/feat/1-humans-md-plugin), [issue #1](https://github.com/alsi-lawr/HUMANS.md/issues/1), and [PR #2](https://github.com/alsi-lawr/HUMANS.md/pull/2).
3. The hash-verified [governed Casefile record](https://github.com/alsi-lawr/agent-planning/tree/main/projects/humans-md/investigations/20260715-thesis-and-plugin), preserved by planning commit [`0e327f0`](https://github.com/alsi-lawr/agent-planning/commit/0e327f0e38054699afa7034c063424cbd20abfe1).
4. The generated packages, validators, test output, and live-cutover record named below.

Every human-authored project turn in the session is reproduced below without summarisation. Agent turns are condensed into observable actions, outcomes, and limits; no agent message or hidden reasoning is copied. Each prompt is identified by its JSONL line, UTC timestamp, and SHA-256 hash of the original UTF-8 message body. HTML character references preserve the prompt's rendered text while keeping this source ASCII-only and preventing historical names inside the evidence block from being mistaken for active public contracts.

The automatic repository-context envelope and machine-generated `<subagent_notification>` events also use the session's `user` transport role. They are excluded because they are not prompts authored by the human. After that exclusion, the session contains the seven human turns below.

## Session chronology

### Turn 1

**Human prompt (verbatim).** Session JSONL line 7; `2026-07-15T15:33:26.702Z`; raw UTF-8 SHA-256 `19671e43178fa2e646f9e77a8ecd75b0ead1f0fe9f3ebfd6c12ed08cf3723256`.

<details>
<summary>Show the complete implementation prompt</summary>

<pre>A previous agent produced the plan below to accomplish the user's task. Implement the plan in a fresh context. Treat the plan as the source of user intent, re-read files as needed, and carry the work through implementation and verification.

# HUMANS.md Thesis-and-Plugin / Casefile Migration

## Summary

Transform the repository into a vendor-neutral portable instruction system that generates committed, installable `humans-md` plugins for Codex and Claude.

Execution will use the selected workflow:

- Investigation: `investigation&#45;inspector-tree`
- Review: `investigation&#45;review-atomic`
- Implementation: `implementation&#45;ticket-batch`

Register `humans-md -&gt; /home/alex/dev/HUMANS.md` in the planning store and preserve the complete governed record under `projects/humans-md/investigations/20260715-thesis-and-plugin/`. Add a sanitized repository case study summarizing turns, surfaced decisions, accepted choices, ticket dispositions, review, and verification without copying raw transcripts.

## Public Contracts and Naming

- Replace the workflow&#8217;s public names and matrix IDs completely:
  - `casefile-workflow`
  - `casefile-investigate-{solo,atomic,inspector-tree}`
  - `casefile-review-{atomic,dialogue,two-stage}`
  - `casefile-implement-ticket-batch`
  - `casefile-switch-strategy`
  - `casefile-closeout`
- Remove old skill directories, matrix files, IDs, references, and installed copies without compatibility aliases. Preserve historical planning records unchanged.
- `casefile-switch-strategy` must:
  - Work during investigation, review, planning, or implementation.
  - Preserve the request-receiving root and completed/in-progress work.
  - Inventory current state and ownership before delegation.
  - Present compatible matrices and require explicit human selection.
  - Permit an ad-hoc switch recorded under task scratch without forcing ticket creation.
  - Persist through the planning store when work is already governed.
  - Refuse unsafe switches involving overlapping active writers or unavailable capabilities.

Introduce these reusable interfaces:

- `packaging/plugins/&lt;plugin&gt;.toml`: portable plugin identity, selected skills/resources, version, and vendor adapters.
- `scripts/package-plugin.py`: deterministic `build`, `check`, and `--all` operations supporting future separate code-skill plugins without code changes.
- `scripts/validate-skill.py`: Python 3.14 stdlib-only validation of metadata, identity, body, links, paths, package safety, scripts, and vendor metadata.
- `scripts/verify-skill.py`: validate strategy/suite/run records and aggregate candidate-versus-baseline evidence.
- Vendor-installed TOML verification matrices: structural, balanced, and comparative.
- Separate suite TOML with stable case IDs, trigger/behavior surfaces, partitions, prompt files, and rubric files.

## Implementation Changes

### Portable core and skills

- Run `~/.codex/strip-non-ascii.sh` over repository text files at the start and after generation; add a non-mutating ASCII CI check. Do not pass binary assets to the stripping script.
- Delete root `CLAUDE.md`, remove the `.claude/skills` discovery shim, and remove every compatibility/shim claim from README and HUMANS.md. Vendor citations in human rationale may remain.
- Keep all shared skill instructions agent-platform-neutral. Move Codex/Claude models, tool names, configuration, installation, sandbox behavior, and metadata into vendor adapters.
- Audit every included skill for brief, forceful prose while preserving its task model.
- Adopt `git-contribution` from `~/.codex/skills/`, substantially review it, and track it:
  - Keep portable Git/forge conduct generic.
  - Generate the Codex elevation rule only in the Codex package.
  - Move narrow GitHub edge cases to conditionally loaded references where retained.
- Leave `build-code` and `test-benchmark-code` untouched for a future code-skills plugin.
- Replace `skill-generator`&#8217;s universal fresh-context proof with a mandatory verification-strategy decision at task-model outset:
  - Enumerate compatible installed TOML presets.
  - Recommend but never silently choose.
  - Record the selected strategy before drafting.
  - Classify results as mechanical checks, sampled behavior, comparative evidence, model judgement, human judgement, or unverified.
- Add the accepted skill-creator improvements:
  - Positive trigger, difficult near-neighbor non-trigger, and task-behavior cases.
  - Candidate versus no-skill/immutable-old-skill baselines.
  - Isolation from diagnoses, expected answers, and leaked artifacts.
  - Absolute candidate acceptance in addition to comparative delta.
  - Deterministic validators and honest run metadata.
- Defer description optimization, browser viewers, and automated LLM grading. Adapt only the reliable mechanisms from the current Anthropic implementation, not its full workflow or known schema/delta defects. [Anthropic skill-creator source](https://github.com/anthropics/claude-plugins-official/blob/main/plugins/skill-creator/skills/skill-creator/SKILL.md)

### Reusable dual-vendor packaging

- Commit generated packages:
  - `plugins/codex/humans-md/`
  - `plugins/claude/humans-md/`
- Make portable source authoritative; generated packages must be reproducible and byte-compared in CI.
- Declare both packages as version `0.1.0`, publisher `alsi-lawr`, repository `alsi-lawr/HUMANS.md`, licence MIT.
- Bundle all selected skills, Casefile schemas/roles/scripts, verification matrices, contract template, and adapter resources. Packages must not depend on the source checkout or private planning repository.
- Keep planning persistence user-configured. The personal Codex installation may select `/home/alex/dev/agent-planning`, but public defaults must not.
- Provide an explicitly invoked contract-bootstrap skill/script:
  - Preview destination and diff.
  - Refuse ambiguous merging.
  - Back up an existing target.
  - Never install or replace global/repository instructions automatically during plugin installation.

### Codex adapter and live cutover

- Generate `.codex-plugin/plugin.json`, marketplace metadata, packaged skill variants, matrix-bound role profiles, and setup/profile skills.
- Store authored model instructions and a canonical profile TOML, not a copied model catalog.
- Cover all currently customized models; patch only allowlisted instruction/model-message fields and declared `multi_agent_version` selectors.
- Reproduce V1 selection by:
  - Explicitly enabling `multi_agent`.
  - Explicitly disabling `multi_agent_v2`.
  - Setting declared model selectors to JSON `null`.
  - Restarting the Codex host and beginning a new root thread because the version is pinned for a thread&#8217;s lifetime.
- Never edit or distribute `models_cache.json`.
- Build a guarded catalog-profile tool that:
  - Takes a freshly exported catalog as explicit input.
  - Rejects missing/duplicate models and unsupported schemas.
  - Preserves every non-allowlisted field.
  - Creates hash-addressed pristine and last-installed backups.
  - Writes atomically with restrictive permissions.
  - Is idempotent and preserves mtime when unchanged.
  - Restores automatically when strict config or runtime verification fails.
- Preserve these role bindings in the authored matrix:
  - Root preset: Sol/xhigh.
  - Inspector: Terra/xhigh.
  - Detective: Terra/medium.
  - Dialogue chair/challenger: Terra/xhigh.
  - Atomic reviewer: Terra/xhigh.
  - Verification reviewer: Terra/medium.
  - Implementation writer: Terra/high.
- Generate matrix-specific named agent profiles so model/effort are bound at packaging time rather than relying solely on spawn overrides.
- Treat the observed Terra-requested/Sol-executed inspector mismatch as a formal ticket and release blocker until a fresh-process probe confirms exact model and effort.
- Perform an atomic live migration:
  1. Back up relevant direct skills, agents, workflow resources, and config fragments.
  2. Install `humans-md` through a personal Codex marketplace.
  3. Run the opt-in setup/profile tools.
  4. Validate strict config and plugin discovery.
  5. Start a fresh process/root and verify V1 plus exact child profiles.
  6. Remove only superseded direct HUMANS.md copies/config entries.
  7. Restore backups if any gate fails.
- Preserve unrelated Codex skills, plugins, configuration, and the global contract.

### Claude adapter

- Generate a standard `.claude-plugin/plugin.json`, root skill/agent resources, marketplace metadata, and `${CLAUDE_PLUGIN_ROOT}`-relative paths following Anthropic&#8217;s documented plugin structure. [Claude plugin reference](https://code.claude.com/docs/en/plugins-reference), [official plugin directory](https://github.com/anthropics/claude-plugins-official/blob/main/README.md)
- Use explicit tiered aliases:
  - Opus/high for spawning inspectors and review chairs.
  - Sonnet/medium-high for detectives, writers, challengers, and atomic reviewers.
  - Haiku/medium for focused verification.
- Keep model bindings in generated Claude matrices/agent wrappers, never portable skills.
- Validate with `claude plugin validate --strict`.
- Do not install or behaviorally forward-test Claude on this machine; report loading, triggering, and runtime matrix behavior as unverified.

### CI, drift monitoring, and documentation

- Add CI using the latest Python 3.14 patch to:
  - Check ASCII-only text.
  - Run portable validators and script tests.
  - Regenerate both packages and require byte parity.
  - Validate all included skills and matrices.
  - Strictly validate the Claude package.
  - Perform an isolated Codex marketplace/install discovery smoke.
- Add a weekly/manual Codex model-drift workflow:
  - Install the latest official Codex CLI.
  - Export `codex debug models --bundled`.
  - Compare model identities and allowlisted profile-relevant fields.
  - Create or update one labelled drift issue with concrete suggested review.
  - Close the issue when profiles are synchronized.
  - Never auto-edit model instructions or commit a full catalog.
- Rewrite README as a thesis-and-plugin project: claim, portable architecture, generated packages, installation, opt-in contract bootstrap, Casefile workflow, verification evidence, limitations, and licence.
- Add `docs/2026-07-15-casefile-plugin-workflow.md` with the requested summarized turn/decision history and links to durable planning evidence.

## Verification and Acceptance

- Formal investigation candidates receive reserved `HMD-###` IDs only after root arbitration; every ticket ends accepted or rejected with rationale.
- Run substantive disjoint review through the selected atomic review matrix. Reviewers write evidence only; corrections return to investigators.
- Implement accepted tickets through exclusive ticket batches with matrix-declared reviewer and verifier flow.
- Unit-test:
  - Package manifest validation, deterministic generation, and multiple-plugin support.
  - Traversal, symlink escape, missing-resource, empty metadata, and stale generated-output failures.
  - Verification matrix/suite validation and candidate-minus-baseline aggregation.
  - Contract bootstrap preview, conflict refusal, backup, and idempotence.
  - Model-catalog allowlisting, backup/restore, idempotence, schema drift, and stale-model reporting.
  - Casefile strategy selection, ad-hoc transition, unavailable strategy, and overlapping-writer refusal.
- Run the selected balanced Codex skill verification:
  - Positive trigger.
  - Difficult near-neighbor non-trigger.
  - Candidate-versus-baseline task behavior.
  - Apply to every materially revised/adopted skill.
- Run no Claude forward tests; limit Claude evidence to deterministic generation and strict manifest/package validation.
- Validate source/package/install parity, fresh Codex V1 thread selection, exact child model/effort bindings, and removal of superseded direct installs.
- Preserve private request, matrices, candidates, tickets, rejected rationale, review rounds, decisions, implementation plan, command evidence, and final disposition; publish only the sanitized case-study summary.

## Assumptions and Boundaries

- Planning namespace: `humans-md`.
- Plugin version: `0.1.0`; no release tag, push, or marketplace publication is implied.
- Generated vendor packages are committed; run/evaluation evidence remains outside distributable plugins.
- The standard plugin systems distribute the contract bootstrap but do not automatically replace standing instruction files.
- Existing Phrasic planning records and unrelated working-tree changes remain untouched.
- No complete model catalog, runtime cache, Anthropic evaluation UI, description optimizer, or automated LLM grader is imported.
</pre>

</details>

**Agent-turn summary.** The agent reconciled the supplied plan with the repository and found that the earlier investigation artifacts were not available in the fresh context. It imported the accepted requirements into a governed Casefile, selected the named investigation, review, and implementation matrices, reserved HMD-001 through HMD-006, and gave HMD-001 through HMD-005 to one exclusive implementation writer while retaining HMD-006 for root closeout. Host checks established Python 3.14.6, Codex 0.144.1, V1 feature state, and the pre-existing unrelated untracked code-skill directories.

The writer produced the first implementation commit. The matrix-declared atomic reviewer then rejected that commit with concrete P0/P1 findings, including the manifest architecture, V1 selector, governed-switch transaction, package-layout validation, profile binding, live cutover, and missing behavioural evidence. The agent routed those findings back for correction rather than treating mechanical checks as acceptance.

**Evidence.**

- Initial implementation: [`66f3f16`](https://github.com/alsi-lawr/HUMANS.md/commit/66f3f1633e59e3207fb5acd80ca2f1549d826e27).
- Atomic rejection: [`review/round-001/primary.md`](https://github.com/alsi-lawr/agent-planning/blob/main/projects/humans-md/investigations/20260715-thesis-and-plugin/review/round-001/primary.md).
- Accepted ticket set: [`tickets/accepted/`](https://github.com/alsi-lawr/agent-planning/tree/main/projects/humans-md/investigations/20260715-thesis-and-plugin/tickets/accepted).
- The review boundary records that `skills/build-code/` and `skills/test-benchmark-code/` were pre-existing, untracked, excluded, and unmodified.

### Turn 2

**Human prompt (verbatim).** Session JSONL line 786; `2026-07-15T17:16:24.486Z`; raw UTF-8 SHA-256 `6851442ec43f894ca3c0b1cc8eeca216d6675e79458ecf3922a93eb846219cf4`.

<details>
<summary>Show verbatim prompt</summary>

<pre>You're adding a lot of tests, huh? Surely these should be minimal at most</pre>

</details>

**Agent-turn summary.** The agent accepted this as a scope correction. It stopped expanding the regression suite, retained only focused coverage for deterministic safety boundaries, and kept behavioural verification as separate run evidence rather than using unit tests as a substitute. The decision was recorded as D-001. The correction writer then returned a second commit addressing the atomic review; the eventual compact suite contained nine tests.

**Evidence.**

- Exact governed decision and rationale: [`D-001-minimal-tests.md`](https://github.com/alsi-lawr/agent-planning/blob/main/projects/humans-md/investigations/20260715-thesis-and-plugin/decision-log/D-001-minimal-tests.md).
- Primary-review corrections: [`093ba0c`](https://github.com/alsi-lawr/HUMANS.md/commit/093ba0c2a65cfe53814139a0da72e0691b1dd970).
- Final compact output: [`evidence/final-checks.log`](https://github.com/alsi-lawr/agent-planning/blob/main/projects/humans-md/investigations/20260715-thesis-and-plugin/evidence/final-checks.log), which begins with nine passing test markers.

### Turn 3

**Human prompt (verbatim).** Session JSONL line 877; `2026-07-15T17:29:49.195Z`; raw UTF-8 SHA-256 `4cb001b10aabbdfcd559bf902dc831307fef8a349299508052f6610b36a78ba9`.

<details>
<summary>Show verbatim prompt</summary>

<pre>What's currently taking so long?</pre>

</details>

**Agent-turn summary.** The agent reported that the delay was the focused verification agent reading the 188-file correction commit, not further test authoring. Root-side checking had already exposed two integration defects: Claude strict validation rejected an unsupported manifest field, and the installed Codex CLI used `plugin add` rather than `plugin install`. The slow verifier was stopped and its missing verdict was recorded instead of inferred.

The agent fixed those defects in `5e63e7c`, corrected exported-null handling and cutover transaction ordering in `6dc2d3c`, and ran the compact local gates. It then performed the guarded personal Codex cutover only after a complete preview and rollback inventory. Fresh processes reported V1, Sol/xhigh for the root, and Terra/xhigh for the inspector; fifteen superseded direct paths were removed after all gates passed. The governed record was promoted and the repository closeout was recorded in `569d34f`.

**Evidence.**

- Focused reviewer disposition: [`review/round-001/verification.md`](https://github.com/alsi-lawr/agent-planning/blob/main/projects/humans-md/investigations/20260715-thesis-and-plugin/review/round-001/verification.md).
- Integration corrections: [`5e63e7c`](https://github.com/alsi-lawr/HUMANS.md/commit/5e63e7cd51ff0d8a2715b1fc3f2f2df836fca047).
- Guarded cutover corrections: [`6dc2d3c`](https://github.com/alsi-lawr/HUMANS.md/commit/6dc2d3c46ef23723c3880490991c06aef40fe1ee).
- Command and cutover evidence: [`evidence/commands.md`](https://github.com/alsi-lawr/agent-planning/blob/main/projects/humans-md/investigations/20260715-thesis-and-plugin/evidence/commands.md) and [`evidence/cutover-record.json`](https://github.com/alsi-lawr/agent-planning/blob/main/projects/humans-md/investigations/20260715-thesis-and-plugin/evidence/cutover-record.json).
- Planning promotion: [`0e327f0`](https://github.com/alsi-lawr/agent-planning/commit/0e327f0e38054699afa7034c063424cbd20abfe1); repository closeout: [`569d34f`](https://github.com/alsi-lawr/HUMANS.md/commit/569d34f4beb5e12fa91a002f814cedddbca0591a).

### Turn 4

**Human prompt (verbatim).** Session JSONL line 1190; `2026-07-15T17:59:29.677Z`; raw UTF-8 SHA-256 `466fdb7ed5a8a9fb65ea8ce57360276e65ff0a8eb0a1301fe7faafb59bd2c673`.

<details>
<summary>Show verbatim prompt</summary>

<pre>README should turn to a focused branded README instead of a repetition of the thesis. The thesis is the core, the plugin is the deliverable (this wording doesn't belong in the readme, this is for context).</pre>

</details>

**Agent-turn summary.** The agent replaced the thesis-shaped root README with a branded, plugin-first page. Installation, shipped capabilities, examples, architecture, evidence, limitations, and licence became the primary structure; `HUMANS.md` retained the full argument. The contextual wording in the prompt was not copied into the README. The rewrite was checked for ASCII and diff errors and remained uncommitted until the later publication instruction.

**Evidence.**

- Final README revision: [`a1648c9`](https://github.com/alsi-lawr/HUMANS.md/commit/a1648c997b33e8507120c21761023324e51823da).
- [Current branded README](../README.md) and [the retained thesis](../HUMANS.md).

### Turn 5

**Human prompt (verbatim).** Session JSONL line 1221; `2026-07-15T18:06:17.216Z`; raw UTF-8 SHA-256 `5037ac8a9a8de6cf217cf5c13418bb8551ddcb1ce3c15f29090909225f74d807`.

<details>
<summary>Show verbatim prompt</summary>

<pre>Is any of this in the upstream yet?</pre>

</details>

**Agent-turn summary.** The agent fetched `origin` and compared both commits and working-tree state. At that point `origin/master` was still `761878a`; local `master` held five migration commits, the branded README was uncommitted, and the two unrelated code-skill directories remained untracked. The answer was therefore no: none of this migration had reached upstream.

**Evidence.**

- Upstream baseline at the time: [`761878a`](https://github.com/alsi-lawr/HUMANS.md/commit/761878a8b874e989df24cdef3d76b7ef9d178c07).
- The subsequent PR's commit list preserves the exact local-only sequence that was later published: [PR #2 commits](https://github.com/alsi-lawr/HUMANS.md/pull/2/commits).

### Turn 6

**Human prompt (verbatim).** Session JSONL line 1245; `2026-07-15T18:07:51.856Z`; raw UTF-8 SHA-256 `47f2ae908184a4953dd1ec14330903cd47d724caf1a3d2b72f81e67cd0448225`.

<details>
<summary>Show verbatim prompt</summary>

<pre>Create an issue with rationale for this change, push to a branch, open a PR into master.</pre>

</details>

**Agent-turn summary.** The agent stated its scope interpretation before mutation: the requested change meant the complete local plugin migration plus the dependent branded README, while the two unrelated untracked code-skill directories remained excluded. It created an issue with rationale, switched to `feat/1-humans-md-plugin`, committed the README as `a1648c9`, pushed the branch, and opened PR #2 into `master`. It verified the remote issue, branch, commit list, base, and head. Nothing was merged directly into `master`.

**Evidence.**

- [Issue #1: Ship humans-md as the repository deliverable](https://github.com/alsi-lawr/HUMANS.md/issues/1).
- Branch: [`feat/1-humans-md-plugin`](https://github.com/alsi-lawr/HUMANS.md/tree/feat/1-humans-md-plugin).
- Publication commit: [`a1648c9`](https://github.com/alsi-lawr/HUMANS.md/commit/a1648c997b33e8507120c21761023324e51823da).
- [PR #2: feat: ship portable humans-md plugin](https://github.com/alsi-lawr/HUMANS.md/pull/2), with base `master` and head `feat/1-humans-md-plugin`.

### Turn 7

**Human prompt (verbatim).** Session JSONL line 1318; `2026-07-15T18:15:56.893Z`; raw UTF-8 SHA-256 `cec8aca8a131943c6819bbc734140c2281a22d08c7c0319884eb5a243b1e053b`.

<details>
<summary>Show verbatim prompt</summary>

<pre>The case study should be an actual evidenced report with real Prompts sent by me, only summarise the agent turns, not the turns from me. It should include every decision and steer I made verbatim from the session log.</pre>

</details>

**Agent-turn summary.** The agent rebuilt this document from the persisted session rather than extending the earlier sanitized narrative. It included every human-authored project turn verbatim, summarized only agent activity, tied claims to session hashes and durable artifacts, and changed the README's link description from a sanitized record to an evidenced report. The original HMD-005 record remains unchanged as historical evidence of the earlier sanitized-summary requirement; this later, explicit human instruction supersedes that requirement for the report on PR #2.

**Evidence.**

- This file is the replacement report: [`docs/2026-07-15-casefile-plugin-workflow.md`](2026-07-15-casefile-plugin-workflow.md).
- [PR #2](https://github.com/alsi-lawr/HUMANS.md/pull/2) carries the report correction without altering the preserved planning history.

## Ticket and review disposition

| Ticket | Governed disposition | Evidence-backed outcome |
| --- | --- | --- |
| HMD-001 | Accepted | Portable Casefile names and cross-phase switch machinery were implemented; the initial switch defects were corrected after atomic rejection. |
| HMD-002 | Accepted | Portable reusable skills, `git-contribution`, contract bootstrap, and verification-strategy-first skill generation were implemented. Balanced behavioural runs remain unverified. |
| HMD-003 | Accepted | Reproducible Codex and Claude packages and guarded vendor tooling were implemented. Codex live gates passed; Claude runtime behaviour was not tested. |
| HMD-004 | Accepted | Deterministic validators, strategy/suite/run handling, and the human-narrowed nine-test suite were implemented. Candidate/baseline behavioural execution remains unverified. |
| HMD-005 | Accepted | CI, drift monitoring, and documentation were implemented. Turns 4 and 7 later replaced its original README shape and sanitized-report wording. |
| HMD-006 | Accepted and closed | The complete governed record was promoted and hash-verified without changing existing Phrasic records. |

The primary atomic review's verdict on `66f3f16` was **reject - corrections required**. Its findings were routed into `093ba0c`, `5e63e7c`, and `6dc2d3c`. The focused verification reviewer produced no verdict before being stopped for latency. The final disposition records root mechanical and runtime evidence, not an invented reviewer approval.

## Verification ledger

| Surface | Recorded result | Evidence limit |
| --- | --- | --- |
| Python 3.14 suite | 9 focused tests passed | Deterministic regression evidence only. |
| Package parity | Claude 107 files; Codex 138 files | Path, mode, and byte parity against committed outputs. |
| Source/package validation | 17 source skills; both standalone package validators; all Casefile surfaces passed | Mechanical structure and metadata, not behaviour. |
| Claude | `claude plugin validate --strict` passed with Claude Code 2.1.204 | No install, loading, triggering, routing, or behavioural run. |
| Isolated Codex | Marketplace add, plugin add, discovery, and installed-byte parity passed with Codex CLI 0.144.1 | Separate from personal live state. |
| Personal Codex cutover | Strict config, discovery, V1, Sol/xhigh root, Terra/xhigh inspector, selective removal, and byte parity passed | Machine-local runtime evidence preserved in the private Casefile. |
| Balanced skill suite | Strategy, suite, prompts, and rubrics exist | Candidate/baseline runs were not executed and remain `unverified`. |
| Forge publication | Issue #1 and open PR #2 exist on the feature branch | No merge, release tag, or marketplace publication is claimed. |

The compact command transcript is preserved in [`evidence/final-checks.log`](https://github.com/alsi-lawr/agent-planning/blob/main/projects/humans-md/investigations/20260715-thesis-and-plugin/evidence/final-checks.log). The broader disposition, including what was not verified, is in [`final-disposition.md`](https://github.com/alsi-lawr/agent-planning/blob/main/projects/humans-md/investigations/20260715-thesis-and-plugin/final-disposition.md).
