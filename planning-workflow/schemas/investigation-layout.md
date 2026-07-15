# Investigation Layout

```text
projects/<project>/investigations/<YYYYMMDD>-<slug>/
  README.md
  request.md
  strategy/{investigation,review,implementation}.toml
  tickets/{provisional,accepted,rejected}/
  decision-log/
  evidence/
  review/round-XXX/
  implementation-plan/PLAN.md
  implementation-plan/tickets/
```

A task-local mirror uses the same shape beneath `<source>/.agent-workspace/<session-id>/agent-planning/`. The root alone synchronises it to durable storage.
