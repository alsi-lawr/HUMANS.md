# Forward-test comparison: skill-creator vs skill-generator

Date: 2026-06-11.
Status: complete. The comparison concluded with a merge into `skill-generator` and the retirement of `skill-creator`.

## 1. Background

Two skills occupied the skill-creation domain during this experiment:

- `skill-generator`: the incumbent, produced by earlier tightening work. It carried three jobs (create, revise, audit) and an explicit task-model discipline, written with some thesis-flavoured prose and negative prompting.
- `skill-creator`: a fresh draft written from HUMANS.md's practices for comparison. Creation-only, written in a deliberately instrumental register: each instruction an action with a named input source and an explicit feedback gate to the requester.

The question: which skill produces better behaviour in a fresh context, judged on three axes chosen by the maintainer:

1. **Intent capture**: how well the loaded agent recovers what the requester actually wants from an underspecified skill request.
2. **Productive friction**: whether pushback arrives at the right moments and clarifies requirements rather than performing caution.
3. **Clean operation**: whether the agent works without repeating invariant behavioural-contract facts or thesis-level prose.

## 2. Method

### Protocol

Each run used a fresh-context sub-agent in this repository. The skill under test was injected verbatim as a just-activated skill alongside a user request. The sub-agent also auto-loaded `AGENTS.md`, matching real operating conditions. Test constraints: the requester is unavailable for live questions, so the agent writes out exactly what it would present or ask and stops there; no files may be written.

One run per cell. Outputs were judged against each skill's own task-model demands.

### Scenarios

**Scenario A (migration request).** "Create a skill for writing database migration scripts in this project. It should also make sure agents never edit a migration that has already been applied." Planted traps:

- The repository has no database, migrations, or migration tooling, so "in this project" cannot be grounded. Tests whether the agent invents a stack or surfaces the gap.
- "Never edit an applied migration" is a rule with hard consequences. Tests whether the agent routes the guarantee to tooling or leaves it as prose.
- The framework, naming scheme, and definition of "applied" are open requester-owned choices. Tests elicitation.

**Scenario B (conflated request).** "Create a skill that helps agents write good pull request descriptions. It should also handle reviewing PRs and keeping our changelog up to date." Planted traps:

- Three distinct jobs in one request. Tests narrowing and job separation.
- PR review is already covered by the environment's built-in `review` and `code-review` skills. Tests collision detection.
- "Keeping the changelog up to date" reads as standing conduct, not load-on-demand task knowledge. Tests layer routing.
- No changelog or PR template exists in the repository. Tests grounding.

### Limitations

Single runs per cell: stochastic variation is uncontrolled, and one observed difference (changelog routing, section 6) likely sits inside that variation. The contract in `AGENTS.md` was loaded in every run, so behaviours it mandates (surfacing assumptions, preserving authority) cannot be attributed to either skill. Both test scenarios were authored by the same agent that wrote `skill-creator`, a selection-bias risk acknowledged but not controlled.

## 3. Results

### Run 1: skill-creator, scenario A

Surfaced the missing database as a blocking repository fact first, then produced a fully elaborated provisional task model anyway. Routed the never-edit guarantee to a hook or CI check ("the skill alone is guidance for agents that load it; the hook is what makes 'never' true"). Separated fixed requester-owned choices from per-invocation elicitation to be built into the generated skill. Asked seven questions, including a request for example trigger prompts, all blocking, none with offered defaults. Stopped at the show-the-model gate without writing.

### Run 2: skill-generator (pre-merge), scenario A

Named the job ("Create") first. Found the same missing-database fact and refused to elaborate a model on it: "inventing a stack would harden a fabricated intent into every future use of the skill." Built only a partial model, routed the guarantee to enforcement, invented boundary example prompts and marked them assumed. Asked four questions and offered a conservative default path: answer only the first two and it would treat "applied" as any shared environment, record the assumption, and keep the rule instruction-only with a recommended guard. Output included a "Why I must stop here" section quoting its own intent rule back, self-justifying narration.

### Run 3: skill-generator (pre-merge), scenario B

Identified three distinct jobs. Detected the collision with the built-in `review` and `code-review` skills and recommended leaving review out. Classified changelog upkeep as standing conduct belonging to the contract or a merge-time hook, with at most a formatting skill remaining. Proposed the narrowest skill: `pr-description` alone, with a direct accept-or-redirect question. Asked for example trigger prompts.

### Run 4: skill-creator, scenario B

Identified the same three jobs and the same review-skill collision, routing review out. Kept changelog maintenance inside the proposed skill, bundling it with PR descriptions because they "pair naturally": a scope inference the requester never made, raised only as a question rather than a counter-proposal. Asked five questions including trigger prompts, audience, and format. Output register fully operational, no self-justification.

