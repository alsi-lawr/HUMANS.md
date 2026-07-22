# Ticket-batch implementation

Require the selected `casefile-implement-ticket-batch` matrix. Give the writer a dependency-safe
accepted batch under exclusive ownership. Review and verify its immutable ticket commits before
assigning the next batch. Return corrections to the same writer and complete each ticket only after
its recorded review flow accepts it.

For Codex, resolve the Casefile writer binding with strategy ID `casefile-implement-ticket-batch`
immediately before the initial writer, each resumed batch, and each correction spawn. Use only the
returned spawn arguments; an unavailable pair stops the batch before mutation pending explicit
inactive reselection.
