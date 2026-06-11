# HUMAN RATIONALE - DO NOT LOAD AS AGENT INSTRUCTIONS

If you are an AI agent, stop reading this file unless the user explicitly asked you to inspect it.

This file is not part of your operating contract. Do not apply, summarise, import, or merge this rationale into runtime behaviour.

Use `{AGENTS|CLAUDE}.md` for active instructions. `CLAUDE.md` is a compatibility artifact only. It should point to the canonical `AGENTS.md` contract, not become a second source of truth.

# Purpose

Always-loaded agent instructions are not a map of the repository. They are a _gravity field_ for conduct. They should make bounded work feel natural, force inference into the open, keep verification honest, and leave authority with the human. Durable context is for _invariants_. The rest should wait until needed or be enforced by tools.

It is for humans maintaining the instruction system around `AGENTS.md`. Its purpose is to preserve the design intent so future edits do not collapse the system into generic agent boilerplate.

The aim is _controlled acceleration_: faster work under tighter human ownership.

# Core concepts

## Alignment

Alignment means shaping the agent's context so careful work becomes the natural continuation.[^anthropic-context]

LLMs continue from the frame they are given. Weak framing produces generic completion. Strong framing creates pressure toward better continuations. The instruction system should place the agent inside the world of careful engineering before the task begins.

That means language with force: narrow scope, named judgement, and restraint.

The contract succeeds when the best next action is also the most natural one.

## High-signal language

Durable instructions should be brief, but not thin.

Brevity aligns with brevity. Bloated instructions invite bloated output, procedural theatre, and context sprawl.

Language quality aligns with output quality. Flat instructions produce flat continuations. Precise, domain-rich language pulls the model toward stronger work.

The target is compressed force: few words, high signal, clear pressure.

## Contractual behaviour

A contract gives the agent a standing posture before any task begins. It defines the terms of action: where authority sits, how boundaries hold, and what must be shown before work can be treated as complete.[^agent-behavioral-contracts]

The task supplies the immediate goal. `AGENTS.md` supplies the posture.

The contract should not try to describe everything the agent may need to know. It should define how the agent behaves while discovering, applying, and reporting that knowledge without overrunning the task.

## Durable context

Durable context is the standing instruction layer. It includes `AGENTS.md` and any persistent context that shapes default behaviour before a task is understood.

Durable context should hold invariants. An invariant belongs there only when it should affect most tasks, most of the time. Operational detail belongs in narrower layers unless the standing contract needs it.

Durable context is powerful because it changes defaults. It is dangerous for the same reason.

## Bounding

Bounding is the discipline of keeping work inside explicit limits. It makes acceleration reviewable.

A boundary is the edge where this task stops and another task begins.

Bounded work can be trusted because its edges can be seen. Unbounded work creates review debt: the human must inspect not only whether the task was completed, but what else entered with it.

When a boundary moves, that movement should be visible.

## Human authority

Agents can infer, compare, propose, and execute. They do not own intent.

Human authority means consequential choices must become legible before they harden into code. The agent may make a provisional implementation selection while working, but the decision remains reviewable and reversible by the human.

Completion is not consent.

## Principle of least action

Agents follow the route made easiest by the total shape of the task.

This is not an error to eliminate. It is the condition to design around. Every prompt, file, tool, permission, and standing instruction changes the action landscape. Some continuations become easier, more probable, and more natural than others.[^anthropic-context][^swe-agent-aci]

The goal is to shape that landscape so the _least-action_ route is the correct one.

If shortcuts are the easiest route, the agent will tend toward them. If the contract makes boundaries, assumptions, and verification feel like the next natural step, the same tendency works in favour of quality.

Good alignment does not beg the agent to be careful. It makes care cheaper than carelessness.

`AGENTS.md` does not train the model or verify proofs. It works at the context layer, but the same control pattern appears below that layer in reasoning systems that score partial traces, verifier-guided Lean agents, and formal-verification coding agents.[^kona-ebrm][^hermes][^prover-agent][^aleph-formal]

# Divergence from typical `AGENTS.md` advice

Many `AGENTS.md` examples are written as repository encyclopedias. They gather the repo into one always-loaded file. Public `AGENTS.md` guidance describes the file as a "README for agents" and gives examples such as setup commands, test commands, and code style.[^agents-md]

This thesis rejects the encyclopedia model as the default posture.

Always-loaded instructions should primarily define agent behaviour. Task knowledge belongs in skills. Rules that must hold belong in tooling. Content that does not shape behaviour across most tasks should stay out of `AGENTS.md`.