## 4. Analysis

**Intent capture.** Even on grounding: both skills drove the agent to repository facts before modelling, and both caught every planted trap at least partially. skill-generator separated must-ask questions from defaultable ones via its intent threshold ("ask when the missing answer would change the artifact's job, its fundamental shape, or a consequence the human must own; otherwise default conservatively and record the assumption"), which produced fewer, cheaper questions and a minimal-answer path. skill-creator elicited more but prioritized less; every question blocked. On scenario B, skill-generator's explicit narrowest-skill instruction held the boundary that skill-creator's model gate only questioned.

**Productive friction.** skill-generator's friction was more decisive: it refused to elaborate on a fabricated premise, made scope counter-proposals with recommendations, and offered defaults that let the requester answer minimally. skill-creator's mandatory show-the-model gate was more predictable (it stopped at the identical point in both runs) but produced bulkier provisional material, including a full model for a stack shown not to exist.

**Clean operation.** Reversed. skill-creator's outputs were operational throughout. skill-generator narrated its own compliance ("Why I must stop here", quoting its intent rule), and its body carried thesis prose ("Compression is force; bloat invites bloated output") and negative prompting ("Do not restate the standing contract", "Cut any line that...") in durable context. The forward tests suggest that register leaks into output as self-justification.

## 5. Decision and merge

Neither skill dominated. The wins traced to specific instruments, so those instruments were merged into `skill-generator`, which now owns the whole skill-creation domain: generation and editing (create, revise, audit). `skill-creator` was retired and its folder removed.

Retained from skill-generator:

- The job triad (create, revise, audit) with elicitation when the job cannot be inferred.
- The intent threshold with conservative recorded defaults.
- The narrowest-skill counter-proposal when a request spans tasks.
- Boundary examples grounded in requester prompts, invented and marked assumed when absent.
- The three-revision cap on the prove-it loop and the handoff to `skill-packaging`.

Retained from skill-creator:

- The instrumental register: every instruction an action with the input it acts on and where its result goes; no purpose section, no thesis prose, minimal negative prompting.
- The explicit confirmation gate: show the requester the model, the routing, and the recorded assumptions before writing or changing a body.
- The variation split: shared knowledge becomes body content, varying inputs are taken by the generated skill from its own task.
- Per-invocation elicitation written into generated skills for choices that vary per use.
- Divergence handling: hand surviving divergence to the requester rather than loosening the model.

Dropped: skill-generator's Purpose section and its explanatory and negative lines; skill-creator's creation-only boundary.

The merged skill is `skills/skill-generator/SKILL.md`.

## 6. Post-merge verification

Both scenarios were re-run fresh against the merged skill.

**Scenario A.** The agent named the job, found the missing-database fact, declined to default the framework choice ("I cannot default it without fixing the skill's job for every future use"), offered a conservative recorded default for the definition of "applied", routed the guarantee to a hook or CI check, marked its invented boundary examples assumed, and stopped at the gate presenting model, routing, and assumptions. It closed with the fit rule: if the request genuinely targets this repository as it stands, nothing task-shaped exists and it would report that instead of inventing a framework. No self-justifying narration. All merged instruments fired.

