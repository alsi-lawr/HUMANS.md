# Investigation Layout

```text
projects.toml
projects/<project>/investigations/<YYYYMMDD>-<slug>/
  README.md
  request.md
  strategy/{investigation,review,implementation,transitions}.toml
  tickets/{provisional,accepted,rejected}/
  decision-log/
  evidence/
  review/round-XXX/
  implementation-plan/PLAN.md
  implementation-plan/tickets/
```

`projects.toml` follows `project-map.md` and must contain the project mapping before `projects/<project>/` or any records beneath it are added.

A task-local mirror uses the same shape beneath `<source>/.agent-workspace/<session-id>/agent-planning/`, including `projects.toml`. The root alone synchronises it to durable storage.
