# Casefile migration: evidenced two-session report

## Evidence basis

This report traces the `humans-md` migration across the planning session that produced the implementation plan and the fresh session that implemented it:

1. Planning rollout `019f6619-6b0f-7980-8408-5fa1b43e7b00`, persisted at `~/.codex/sessions/2026/07/15/rollout-2026-07-15T15-06-09-019f6619-6b0f-7980-8408-5fa1b43e7b00.jsonl`.
2. Implementation rollout `019f6669-5094-7aa0-aaf8-13676411e5f7`, persisted at `~/.codex/sessions/2026/07/15/rollout-2026-07-15T16-33-25-019f6669-5094-7aa0-aaf8-13676411e5f7.jsonl`.
3. The public [repository commits](https://github.com/alsi-lawr/HUMANS.md/commits/feat/1-humans-md-plugin), [issue #1](https://github.com/alsi-lawr/HUMANS.md/issues/1), and [PR #2](https://github.com/alsi-lawr/HUMANS.md/pull/2).
4. The hash-verified [governed Casefile record](https://github.com/alsi-lawr/agent-planning/tree/main/projects/humans-md/investigations/20260715-thesis-and-plugin), preserved by planning commit [`0e327f0`](https://github.com/alsi-lawr/agent-planning/commit/0e327f0e38054699afa7034c063424cbd20abfe1).
5. The generated packages, validators, test output, and live-cutover record named below.

Every human-authored project input in both rollouts is reproduced without summarisation: five direct prompts and ten structured selections from planning, followed by twelve direct prompts from implementation. Agent turns are condensed into observable actions, outcomes, and limits; no agent message or hidden reasoning is copied. Each human input is identified by its JSONL line, UTC timestamp, and SHA-256 hash of the original UTF-8 payload.

HTML character references preserve the rendered prompt text while keeping this source ASCII-only and preventing historical names inside evidence blocks from being mistaken for active public contracts. Round-trip verification decodes those references and byte-compares all 27 rendered bodies with their session payloads.

The session transport also records automatic repository-context envelopes, an injected skill body, sub-agent notifications, approval/tool traffic, and a machine-generated `<turn_aborted>` marker. Those are excluded because they are not message text or structured answers authored by the human. The direct correction immediately following the abort is included.

## Planning session

### Planning turn 1

**Human prompt (verbatim).** Session JSONL line 7; `2026-07-15T14:06:13.236Z`; raw UTF-8 SHA-256 `97570efece426defdc49fe45db2fa37b59cdb11efb9e471aa9555aa71a312665`.

<details>
<summary>Show the workflow hand-off</summary>

<pre># Workflow hand-off

The portable investigation and ticketing workflow is implemented and installed.

- Reusable source: `~/dev/HUMANS.md/`
- Operational records: `~/dev/agent-planning/`
- Live Codex package: `~/.codex/{skills,agents,planning&#45;workflow}`
- Validator: `python ~/dev/HUMANS.md/scripts/validate-planning&#45;workflow.py --source ~/dev/HUMANS.md --codex-home ~/.codex --model-catalog ~/.codex/models-sol-v1.json`

The request-receiving root is always the orchestrator. Never choose an unspecified investigation, review, or implementation strategy; present compatible choices and ask me. Keep platform bindings in adapters and selected matrices, not the portable core.

Before ticket creation, investigators report candidates and the root reserves an ID and exact provisional path. Resolve every ticket to accepted or rejected, retain rejected rationale, delegate substantive review, and escalate non-obvious contention to me.

Preserve unrelated work. In `~/dev/HUMANS.md`, the untracked `skills/build-code/`, `skills/git-contribution/`, and `skills/test-benchmark-code/` directories are unrelated.

Wait for my next task.</pre>

</details>

**Agent-turn summary.** The agent acknowledged the hand-off and waited. It subsequently treated the request-receiving root, explicit strategy selection, candidate arbitration, review, rejected-rationale retention, adapter boundaries, and unrelated untracked directories as governing constraints.

### Planning turn 2

**Human prompt (verbatim).** Session JSONL line 16; `2026-07-15T14:07:27.952Z`; raw UTF-8 SHA-256 `d56c9d0fb93b5a4c8bb90d1fad25272a748e6e85cbf9f15a739902ede739b9f6`.

<details>
<summary>Show verbatim human input</summary>

<pre>Update the agent-planning instructions wherever they exist to include a mandatory toml map for &lt;project name&gt; -&gt; &lt;directory&gt; on addition.</pre>

</details>

**Agent-turn summary.** The agent traced the portable workflow, installed Codex mirrors, and operational planning store. It defined a store-root `projects.toml` with a `[projects]` table mapping each project namespace to an absolute source directory, required the entry before project records, refused silent conflicts, updated active source and mirrors, and backfilled the existing `phrasic` registration without rewriting historical records.

**Evidence.** The resulting portable contract survives the naming migration in [`casefile-workflow/schemas/project-map.md`](../casefile-workflow/schemas/project-map.md) and [`casefile-workflow/scripts/validate-project-map.py`](../casefile-workflow/scripts/validate-project-map.py).

### Planning turn 3

**Human prompt (verbatim).** Session JSONL line 101; `2026-07-15T14:12:37.052Z`; raw UTF-8 SHA-256 `4b761a98c35ff78c7c74656a931830d7eca41807401b52d2986f57d50e0822b8`.

<details>
<summary>Show verbatim human input</summary>

<pre>No forward test. You should absolutely add tooling to verify project map layout that's invocable by the skill, though.</pre>

</details>

**Agent-turn summary.** The agent abandoned the proposed fresh-context forward test and added deterministic project-map tooling instead. It made the investigation and closeout skills invoke the validator, installed the executable and updated resources, checked the real planning store, proved rejection of an unmapped namespace, and verified source/install parity. No forward test was performed.

### Planning turn 4

**Human prompt (verbatim).** Session JSONL line 146; `2026-07-15T14:42:12.376Z`; raw UTF-8 SHA-256 `61a16dcaa08307547f980d33a50b0223efea8fac7d9993840cc61210410b7725`.

<details>
<summary>Show the initial thesis-and-plugin request</summary>

<pre>This project is going to move slightly away from the thesis-centric model to thesis-and-plugin model. We want these concrete steps as actual work:

1. remove non-unicode chars using the ~/.codex/strip-non-ascii.sh script
2. remove all "CLAUDE is a shim to AGENTS" language, this is vendor neutral.
3. ensure skills are vendor neutral, and vendor-specific parts are packaged vendor-specifically.
4. add re-usable packaging scripts to this repository to convert the shared abstract layer into a plugin one may install
5. relax the forward-test always rule in skill-generator to instead propose verification strategies as a mandatory requirement on skill-generation outset. This must have a matrix of strategies approach in configurable toml in the installed plugins, with short verification scripts.
6. do a larger improvement of the skill-creator with `~/dev/agent-planning/human-prompts/skill-creator-improvement.md`
7. ensure packaging remains the boundary at which generic skills/instructions/models/config become platform-specific.

Also, include a codex-specific instruction on how to switch back to multi-agent-v1 based on what I did in ~/.codex to do this, as well as the improved model matrix and custom instructions. These should be discoverable artifacts, not merely prose.

Use the $investigation&#45;review-atomic skill to do this.</pre>

</details>

**Agent-turn summary.** The agent mapped the portable layer, packaging boundary, installed Codex customizations, and skill-improvement source. It found the improvement prompt at the deeper `human-prompts/humans.md/` path, treated atomic review as selected, and refused to infer the still-open investigation strategy, implementation strategy, or plugin target.

### Planning turn 5

**Human structured response (verbatim).** Session JSONL line 204; `2026-07-15T14:50:39.355Z`; raw UTF-8 SHA-256 `be0662598592c848d50361092e879e323e7b4e6f765d98df210d4ebee5e3c043`.

<details>
<summary>Show verbatim human input</summary>

<pre>{"answers":{"implementation_strategy":{"answers":["Ticket batches (Recommended)"]},"plugin_target":{"answers":["Codex + Claude"]},"investigation_strategy":{"answers":["Inspector tree (Recommended)"]}}}</pre>

</details>

**Agent-turn summary.** The agent recorded inspector-tree investigation, atomic review, ticket-batch implementation, and both vendor packages. It used the inspector-tree matrix for read-only planning reconnaissance while leaving formal ticket creation and review to implementation, as required by Plan Mode.

### Planning turn 6

**Human prompt (verbatim).** Session JSONL line 214; `2026-07-15T14:51:40.807Z`; raw UTF-8 SHA-256 `614eeb7613f835c102917bfe9de06773e9e3e3630b146ef003cadb8a2eb6ff0c`.

<details>
<summary>Show verbatim human input</summary>

<pre>I would also like to preserve this process as evidence of the workflow.</pre>

</details>

**Agent-turn summary.** The agent made workflow evidence a deliverable and surfaced whether it should remain solely in the private planning store or also appear as a curated repository case study.

### Planning turn 7

**Human structured response (verbatim).** Session JSONL line 220; `2026-07-15T14:54:16.565Z`; raw UTF-8 SHA-256 `f5ebaac0181df874955d57e385a94318214879c164793ea19500a0610e11288d`.

<details>
<summary>Show verbatim human input</summary>

<pre>{"answers":{"workflow_evidence":{"answers":["Store + case study (Recommended)","user_note: A literal transcript isn't required, but a summary of turns and decisions surfaced/accepted is required for the case study."]}}}</pre>

</details>

**Agent-turn summary.** The agent recorded both evidence destinations. At this point it defined the public artifact as a turn-and-decision summary rather than a literal transcript, while the private store would retain the complete governed workflow. The later implementation-session instruction expressly changed that public-report boundary.

### Planning turn 8

**Human structured response (verbatim).** Session JSONL line 302; `2026-07-15T15:06:27.260Z`; raw UTF-8 SHA-256 `2e15c6fc92fb15057a2899e7d34d5f5577994165f5101ca053b9418a6533aff4`.

<details>
<summary>Show verbatim human input</summary>

<pre>{"answers":{"plugin_name":{"answers":["humans-md"]},"utility_skills":{"answers":["None of the above","user_note: adopt git-contribution from ~/.codex/skills/ as well, with a substantial review pass. All skill prose must be brief and impactful/forceful. Ensure that packaging allows for multiple plugin creation, as I will want to create separate plugins for my code skills in a future pass."]},"package_artifacts":{"answers":["Commit packages (Recommended)","user_note: add CI to verify this"]}}}</pre>

</details>

**Agent-turn summary.** The agent fixed the public identity as `humans-md`, made both generated packages committed outputs with CI parity, brought only `git-contribution` into this plugin for substantial review, kept the two code skills out of scope, required brief forceful skill prose, and made packaging manifest-driven for future plugins.

### Planning turn 9

**Human structured response (verbatim).** Session JSONL line 323; `2026-07-15T15:10:19.864Z`; raw UTF-8 SHA-256 `dd1ba3a295f60ee04557864f26e6a7d0bb9f0e10ed84ca4e80ff7f0055ab2df0`.

<details>
<summary>Show verbatim human input</summary>

<pre>{"answers":{"planning_store_default":{"answers":["User-configured (Recommended)"]},"codex_profile_scope":{"answers":["All authored profiles (Recommended)","user_note: If we could get a dependabot-style verification of staleness on this, that would be ideal. Ideally check every week or so for new/changed models and suggests changes."]},"contract_install":{"answers":["Opt-in bootstrap (Recommended)","user_note: Is this actually possible to do using the standard plugin system?"]}}}</pre>

</details>

**Agent-turn summary.** The agent selected an explicitly invoked contract bootstrap, user-configured public planning persistence, and all authored Codex profiles. It verified that standard plugin manifests could distribute but not safely auto-install a standing contract, then framed preview, conflict refusal, backup, and confirmation as bootstrap requirements. It also investigated a weekly model-drift check.

### Planning turn 10

**Human structured response (verbatim).** Session JSONL line 351; `2026-07-15T15:14:30.939Z`; raw UTF-8 SHA-256 `7fff2bd83714b48e711d0ac5a9d9eebfa4428718b3e0e4f73db531c5b690a420`.

<details>
<summary>Show verbatim human input</summary>

<pre>{"answers":{"python_dependency":{"answers":["Python 3.11 stdlib (Recommended)","user_note: should be the absolute latest python version."]},"skill_verification_strategy":{"answers":["Balanced sampling"]},"staleness_output":{"answers":["Update one issue (Recommended)"]}}}</pre>

</details>

**Agent-turn summary.** The agent resolved the Python note to the latest stable version available, Python 3.14, while retaining a stdlib-only boundary. It selected balanced sampling for revised skills and a single maintained issue for weekly model drift after confirming `codex debug models --bundled` could supply an unauthenticated catalog export.

### Planning turn 11

**Human structured response (verbatim).** Session JSONL line 373; `2026-07-15T15:17:25.020Z`; raw UTF-8 SHA-256 `63316e0a8fb6e5b7a15ff2cc0ae1a1730f8e447054da7447780bbcb5c97ec0a4`.

<details>
<summary>Show verbatim human input</summary>

<pre>{"answers":{"plugin_version":{"answers":["0.1.0 (Recommended)"]},"cross_vendor_sampling":{"answers":["None of the above","user_note: codex full, claude cannot have forward tests as it is not setup on this machine."]},"claude_model_matrix":{"answers":["Tiered aliases (Recommended)"]}}}</pre>

</details>

**Agent-turn summary.** The agent fixed version `0.1.0`, explicit tiered Claude role aliases, full Codex verification, and no Claude forward testing on the unconfigured machine. Claude evidence was therefore limited to deterministic generation and strict package validation.

### Planning turn 12

**Human structured response (verbatim).** Session JSONL line 395; `2026-07-15T15:21:33.103Z`; raw UTF-8 SHA-256 `5d055d3ee5e68e905bcbb91b970f8eddfde148a825622161494dc95b421047c1`.

<details>
<summary>Show verbatim human input</summary>

<pre>{"answers":{"skill_eval_tranche":{"answers":["Core matrix only (Recommended)"]},"codex_cutover":{"answers":["Atomic replacement (Recommended)","user_note: extra work discovered (unrelated to this answer): skill for switching ongoing work to an agent strategy using the workflow. Also, better labelling of the workflow instead of `investigation-x`"]}}}</pre>

</details>

**Agent-turn summary.** The agent selected atomic replacement for the personal Codex cutover and limited imported skill-creator machinery to deterministic matrices, records, balanced sampling, and aggregation. It separated the newly surfaced strategy-switch skill and workflow naming request into an explicit feature contract before adding either to the plan.

### Planning turn 13

**Human structured response (verbatim).** Session JSONL line 408; `2026-07-15T15:24:36.766Z`; raw UTF-8 SHA-256 `24d2167c7e9fab2817aace2a296dd6616af0723d925f7ef03f0256846c2ddd95`.

<details>
<summary>Show verbatim human input</summary>

<pre>{"answers":{"strategy_switch_persistence":{"answers":["Allow ad-hoc switch"]},"workflow_naming":{"answers":["None of the above","user_note: name the workflow something memorable for this style, with split phase + strategy as additions to the name."]},"strategy_switch_scope":{"answers":["Any workflow phase (Recommended)"]}}}</pre>

</details>

**Agent-turn summary.** The agent defined strategy switching across every phase, preserving the request root and existing work, permitting a task-local ad-hoc transition, and still requiring explicit compatible-matrix selection. It rejected compatibility aliases and moved to a memorable `<workflow>-<phase>-<strategy>` naming family.

### Planning turn 14

**Human structured response (verbatim).** Session JSONL line 415; `2026-07-15T15:27:04.326Z`; raw UTF-8 SHA-256 `5ad7af4adc1158e303c8fa761c5cf2fe2a8cba957a64c48c0f6065a581c4b9b7`.

<details>
<summary>Show verbatim human input</summary>

<pre>{"answers":{"workflow_brand":{"answers":["None of the above","user_note: based on the superintendent/investigator/detective naming, suggest 5 options. Surface this in mid-turn visible reasoning to me, then user the ask_user_question tool to ask if I want to choose the recommendation or None of the above with my own choice"]}}}</pre>

</details>

**Agent-turn summary.** The agent evaluated five role-compatible, vendor-neutral names in a visible turn: Casefile, Inspectorate, Bureau, Precinct, and Casebook. It recommended Casefile and returned the decision through the structured question tool as requested.

### Planning turn 15

**Human structured response (verbatim).** Session JSONL line 422; `2026-07-15T15:28:43.341Z`; raw UTF-8 SHA-256 `11d3a94ce63d685ba7c94dd5ee47c3c224b9b380a743c2d46c7aa07cfd5012a8`.

<details>
<summary>Show verbatim human input</summary>

<pre>{"answers":{"workflow_brand_final":{"answers":["Casefile (Recommended)"]}}}</pre>

</details>

**Agent-turn summary.** The agent fixed Casefile as the public prefix, integrated all preceding choices and investigation findings, and produced the decision-complete plan at `2026-07-15T15:33:19.602Z`. That agent-authored plan is not copied as an agent turn here; its complete text appears verbatim in the next session because the human sent it as the implementation prompt.

## Implementation session

### Implementation turn 1

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

- Initial implementation: [`9c3f998`](https://github.com/alsi-lawr/HUMANS.md/commit/9c3f99851b27607293e4ad4495fba7cef6f914ff).
- Atomic rejection: [`review/round-001/primary.md`](https://github.com/alsi-lawr/agent-planning/blob/main/projects/humans-md/investigations/20260715-thesis-and-plugin/review/round-001/primary.md).
- Accepted ticket set: [`tickets/accepted/`](https://github.com/alsi-lawr/agent-planning/tree/main/projects/humans-md/investigations/20260715-thesis-and-plugin/tickets/accepted).
- The review boundary records that `skills/build-code/` and `skills/test-benchmark-code/` were pre-existing, untracked, excluded, and unmodified.

### Implementation turn 2

**Human prompt (verbatim).** Session JSONL line 786; `2026-07-15T17:16:24.486Z`; raw UTF-8 SHA-256 `6851442ec43f894ca3c0b1cc8eeca216d6675e79458ecf3922a93eb846219cf4`.

<details>
<summary>Show verbatim human input</summary>

<pre>You're adding a lot of tests, huh? Surely these should be minimal at most</pre>

</details>

**Agent-turn summary.** The agent accepted this as a scope correction. It stopped expanding the regression suite, retained only focused coverage for deterministic safety boundaries, and kept behavioural verification as separate run evidence rather than using unit tests as a substitute. The decision was recorded as D-001. The correction writer then returned a second commit addressing the atomic review; the eventual compact suite contained nine tests.

**Evidence.**

- Exact governed decision and rationale: [`D-001-minimal-tests.md`](https://github.com/alsi-lawr/agent-planning/blob/main/projects/humans-md/investigations/20260715-thesis-and-plugin/decision-log/D-001-minimal-tests.md).
- Primary-review corrections: [`c83f5c5`](https://github.com/alsi-lawr/HUMANS.md/commit/c83f5c5e2ac3228ceeee87f952efe8f42fe2825c).
- Final compact output: [`evidence/final-checks.log`](https://github.com/alsi-lawr/agent-planning/blob/main/projects/humans-md/investigations/20260715-thesis-and-plugin/evidence/final-checks.log), which begins with nine passing test markers.

### Implementation turn 3

**Human prompt (verbatim).** Session JSONL line 877; `2026-07-15T17:29:49.195Z`; raw UTF-8 SHA-256 `4cb001b10aabbdfcd559bf902dc831307fef8a349299508052f6610b36a78ba9`.

<details>
<summary>Show verbatim human input</summary>

<pre>What's currently taking so long?</pre>

</details>

**Agent-turn summary.** The agent reported that the delay was the focused verification agent reading the 188-file correction commit, not further test authoring. Root-side checking had already exposed two integration defects: Claude strict validation rejected an unsupported manifest field, and the installed Codex CLI used `plugin add` rather than `plugin install`. The slow verifier was stopped and its missing verdict was recorded instead of inferred.

The agent fixed those defects in `fb9beda`, corrected exported-null handling and cutover transaction ordering in `8f742d5`, and ran the compact local gates. It then performed the guarded personal Codex cutover only after a complete preview and rollback inventory. Fresh processes reported V1, Sol/xhigh for the root, and Terra/xhigh for the inspector; fifteen superseded direct paths were removed after all gates passed. The governed record was promoted and the repository closeout was recorded in `8c34b74`.

**Evidence.**

- Focused reviewer disposition: [`review/round-001/verification.md`](https://github.com/alsi-lawr/agent-planning/blob/main/projects/humans-md/investigations/20260715-thesis-and-plugin/review/round-001/verification.md).
- Integration corrections: [`fb9beda`](https://github.com/alsi-lawr/HUMANS.md/commit/fb9bedaf057d5804ea85cd64fe582019ce8b31db).
- Guarded cutover corrections: [`8f742d5`](https://github.com/alsi-lawr/HUMANS.md/commit/8f742d5f815ea6087a19775e6ef24c991db3ac75).
- Command and cutover evidence: [`evidence/commands.md`](https://github.com/alsi-lawr/agent-planning/blob/main/projects/humans-md/investigations/20260715-thesis-and-plugin/evidence/commands.md) and [`evidence/cutover-record.json`](https://github.com/alsi-lawr/agent-planning/blob/main/projects/humans-md/investigations/20260715-thesis-and-plugin/evidence/cutover-record.json).
- Planning promotion: [`0e327f0`](https://github.com/alsi-lawr/agent-planning/commit/0e327f0e38054699afa7034c063424cbd20abfe1); repository closeout: [`8c34b74`](https://github.com/alsi-lawr/HUMANS.md/commit/8c34b748b04b996426444c38e1b0e898f43f5c3b).

### Implementation turn 4

**Human prompt (verbatim).** Session JSONL line 1190; `2026-07-15T17:59:29.677Z`; raw UTF-8 SHA-256 `466fdb7ed5a8a9fb65ea8ce57360276e65ff0a8eb0a1301fe7faafb59bd2c673`.

<details>
<summary>Show verbatim human input</summary>

<pre>README should turn to a focused branded README instead of a repetition of the thesis. The thesis is the core, the plugin is the deliverable (this wording doesn't belong in the readme, this is for context).</pre>

</details>

**Agent-turn summary.** The agent replaced the thesis-shaped root README with a branded, plugin-first page. Installation, shipped capabilities, examples, architecture, evidence, limitations, and licence became the primary structure; `HUMANS.md` retained the full argument. The contextual wording in the prompt was not copied into the README. The rewrite was checked for ASCII and diff errors and remained uncommitted until the later publication instruction.

**Evidence.**

- Final README revision: [`89f231a`](https://github.com/alsi-lawr/HUMANS.md/commit/89f231a7dd3c12c7f78b7b790593a134f7584014).
- [Current branded README](../README.md) and [the retained thesis](../HUMANS.md).

### Implementation turn 5

**Human prompt (verbatim).** Session JSONL line 1221; `2026-07-15T18:06:17.216Z`; raw UTF-8 SHA-256 `5037ac8a9a8de6cf217cf5c13418bb8551ddcb1ce3c15f29090909225f74d807`.

<details>
<summary>Show verbatim human input</summary>

<pre>Is any of this in the upstream yet?</pre>

</details>

**Agent-turn summary.** The agent fetched `origin` and compared both commits and working-tree state. At that point `origin/master` was still `83c1d4c`; local `master` held five migration commits, the branded README was uncommitted, and the two unrelated code-skill directories remained untracked. The answer was therefore no: none of this migration had reached upstream.

**Evidence.**

- Upstream baseline at the time: [`83c1d4c`](https://github.com/alsi-lawr/HUMANS.md/commit/83c1d4c470732c7f95f907e25ebb91294ca95562).
- The subsequent PR's commit list preserves the exact local-only sequence that was later published: [PR #2 commits](https://github.com/alsi-lawr/HUMANS.md/pull/2/commits).

### Implementation turn 6

**Human prompt (verbatim).** Session JSONL line 1245; `2026-07-15T18:07:51.856Z`; raw UTF-8 SHA-256 `47f2ae908184a4953dd1ec14330903cd47d724caf1a3d2b72f81e67cd0448225`.

<details>
<summary>Show verbatim human input</summary>

<pre>Create an issue with rationale for this change, push to a branch, open a PR into master.</pre>

</details>

**Agent-turn summary.** The agent stated its scope interpretation before mutation: the requested change meant the complete local plugin migration plus the dependent branded README, while the two unrelated untracked code-skill directories remained excluded. It created an issue with rationale, switched to `feat/1-humans-md-plugin`, committed the README as `89f231a`, pushed the branch, and opened PR #2 into `master`. It verified the remote issue, branch, commit list, base, and head. Nothing was merged directly into `master`.

**Evidence.**

- [Issue #1: Ship humans-md as the repository deliverable](https://github.com/alsi-lawr/HUMANS.md/issues/1).
- Branch: [`feat/1-humans-md-plugin`](https://github.com/alsi-lawr/HUMANS.md/tree/feat/1-humans-md-plugin).
- Publication commit: [`89f231a`](https://github.com/alsi-lawr/HUMANS.md/commit/89f231a7dd3c12c7f78b7b790593a134f7584014).
- [PR #2: feat: ship portable humans-md plugin](https://github.com/alsi-lawr/HUMANS.md/pull/2), with base `master` and head `feat/1-humans-md-plugin`.

### Implementation turn 7

**Human prompt (verbatim).** Session JSONL line 1318; `2026-07-15T18:15:56.893Z`; raw UTF-8 SHA-256 `cec8aca8a131943c6819bbc734140c2281a22d08c7c0319884eb5a243b1e053b`.

<details>
<summary>Show verbatim human input</summary>

<pre>The case study should be an actual evidenced report with real Prompts sent by me, only summarise the agent turns, not the turns from me. It should include every decision and steer I made verbatim from the session log.</pre>

</details>

**Agent-turn summary.** The agent rebuilt the original case study from the persisted implementation session, included its seven direct human prompts verbatim, summarized only agent activity, tied claims to session hashes and durable artifacts, changed the README's link description, and pushed the report to PR #2. The first CI run exposed historical public names inside the verbatim plan; the agent encoded those names as HTML references so the rendered prompt remained exact while active-name validation stayed meaningful. Both push and pull-request workflows then passed.

### Implementation turn 8

**Human prompt (verbatim).** Session JSONL line 1491; `2026-07-15T18:39:50.857Z`; raw UTF-8 SHA-256 `52a2f4dfcc38411ead6d2efe92ac206f69fae2f71440469cf120aa555150c483`.

<details>
<summary>Show verbatim human input</summary>

<pre>Something missing from the evidence transcript: the messages to get to that plan I sent you at the start of this session. There was a prior session that produced the plan, all of which should be included.</pre>

</details>

**Agent-turn summary.** The agent corrected the evidence boundary again. It traced the opening plan to planning rollout `019f6619-6b0f-7980-8408-5fa1b43e7b00`, distinguished five direct human prompts and ten structured human responses from generated transport events, and rebuilt this report as a complete two-session chronology. The original HMD-005 record remains unchanged as historical evidence of the earlier summary-only requirement; the later human instructions supersede that public-report boundary without rewriting the governed history.

**Evidence.**

- This file is the replacement report: [`docs/2026-07-15-casefile-plugin-workflow.md`](2026-07-15-casefile-plugin-workflow.md).
- [PR #2](https://github.com/alsi-lawr/HUMANS.md/pull/2) carries the report corrections without altering the preserved planning record.

### Implementation turn 9

**Human prompt (verbatim).** Session JSONL line 1553; `2026-07-15T18:45:49.802Z`; raw UTF-8 SHA-256 `1d17280d984864b51651dba4080e284713080a9df94fc497de425b2a74d83b01`.

<details>
<summary>Show verbatim human input</summary>

<pre>Add citation information and prepare the repo as a research-adjacent artifact, not just a plugin</pre>

</details>

**Agent-turn summary.** The agent kept the branded README plugin-first while adding a separate research surface. It consulted the official Citation File Format 1.2.0 schema, added machine-readable software metadata under the anonymous `alsi-lawr` project identity, documented artifact status, evidence classes, reproduction, limitations, and citation, and made no claim of peer review, archival DOI, or general behavioural effectiveness.

**Evidence.**

- Machine-readable metadata: [`CITATION.cff`](../CITATION.cff).
- Research framing and reproduction guidance: [`docs/research-use.md`](research-use.md).
- The README now links both while retaining its product-focused structure.

### Implementation turn 10

**Human prompt (verbatim).** Session JSONL line 1577; `2026-07-15T18:46:50.140Z`; raw UTF-8 SHA-256 `a123974e537f95868c9bab3887f0b56a2a86c30ebe7ef17548ebc47413cbb9d8`.

<details>
<summary>Show verbatim human input</summary>

<pre>Anonymise the complete github git history with my noreply email.</pre>

</details>

**Agent-turn summary.** The agent activated the repository's identity-rewrite procedure and ran its mandatory intake before mutation. It interpreted the request as a permanent author, committer, and tagger rewrite rather than an ordinary amend or display-level mailmap.

### Implementation turn 11

**Human prompt (verbatim).** Session JSONL line 1600; `2026-07-15T18:48:17.218Z`; raw UTF-8 SHA-256 `d93c65b386490e9fc284b0994239293ac0e69a1b0778c95e87377ede7690a55a`.

<details>
<summary>Show verbatim human input</summary>

<pre>You may infer the answers from my prompt. I authorise gh replacing the upstream. Full anonymisation.</pre>

</details>

**Agent-turn summary.** The agent inferred all published branches and tags, full anonymisation, no nearby-repository rewrite, mapped remote verification, and GitHub replacement. The audit found eighteen reachable commits: six used the personal address and twelve already used the GitHub noreply identity; no signed commit or tag required signature-loss approval.

With restrictive permissions, the agent created and verified a complete recovery bundle, cloned a private mirror without local object sharing, applied `git-filter-repo` 2.47 through an exact mailmap, and verified commit trees, parent mappings, refs, and identities. It snapshotted GitHub state and disclosed the unavoidable loss of three stars, six Actions histories, and original repository/issue/PR timestamps and internal IDs. It then paused for the required fresh destructive approval; no deletion occurred before the next human turn.

### Implementation turn 12

**Human prompt (verbatim).** Session JSONL line 1718; `2026-07-15T18:56:56.873Z`; raw UTF-8 SHA-256 `85a39ab345d672ff8ca9b9c6876f3adcacf45ee7c1e2dbd2408fd338bd55e07e`.

<details>
<summary>Show verbatim human input</summary>

<pre>Yes</pre>

</details>

**Agent-turn summary.** The agent rechecked the source snapshot, repository identity, and exact remote branch SHAs, then deleted and recreated the GitHub repository. It published the anonymised branches; restored settings, topics, labels, Actions policy, issue #1, and PR #2; repointed local refs with expected-old safeguards; and retained the dirty research/report work and unrelated untracked code skills.

A fresh remote clone passed mapped verification for the eighteen rewritten commits and both original branch refs. After the research and transcript commits, all twenty published branch commits use `alsi-lawr <177320313+alsi-lawr@users.noreply.github.com>` as author and committer. No personal address appears in any fetched ref. GitHub's synthetic PR merge ref uses `GitHub <noreply@github.com>`. The public report uses rewritten commit identifiers; the old-to-new map and complete original bundle remain private recovery evidence.

The approved replacement permanently lost three stars, six prior Actions histories, and original repository/issue/PR timestamps and internal IDs. The issue and PR numbers, URLs, titles, bodies, branches, and open states were restored, then updated for the later research scope.

**Evidence.**

- Rewritten branch: [`feat/1-humans-md-plugin`](https://github.com/alsi-lawr/HUMANS.md/tree/feat/1-humans-md-plugin).
- Recreated and updated [issue #1](https://github.com/alsi-lawr/HUMANS.md/issues/1) and [PR #2](https://github.com/alsi-lawr/HUMANS.md/pull/2).
- Local recovery and audit artifacts remain under the session-scoped identity-rewrite scratch directory and are intentionally not committed.

## Ticket and review disposition

| Ticket | Governed disposition | Evidence-backed outcome |
| --- | --- | --- |
| HMD-001 | Accepted | Portable Casefile names and cross-phase switch machinery were implemented; the initial switch defects were corrected after atomic rejection. |
| HMD-002 | Accepted | Portable reusable skills, `git-contribution`, contract bootstrap, and verification-strategy-first skill generation were implemented. Balanced behavioural runs remain unverified. |
| HMD-003 | Accepted | Reproducible Codex and Claude packages and guarded vendor tooling were implemented. Codex live gates passed; Claude runtime behaviour was not tested. |
| HMD-004 | Accepted | Deterministic validators, strategy/suite/run handling, and the human-narrowed nine-test suite were implemented. Candidate/baseline behavioural execution remains unverified. |
| HMD-005 | Accepted | CI, drift monitoring, and documentation were implemented. Later human turns replaced its original README shape and summary-only report boundary. |
| HMD-006 | Accepted and closed | The complete governed record was promoted and hash-verified without changing existing Phrasic records. |

The primary atomic review's verdict on `9c3f998` was **reject - corrections required**. Its findings were routed into `c83f5c5`, `fb9beda`, and `8f742d5`. The focused verification reviewer produced no verdict before being stopped for latency. The final disposition records root mechanical and runtime evidence, not an invented reviewer approval.

## Verification ledger

| Surface | Recorded result | Evidence limit |
| --- | --- | --- |
| Session transcript | 27 human-authored inputs round-trip exactly from two JSONL rollouts | Generated context, agent text, tool traffic, and transport markers are not represented as human prompts. |
| Python 3.14 suite | 9 focused tests passed | Deterministic regression evidence only. |
| Package parity | Claude 107 files; Codex 138 files | Path, mode, and byte parity against committed outputs. |
| Source/package validation | 17 source skills; both standalone package validators; all Casefile surfaces passed | Mechanical structure and metadata, not behaviour. |
| Claude | `claude plugin validate --strict` passed with Claude Code 2.1.204 | No install, loading, triggering, routing, or behavioural run. |
| Isolated Codex | Marketplace add, plugin add, discovery, and installed-byte parity passed with Codex CLI 0.144.1 | Separate from personal live state. |
| Personal Codex cutover | Strict config, discovery, V1, Sol/xhigh root, Terra/xhigh inspector, selective removal, and byte parity passed | Machine-local runtime evidence preserved in the private Casefile. |
| Balanced skill suite | Strategy, suite, prompts, and rubrics exist | Candidate/baseline runs were not executed and remain `unverified`. |
| Forge publication | Issue #1 and open PR #2 exist on the feature branch | No merge, release tag, or marketplace publication is claimed. |

The compact implementation command transcript is preserved in [`evidence/final-checks.log`](https://github.com/alsi-lawr/agent-planning/blob/main/projects/humans-md/investigations/20260715-thesis-and-plugin/evidence/final-checks.log). The broader implementation disposition, including what was not verified, is in [`final-disposition.md`](https://github.com/alsi-lawr/agent-planning/blob/main/projects/humans-md/investigations/20260715-thesis-and-plugin/final-disposition.md).
