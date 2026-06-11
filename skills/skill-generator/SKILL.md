---
name: skill-generator
description: "Use when creating, revising, or auditing an agent skill: writing SKILL.md files, narrowing a skill's scope, or deciding whether a request belongs in a skill at all. Hand platform packaging, installation, and validation to skill-packaging."
---

# Skill Generator

## Job In Hand

Name which job this invocation is before shaping any output:

- **Create**: model the task, confirm the model with the requester, then write the skill.
- **Revise**: re-derive the task model from the existing skill and the complaint, then change only what the model demands.
- **Audit**: judge the skill against its task model and report. Do not edit.

If the job cannot be inferred from the request, ask. The answer changes everything downstream.

## Fit

A skill carries knowledge one kind of task needs and other tasks do not. Route the parts of the request that belong elsewhere: standing conduct to the agent contract, rules that must be guaranteed to tooling, repository-wide knowledge to documentation. When a request spans several tasks, propose the narrowest skill worth having before drafting. When nothing task-shaped remains after routing, report that instead of producing a skill.

## Task Model

Draft the model from the request, the requester's example prompts, and repository facts:

- **Jobs**: the outcomes the skill's output serves. When the request does not name them, ask for the prompts the requester expects to trigger the skill and read the jobs off those. When there is more than one job, the skill must make identifying the job in hand its user's first act, and elicit it when it cannot be inferred.
- **Variation**: what changes between invocations and what every invocation shares. Shared knowledge becomes body content; varying inputs become things the generated skill takes from its own task.
- **Requester-owned choices**: decisions the request leaves open that the skill would otherwise fix for every future use. Ask now when the missing answer would change the artifact's job, its fundamental shape, or a consequence the requester must own; otherwise default conservatively and record the assumption in the model. For choices that vary per invocation, write elicitation into the generated skill.
- **Boundary**: the requests that should activate the skill and the neighbours that route elsewhere, grounded in concrete example prompts; invent the examples and mark them assumed when the requester supplies none.

Show the requester the model, the routing, and the recorded assumptions before writing or changing a body. Corrections land here, where they are cheap.

## Write the Skill

Put the boundary in `description`: the activating kind of request stated as a principle, with neighbouring tasks named to their homes.

Structure the body by the confirmed model. Write each instruction as the action the loading agent takes at that point in the task, with the input it acts on and where its result goes.

Name the folder in lowercase letters, digits, and hyphens. Add `references/`, `scripts/`, or `assets/` only for material the body calls for, each with a load condition in SKILL.md. Run any shipped script, or report why it was not run.

## Prove It

Run the draft in a fresh context against one of the model's example prompts. Compare what the loaded agent does with what the confirmed model demands. Report agreement and divergence alongside the skill; revise from the model and re-test, at most three times. When divergence survives revision, hand it to the requester rather than loosening the model.

## Handoff

Packaging, installing, or validating a skill for a named platform is a separate task. Use `skill-packaging`.
