---
name: readme-generator
description: "Use when creating, rewriting, or updating a repository's README, the user-facing capture of a project's intent. Route reference documentation, changelogs, contributing guides, and agent instruction files to their own tasks."
---

# README Generator

## Job In Hand

Name which job this invocation is before shaping any output:

- **Create**: derive the README from a fresh reading of the repository.
- **Maintain**: check the existing README's claims against the repository's current state and change only what no longer holds.

When the request does not settle the job, the presence and state of an existing README usually does. Ask only when neither does.

## Read the Repository

Recover two things from repository facts (manifests, source layout, documentation, history) before drafting a line:

- **Repo type**: the kind of artifact the repository delivers. The type fixes who the reader is and what the README must sell. A library sells a capability and the path to adopting it; an executable sells an outcome and the path to running it; a thesis or research repository sells a claim and the path to engaging with it. Identify the type from evidence; when the evidence is genuinely mixed, ask the requester rather than blending shapes.
- **Intent**: the problem the project exists to solve or the claim it makes, in the project's own terms. A README captures intent. It describes mechanism only where mechanism is what the reader is deciding to adopt. When intent cannot be grounded in repository facts, ask the human who owns the project; do not invent it.

## Shape the Arc

Arrange content as a rough arc, not a template: a one-line description, a hook that earns the reader's next minute, progressively deeper detail, and disclosures at the end (status, limitations, licence, attribution). The repo type decides which stations on the arc exist and how deep each goes. Drop any station the project does not need, and reorder when the project reads better another way.

## Write

Write for the reader the repo type names, in a register that markets honestly: the hook sells what the project actually does, and every claim in the file traces to a repository fact or a recorded requester answer. Use HTML inside the Markdown (alignment, banners, badges, collapsible sections) where it improves the look and feel; presentation carries the content, never replaces it. Target GitHub-flavoured Markdown unless the repository's host says otherwise, and keep anything beyond plain Markdown inside what that host renders.

## Maintain

Let the request set the reading scope. When it localises the drift, read the repository state the touched claims depend on and leave the rest of the file alone. Walk the full README claim by claim, against a fresh reading of the repository, only when the request is about overall staleness or does not say where the drift is. Update claims the project has outgrown, keep voice and structure that still serve the intent, and surface rather than silently rewrite any change that alters what the project claims to be. Drift in fact is yours to fix; drift in intent is the requester's call.

## Present

Hand over the README with the repo type identified, the intent recovered and where it was grounded, and any claim resting on an assumption instead of a repository fact.
