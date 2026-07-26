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
  boards/delivery.toml
  progress/log.toml
  review/round-XXX/
  implementation-plan/PLAN.md
  implementation-plan/tickets/
```

`projects.toml` follows `project-map.md` and must contain the project mapping before
`projects/<project>/` or any records beneath it are added.

This layout exists only in the resolved configured planning store. Read and write every durable
Casefile artifact there directly; never clone or mirror the planning store in task scratch. The
session's `.agent-workspace` is only for disposable, non-authoritative previews, content-hash
backups, isolated output, and command logs.

New Casefiles receive the explicit `boards/delivery.toml` through
`scripts/provision-delivery-board.py`. Existing investigations receive it only through an explicit
`casefile-consolidate` run. The progress log remains independently created or repaired through the
sole progress-transition script; neither record implies or mutates the other.