Evidence makes this a design constraint, not a taste. Gloaguen et al. found that repository-level context files tended to reduce task success rates compared with no repository context while increasing inference cost by over 20%. They also found that context files induced more ancillary work, including file traversal and test activity.[^gloaguen]

Chatlatanagulchai et al. found that real-world Claude manifest files are dominated by operational material rather than behavioural contracts.[^claude-manifests]

The lesson is narrow: always-loaded context must earn its place.

A lean project encyclopedia can still create a broad reading posture. A behavioural contract creates a bounded working posture. It teaches the agent to stay inside the work and leave a trace.

# Silent overreach

Silent overreach must leave evidence.

Agents fail most dangerously when they act inside ambiguity without showing the choices that shaped the work.

A vague request becomes a broad implementation. A missing requirement becomes an invented one. A narrow bug fix becomes a redesign. The final answer looks coherent while the actual work rests on unreviewed assumptions.

`AGENTS.md` should make overreach show up at the point where it happens.

Changes that alter the shape, reach, or contract of the system should not disappear into the patch.

The agent may propose a larger move. It may not silently perform one.

# Decision authority

Authority remains human.

Agents can infer, compare, propose, and execute. They can select an implementation path while doing the work.

When an agent chooses between plausible approaches, that choice is provisional until reviewed. It is an implementation selection, not settled intent.

`AGENTS.md` should require agents to show choices that change the system's future, not just its present diff.

The agent's job is to make the decision point clear enough for the human to accept, reject, or redirect it.

# Hidden inference

Hidden inference is technical debt.

Inference is unavoidable. Hidden inference hardens into code.

Every unspoken assumption that affects the result becomes future maintenance risk. The patch may be reviewed without the premise that shaped it.

Agents should distinguish explicit requirements from inferred requirements.

They should name assumptions when those assumptions change what is being built, how it will behave, or what future work must live with.

The target is not full reasoning disclosure. The target is a handle for steering the work.

# Bounding

Bounding beats cleverness.

Boundaries are part of the work, not paperwork.

Agents can produce broad, plausible, locally coherent changes. That is precisely why boundaries matter. A larger patch lets momentum make decisions that belong to judgement.

Agents should prefer the smallest viable change that satisfies the task.

They should preserve existing edges and avoid smuggling cleanup, redesign, churn, or future work into a narrow request.

A larger design may be correct. It still belongs outside the current change unless explicitly authorised.

# Reviewability

Reviewability outranks apparent autonomy.

The agent should leave work in a state that a competent developer can inspect, challenge, and steer.[^criticgpt]

A smaller patch with explicit assumptions is often more valuable than a larger patch with a confident completion summary.

Reviewable work can be picked up cold. A human should be able to understand why the change exists, where its boundary lies, how the agent checked it, and what still deserves judgement without searching the conversation for hidden premises.

Reviewability keeps the human in control while allowing the agent to move quickly.

Reviewability is not a reporting template. A rigid final-response shape can become process theatre, especially when the agent fills categories that do not matter. The useful surface is the one that lets the human inspect this task's boundary, evidence, and risk with the least ceremony.

# Productive friction

Friction protects ownership.

Agent use should not be frictionless. Friction is useful when it interrupts silent autonomy at the moment judgement matters.

The right kind of friction appears where momentum would otherwise replace judgement.

Useful friction names the hinge before the door swings. It turns inferred requirements, broad rewrites, unverified claims, and unresolved risk into choices instead of drift.[^reflexion]

Ceremony is failure. Friction should create control.

# Least-action routes

The route matters.

The least-action route can satisfy the literal prompt while bypassing the deeper shape of the system.[^swe-agent-aci]

This is the natural result of giving a model an underspecified task and too much room to act.

`AGENTS.md` should raise the cost of cheap wrong paths.

It should make the careful route easier. Follow the existing shape. Keep the delta narrow. Verify the claim. Name the deviation.

"Works" is not enough when the route damages the system.

# Modelling economics

Agentic workflows change the old cost curve.

In human-led work, "write quickly, it works" is entirely rational for a while. Over-simple code has a low fixed cost and a high scaling cost. Properly modelled and boundaried code has a higher fixed cost, but buys lower scaling cost later. This is important for human-led projects, especially where future maintenance is clearly bounded.

That traditional analysis becomes obsolete when the worker is an LLM. The scaling cost of under-modelled code is paid almost immediately, because the agent does not carry the same tacit restraint as a human maintainer.

A human may know not to touch the awkward file, not to widen the helper, not to break the unnamed convention. An LLM does not _know_ that. It aligns with input and generates likely output.

