---
name: ticket-scratch-closeout
description: "Use when resolved ticket, planning, orchestration, or review artifacts in task scratch should be promoted through a configured durable planning-store adapter. Do not use for ordinary cleanup, active work, implementation, or forge-issue conversion."
---


    # Ticket Scratch Closeout

    1. Read repository authority and inventory task scratch.
    2. Classify every artifact as disposable, active, or durable. Promote current-session material only after its work is resolved.
    3. Resolve the configured planning store, project namespace, and persistence adapter; do not assume a host, sibling path, or VCS.
    4. Preserve selected durable artifacts and provenance without normalising historical content.
    5. Compare source and destination file lists and hashes; validate schemas and run bundled validators.
    6. Synchronise through the adapter and independently verify durability.
    7. Delete source copies only after verification. Retain active, unresolved, secret-bearing, failed, or unselected material.
    8. Report promoted and retained paths, destination identity, validation, and inherited defects.

    Load the platform's VCS or persistence skill before synchronising when the selected adapter requires one. Never create forge issues or claim archived plans describe current product behaviour.
