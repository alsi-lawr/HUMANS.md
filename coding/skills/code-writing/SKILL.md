---
name: code-writing
description:
  "Use when implementing, debugging, or refactoring code and tests, especially when choosing
  validation, recovery, error-handling, or defensive scope. Do not use for read-only review,
  documentation, Git operations, infrastructure provisioning, or agent-skill authoring."
---

# Code Writing

## Locate Ownership

Before adding a guard, recovery path, error state, or test, identify:

- the supported operation that can reach the state;
- the authoritative data the component owns;
- whether the component, its caller, an operator, or a dependency owns the failing configuration or
  artifact.

Treat an invariant as code-owned when a supported operation can violate it or when failing to
enforce it can damage authoritative state. Treat unsupported operator changes, manually damaged
derived artifacts, and dependency-internal failures as owned by their existing boundary.

## Implement the Owned Boundary

Validate inputs and transitions that the supported API promises to handle. Preserve authoritative
data across stale writes, partial operations, and failures created by the implementation.

Let unowned failures surface through the ordinary dependency or configuration error path. Do not add
a parallel validator, recovery state, repair workflow, or compatibility layer for them. Do not
translate disposable derived state into product authority.

When a requested safeguard crosses that boundary, state what owns the failure and omit it unless the
requester explicitly expands the supported contract.

## Code Comments

Do not add comments that restate what the code does. Prefer self-explanatory names and structure.
Only add comments for non-obvious intent, constraints, workarounds, invariants, or surprising
implementation details. Do not add section-header comments. Do not editorialize or narrate in
comments. A comment must only be used to disambiguate genuinely ambiguous intent.

If in doubt, do not comment. Treat comments as a code smell unless you can genuinely justify it.

## Test Supported Behaviour

A test suite is a maintenance burden: every test must guard something genuinely valuable. Do not
practice TDD and do not chase coverage. Write a test only where correct behaviour is not obvious
from reading the code: emergent, cross-cutting, transactional, concurrent, or boundary-window
behaviour. Where a straightforward read already proves the behaviour, add no test: presence and
registration assertions, copy and string assertions, and signature-echo tests are the burden, not
the guard. Generated code never warrants a test; authored code is judged by the same bar, including
authored fragments inside otherwise generated files.

Trace each test that clears that bar to a supported operation, an owned invariant, or a concrete
regression in this codebase.

Test stale revisions, validation failures, rollback behavior, and data preservation when they are
reachable through supported calls. Do not manufacture coverage by directly corrupting derived
storage, inventing operator misconfiguration, or exercising dependency internals that the product
does not promise to handle.

## Hand Off the Boundary

Report the owned invariants enforced, the supported failures tested, and any unowned failures
deliberately left to their existing boundary.
