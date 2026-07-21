# Decision Schema

A decision record contains ID and title, status, context, human decision, rationale, applicability,
source question or prompt, affected tickets, date, and supersedes/superseded-by links.

Place investigation-only decisions in the investigation, project-reusable decisions in
`projects/<project>/decision-log/`, and cross-project process or preference decisions in the
planning-store root. Elevation moves the sole authoritative record, removes the narrower copy, and
updates references.
