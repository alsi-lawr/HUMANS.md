---
name: readme-generator
description: Use when creating, revising, or auditing a project README. Trigger for requests to write README.md files, improve project presentation, structure installation or usage docs, sharpen a project pitch, or decide how much README structure fits a repository.
---

# README Generator

## Purpose

Create READMEs that explain the project immediately, market it cleanly, and disclose detail in a useful order: description first, pitch next, then the usage, installation, examples, or deeper context the reader actually needs.

Do not force a universal README template. Let the repository, audience, and project maturity determine how much structure belongs in the file, and prefer user-facing sections over contributor or inventory sections unless the README is explicitly for maintainers.

## Fit Gate

Use this skill for project-facing README work: new READMEs, README rewrites, pitch cleanup, installation/usage documentation, and README audits.

Do not use it for:

- Full project websites or landing pages unless the README is also the requested artifact.
- API reference generation that belongs in dedicated docs.
- Changelogs, contributor guides, or architecture docs unless the README needs a short pointer to them.
- Marketing copy detached from the actual repository.

## Required Inputs

Read the existing README and nearby project facts before editing when available: package metadata, app entrypoints, CLI help, tests, examples, docs, screenshots, config files, and visible build commands.

If the project purpose, install path, or audience cannot be inferred safely, ask a narrow question or mark the assumption in the README only when it materially affects users.

## Disclosure Order

Start with the project description. The first screen should answer what the project is before explaining why it matters or how to install it.

Use a staggered arc:

1. **Description**: one or two direct sentences naming the project and what it does.
2. **Elevator pitch**: a short section that explains the value, audience, and distinguishing angle.
3. **Practical next step**: the minimum path for the reader to try, install, use, inspect, or trust the project.
4. **Depth**: architecture, configuration, development, troubleshooting, or repository layout only when the reader needs it.

Prefer progressive disclosure over front-loaded completeness.

## Marketing Style

Market cleanly without inflating claims.

- Lead with concrete value, not slogans.
- Use crisp headings and short paragraphs.
- Prefer proof-bearing specifics over generic adjectives.
- Keep claims consistent with the code and visible project state.
- Use screenshots, badges, or HTML when they improve scanning or presentation.
- Avoid dark-pattern urgency, exaggerated maturity, or feature claims the repo does not support.

HTML is allowed where Markdown is weak, especially for centered headings, badges, screenshots, compact link rows, or layout that GitHub-flavored Markdown cannot express cleanly. Keep HTML readable and portable.

## Structure Choices

Choose sections to fit the README audience. Common user-facing sections include:

- `Overview` or a short pitch section after the description.
- `Features` when the project has user-visible capabilities.
- `Installation` when setup is non-trivial or external users are expected.
- `Usage` or `Quick Start` when users need commands or examples.
- `Examples` when concrete use is clearer than explanation.
- `Configuration` when behavior depends on environment variables or config files.
- `Project Status`, `Roadmap`, or `Limitations` when maturity or constraints affect adoption.
- `License` only when license information exists or the user asks for it.

Use contributor-facing sections like `Development`, `Testing`, `Architecture`, or repository layout only when the README's audience includes maintainers or contributors and the information helps them act.

Skip sections that would be empty, speculative, redundant, or merely prove the repo was inspected. For a tiny script, a description plus usage example may be the right README.

## Workflow

1. Identify the README's audience: users, developers, evaluators, contributors, or a mix.
2. Read the current README and repo facts before changing public claims.
3. Draft or revise the opening description first.
4. Add an elevator pitch-style section that frames value without hype.
5. Add only reader-facing sections the repo can support with facts.
6. Keep installation and usage commands accurate to the repository.
7. Use HTML or visual elements only when they improve presentation and remain maintainable.
8. Review for over-structure, missing first-screen clarity, unsupported claims, and stale commands.

## Ask Or Stop

Ask before inventing:

- The project's purpose when the repo does not reveal it.
- Public positioning, audience, or maturity when it changes how the project is marketed.
- Installation or deployment commands that cannot be inferred from project files.
- Screenshots, logos, badges, or hosted links that are not present.

Stop and surface the issue if the README would need to claim functionality, support status, licensing, security posture, or compatibility that the repository does not show.

## Verification

Check what matters for the README being changed:

- The opening description appears before badges, installation, background, or long explanation.
- The pitch is clear, concrete, and not inflated beyond repo evidence.
- Commands match package scripts, Makefiles, CLI help, docs, or existing conventions.
- Links, image paths, badges, and anchors are plausible in the repo.
- The section structure fits the project instead of following a rigid template.
- Internal structure appears only when it helps the intended reader, not as a default inventory.
- HTML, if used, renders acceptably in common Markdown viewers and does not obscure the source.

Run commands only when needed to verify important setup or usage claims and when doing so is safe.

## Report Back

Leave a review surface shaped to the README work. For small edits, summarize the presentation change and any assumptions. For larger rewrites, call out the new disclosure order, facts verified, and any claims that still need human confirmation.