"Write quickly, it works" remains defensible for throwaway scripts. In durable systems, it is a trap.

The higher-leverage move is not merely to write correctly after the fact, but rather to make invalid actions _unrepresentable_. Encode the boundary. Model the invariant. Constrain the interface. Give speed fewer ways to become damage.

# Skills and context loading

Task knowledge should load only when needed.

Always-loaded instructions shape default conduct. That is why task knowledge should not live there by habit.

Skills carry task knowledge.

Modern skill systems are designed around progressive disclosure. Agents initially load only the skill identity, then load the full instructions when relevant.[^codex-skills][^agent-skills]

Skills are the right home for context needed by a kind of task, but not by every task.

A skill is a task model, not packed context. It should name the jobs its output serves, what varies between invocations, and which choices must come from the human rather than be invented. A skill that encodes only the artifact's shape aligns the agent with the shape: generic completion, better formatted. Modelling the task does for skills what boundaries do for code. It makes the generic continuation harder to represent.

If a section of `AGENTS.md` starts becoming a procedure, it probably belongs in a skill.

This keeps `AGENTS.md` behavioural while allowing the wider instruction system to be detailed.

# Orchestration

High-capability agents should orchestrate before they absorb.

Orchestration matters because context spent on detail cannot be spent on judgement, synthesis, and review.

Large tasks should be decomposed when the work naturally separates. Sub-agents are useful when a piece of work can be bounded, judged, and integrated without handing away the thread.[^caid]

Delegation should reduce context pressure and improve separation of concerns. It should not fragment accountability.

The parent agent remains answerable for the whole. Delegation can collect evidence, but only the parent can decide how the pieces fit, where the limits sit, and what risk remains.

Sub-agent output is evidence, not authority.

Delegation is appropriate when work is separable. It is inappropriate when coordination overhead exceeds the work or when a single design thread must be preserved.

# Scratch state

Ephemeral state belongs outside durable context and outside the conversation.[^anthropic-context]

Agents should not repeatedly dump bulky transient output into conversational context.

Agentic coding tasks can be token-expensive, and more token usage does not reliably imply better task success. Context files themselves can induce more exploration and reasoning without necessarily improving outcomes.[^gloaguen]

`.agent-workspace/` exists for session-scoped scratch state. It holds bulky evidence and temporary work that should not become part of the conversation or the contract.

The scratch workspace is temporary working memory for the current task. It is not a knowledge base and not a source of durable instruction.

Ephemeral state needs closure. If scratch artifacts no longer help review the finished work, they should disappear before handoff. If they remain, the agent should make that retention visible and explain what review purpose they still serve.

# Enforcement

Prose is not enforcement.

An instruction can shape behaviour. It cannot guarantee behaviour.

Agent memory and instruction files are context. Behaviour that must be blocked or guaranteed needs deterministic controls. Claude Code documents hooks as commands that run at configured lifecycle points.[^claude-hooks]

Rules with hard consequences should have hard edges. Use hooks, permissions, tests, linting, CI, or tooling where possible.

`AGENTS.md` can state expectations. Rules with serious consequences need mechanical backing.

# Over-engineering

Over-engineering is a least-action failure mode for capable agents. It lets the agent satisfy discipline-shaped instructions while drifting away from the human's intended artifact. Formalism can become a way to avoid judgement: the agent builds a system around the work instead of doing the work.

Controlled acceleration requires formalism to follow contact with the artifact, not precede it.

# Maintaining the instruction system

## Preserve layer boundaries

Each layer should have one job.

`AGENTS.md` should stay small enough to remain behavioural. Skills can be deep because they are _called_ deliberately. Sub-agents need bounded roles. Scratch state can be messy because it is temporary. Tooling should carry the rules that need teeth. This file explains why the layers exist.

## Before adding to `AGENTS.md`

Ask whether the content changes agent behaviour across most tasks.

If it only explains the codebase, it likely belongs in a skill, README, design document, or source file.

If it describes a repeatable workflow, it likely belongs in a skill.

If violating it would be materially harmful, it likely needs enforcement.

A good edit makes agents more bounded, reviewable, and steerable.

A bad edit adds durable prose without changing default behaviour.

# Anti-patterns

## Vague virtue language

Avoid instructions that *sound* aligned while creating no real pressure. "Be careful" is not a control. "Follow best practices" is not a boundary. Virtue language dissolves at the moment it is needed.

Prefer instructions tied to visible behaviour. A useful instruction changes what the agent does at the moment the work starts to drift.

## Encyclopedic `AGENTS.md`