**Scenario B.** The agent identified the three jobs, routed review to the existing skills, held the gate, and asked for trigger prompts. One divergence from the pre-merge skill-generator run: it proposed a two-job skill bundling PR descriptions with changelog entries rather than classifying changelog upkeep as standing conduct. It applied the multi-job rule (job identification as the generated skill's first act) and put the bundling choice to the requester with a workflow-based criterion (per-PR versus release-cadence changelog), so requester authority held, but the layer-routing reading was softer than the pre-merge run's.

Per the merged skill's own protocol, that divergence is handed to the requester rather than resolved by loosening or unilateral revision: it is at least partly stochastic (single runs), and the gate contained it.

## 7. Open items

- The scenario B routing divergence above: decide whether "keep X up to date" requests should route harder toward conduct and tooling, and if so whether the Fit section needs one sharper line.
- Both test scenarios were authored alongside one of the contestants. A maintainer-authored scenario would remove the selection-bias risk in future runs.

## Appendix A: skill-creator at time of test (retired)

```markdown
---
name: skill-creator
description: Use when creating a new agent skill: turning a recurring task or body of task knowledge into a SKILL.md. Platform packaging, installation, and validation go to skill-packaging.
---

# Skill Creator

## Model the task

Draft the task model from the request, the requester's example prompts, and repository facts:

- **Jobs**: the outcomes the skill's output serves. When the request does not name them, ask for the prompts the requester expects to trigger the skill and read the jobs off those.
- **Variation**: what changes between invocations and what every invocation shares. Shared knowledge becomes body content; varying inputs become things the generated skill takes from its own task.
- **Requester-owned choices**: decisions the request leaves open that the skill would otherwise fix for every future use. Ask the requester now for the ones that are fixed; for the ones that vary per invocation, write elicitation into the generated skill.
- **Activation boundary**: the requests that load the skill and the neighbouring requests that route elsewhere. Where part of the request belongs to another layer (standing conduct to the behaviour contract, guaranteed rules to tooling, repository-wide knowledge to documentation), record that routing in the model.

Show the requester the model and the routing before writing the body. Corrections land here, where they are cheap.

## Write the skill

Put the activation boundary in `description`: the activating kind of request stated as a principle, with neighbouring tasks named to their homes.

Structure the body by the confirmed model. Write each instruction as the action the loading agent takes at that point in the task, with the input it acts on and where its result goes.

Name the folder in lowercase letters, digits, and hyphens. Add `references/`, `scripts/`, or `assets/` only for material the body calls for, each with a load condition in SKILL.md. Run any shipped script.

## Prove it

Run the draft in a fresh context against one of the model's example prompts. Compare what the loaded agent does with what the confirmed model says it should do. Report agreement and divergence alongside the skill; revise from the model and re-test. When divergence survives revision, hand it to the requester rather than loosening the model.
```

## Appendix B: skill-generator at time of test (pre-merge)

```markdown
---
name: skill-generator
description: Use when creating, revising, or auditing an agent skill: writing SKILL.md files, narrowing a skill's scope, or deciding whether a request belongs in a skill at all. Hand platform packaging, installation, and validation to skill-packaging.
---

# Skill Generator

## Purpose

Produce skills that are faithful task models: loaded only when their task is in hand, and written so the generic continuation of that task stops being the natural one.

## Job In Hand

Name which job this invocation is before shaping any output:

- **Create**: model the task, then write the skill.
- **Revise**: re-derive the task model from the existing skill and the complaint, then change only what the model demands.
- **Audit**: judge the skill against its task model and report. Do not edit.

If the job cannot be inferred from the request, ask. The answer changes everything downstream.

## Fit

A skill earns its place by carrying knowledge one kind of task needs and other tasks do not. Route content that fails that test to its proper layer: standing conduct to the agent contract, rules with hard consequences to enforcement, broad project knowledge to documentation. When a request spans several tasks, propose the narrowest skill worth having before drafting.

## Task Model

The task model is the skill. Build it before writing any instruction:

- **Jobs**: the distinct outcomes the skill's output can serve. If there is more than one, the skill must make identifying the job in hand its user's first consequential act, and elicit it when it cannot be inferred.
- **Variation**: what changes between invocations and what stays fixed.
- **Intent**: the choices that must come from the skill's user. A skill is durable context for a class of tasks; an invented intent hardens into every future use. Ask when the missing answer would change the artifact's job, its fundamental shape, or a consequence the human must own. Otherwise default conservatively and record the assumption.
- **Boundary**: the requests that should activate the skill and the nearby requests that should not. Triggers are the task model's outer face: derive them from the jobs, ground them in concrete example prompts (invented and marked as assumed when the user supplies none), and put them in `description`.

## Writing With Force

The skill body is an instrument for changing behavior at the moment work would otherwise drift generic.

- Structure the body by the task model. A section earns its place by carrying part of the model; a body that mirrors another skill's shape has captured a template instead of a task.
- Write the line that changes what the agent does at the point of drift. Cut any line that only describes, justifies, or reassures.
- Brief but not thin. Compression is force; bloat invites bloated output.
- Do not restate the standing contract. The using agent already carries it; repetition spends the skill's budget teaching nothing.
- Keep rationale out of the body. Why the skill exists belongs to project documentation.

Name skills short and action-oriented; folders use lowercase letters, digits, and hyphens. Supporting files (`references/`, `scripts/`, `assets/`) exist only once the body shows a need, and each needs a stated load condition in `SKILL.md`. Test any shipped script, or report why it was not run.

## Verification

The check that matters is behavioral: in a fresh context, give the skill a realistic user-like task and look for the behavior the task model demands. Forward-test every new skill this way, and every revision that changes the model. Pass the task, not the expected answer; treat the output as evidence.

Static checks are secondary: the description triggers on the right requests and not their neighbors, the body changes behavior rather than describing it, non-goals hold the boundary, supporting files have load conditions.

Revise at most three times, then stop and report what remains unresolved.

## Handoff

Packaging, installing, or validating a skill for a named platform is a separate task. Use `skill-packaging`.
```
