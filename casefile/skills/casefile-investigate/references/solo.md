# Solo investigation

Persist the selected `casefile-investigate-solo` matrix even though it has no workers. The root
investigates read-only, arbitrates duplicates, reserves IDs and paths, authors provisional tickets,
verifies evidence, and disposes them. Spawn no worker and never select solo as an implicit fallback.