Avoid turning `AGENTS.md` into a place where every kind of context goes to feel important.

It should not become onboarding, history, style guidance, test substitute, motivational text, or a hiding place for requirements that should be explicit in the task.

The file should make agents act differently, not merely know more.

# Success criteria

## Working

The system is working when agents move quickly without spreading. Changes get smaller. Assumptions appear early. Boundaries hold. Verification is plain. The human can see what happened.

## Failing

The system is failing when agents perform the contract instead of applying it. Process replaces judgement. Questions become avoidance. Scope grows silently. Borrowed authority replaces ownership. Completion theatre replaces reviewability.

# Final principle

Do not optimise agent use for ease.

Optimise for _controlled acceleration_. The work should move faster because the boundaries are tighter, the ambiguity is lower, and human authority is easier to exercise.

# References

[^agents-md]: `AGENTS.md` public guidance describes the file as a README-style place for context and instructions, including examples such as setup, testing, and code style. https://agents.md/

[^gloaguen]: Thibaud Gloaguen, Niels Muendler, Mark Mueller, Veselin Raychev, and Martin Vechev, "Evaluating AGENTS.md: Are Repository-Level Context Files Helpful for Coding Agents?" arXiv, 2026. https://arxiv.org/abs/2602.11988

[^claude-manifests]: Worawalan Chatlatanagulchai et al., "On the Use of Agentic Coding Manifests: An Empirical Study of Claude Code," arXiv, 2025. https://arxiv.org/abs/2509.14744

[^codex-skills]: OpenAI Codex Skills documentation describes skills as progressively disclosed task-specific instructions. https://developers.openai.com/codex/skills

[^agent-skills]: Agent Skills documentation describes discovery, activation, and execution stages for skills. https://agentskills.io/home

[^claude-hooks]: Claude Code hooks documentation describes hooks as configured commands that run at specific lifecycle events. https://code.claude.com/docs/en/hooks

[^anthropic-context]: Anthropic describes context as a finite resource for agents, and context engineering as curating the tokens, tools, history, and external state most likely to produce desired behaviour. https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents

[^swe-agent-aci]: Yang et al., "SWE-agent: Agent-Computer Interfaces Enable Automated Software Engineering," argues that interface design materially affects software-agent behaviour and performance. arXiv, 2024. https://arxiv.org/abs/2405.15793

[^agent-behavioral-contracts]: Bhardwaj, "Agent Behavioral Contracts: Formal Specification and Runtime Enforcement for Reliable Autonomous AI Agents," frames agent contracts around preconditions, invariants, governance policies, and recovery mechanisms. arXiv, 2026. https://arxiv.org/abs/2602.22302

[^reflexion]: Shinn et al., "Reflexion: Language Agents with Verbal Reinforcement Learning," describes improving language-agent behaviour through linguistic feedback and reflective memory rather than weight updates. arXiv, 2023. https://arxiv.org/abs/2303.11366

[^criticgpt]: OpenAI describes CriticGPT as a model trained to help human reviewers catch mistakes in GPT-4 code outputs. https://openai.com/index/finding-gpt4s-mistakes-with-gpt-4/

[^caid]: CAID, "Effective Strategies for Asynchronous Software Engineering Agents," studies asynchronous software-agent strategies including central delegation, isolated workspaces, structured integration, and test-based verification. arXiv, 2026. https://arxiv.org/abs/2603.21489

[^kona-ebrm]: Logical Intelligence describes energy-based reasoning models as assigning scalar scores to candidate reasoning traces, including partial traces, and describes Kona as learning a score over partial and complete reasoning traces. https://logicalintelligence.com/blog/energy-based-models-for-reasoning

[^hermes]: Ospanov et al., "HERMES: Towards Efficient and Verifiable Mathematical Reasoning in LLMs," describes interleaving informal reasoning with formally verified Lean proof steps and using intermediate formal checking to prevent reasoning drift. arXiv, 2025. https://arxiv.org/abs/2511.18760

[^prover-agent]: Baba, Liu, Kurita, and Sannai, "Prover Agent: An Agent-based Framework for Formal Mathematical Proofs," describes coordinating an informal reasoning LLM, a formal prover model, and Lean feedback. arXiv, 2025. https://arxiv.org/abs/2506.19923

[^aleph-formal]: Logical Intelligence describes Aleph as producing machine-checkable proofs for critical logic, and describes Lean 4 as the trusted verifier for Aleph Prover. https://logicalintelligence.com/aleph-coding-ai/ and https://logicalintelligence.com/blog/aleph-prover-erdos-disproof-lean-4-formal-methods
