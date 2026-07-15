---
name: skill-generator
description: "Use when creating, revising, or auditing an agent skill: model its task, boundary, and verification before drafting. Hand platform packaging and loading diagnostics to skill-packaging."
---

# Skill Generator

## Select the verification contract first

At the outset of every create, revise, or audit task, enumerate installed TOML verification presets, filter them against the task and available runtime capabilities, and show their paths and material constraints. Recommend one compatible preset with reasons, but never select it. Require the human to select that preset or provide a complete ad-hoc TOML strategy, then record its path and SHA-256 in the task model before further modelling or drafting.

The selected strategy records positive triggers, hard near-neighbour non-triggers, task-behaviour cases, isolated prompt paths, absolute acceptance, deterministic validators, evidence classes, and candidate/baseline arms. Baselines are either no-skill or an immutable old-skill reference; never mutate a baseline while testing its candidate. Run candidate and baseline cases in the same evaluation window.

Classify every result as `mechanical`, `sampled_behavior`, `comparative`, `model_judgement`, `human_judgement`, or `unverified`. Never promote sampled or judged evidence to a deterministic claim. Reject strategies that leak diagnoses, rubrics, or expected answers into prompts.

## Model the job

Name the invocation as create, revise, or audit in the recorded task model. Derive jobs, variation, requester-owned choices, and activation boundary from the request, examples, and repository facts. Route standing conduct to the agent contract, guaranteed rules to tooling, repository knowledge to documentation, and packaging to `skill-packaging`. Show the model, routing, assumptions, and selected verification strategy before writing or auditing.

## Write and verify

Put the activation boundary and neighbours in `description`. Write instructions as actions taken at the point they matter. Add resource directories only when the body names their load condition. Use lowercase letters, digits, and hyphens for the folder and run every shipped script.

Execute the selected isolated cases. Enforce absolute acceptance before comparative deltas: a candidate that beats its baseline but misses the contract still fails. Aggregate candidate against its simultaneous no-skill or immutable-old-skill baseline, preserve raw run references, and report divergence without optimizer, viewer, or automated grader substitutions.
