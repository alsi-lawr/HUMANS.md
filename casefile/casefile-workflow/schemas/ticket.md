# Ticket Schema

```yaml
---
id: T-001
title: Short outcome-oriented title
project: project-slug
investigation: YYYYMMDD-investigation-slug
status: provisional
reported_by_role: detective
reported_by_agent: platform-agent-identifier
source_commit: full-commit-or-documented-uncommitted-state
created_at: YYYY-MM-DDTHH:MM:SSZ
updated_at: YYYY-MM-DDTHH:MM:SSZ
confidence: low
decision_refs: []
related_tickets: []
supersedes: []
superseded_by: []
---
```

The body contains: applicable requirement or invariant; evidence; finding; impact; recommended
resolution boundary; acceptance criteria; verification; relationships and duplicate analysis; review
and disposition history. Directory and frontmatter status must agree. Rejected tickets are resolved
records and require rationale plus decision links where applicable.
