# Casefile mutation and format compatibility

Provider protocol 3 returns target-only mutation receipts. Record, batch, progress, strategy and
writer-binding results contain the affected paths, exact resulting target revisions, diffs and
applicable no-op state. They do not contain `resulting_store_revision` and do not claim an atomic
snapshot of other files. Explicit snapshots, reads, audits and advisory-cache revisions retain their
whole-Store meaning. Launchers must negotiate protocol 3; protocol 2 is rejected rather than silently
changing its receipt contract. MCP transport versions are unchanged.

Preview inputs include exact captured target/dependency revisions. Apply revalidates the proposal
against captured inputs while coordinating canonical target paths and genuine read dependencies.
Coordination uses shared read locks and exclusive target locks in deterministic order across Store
instances and processes. Stable sidecars live in Git metadata and survive target replacement or
absent-target creation; they must not be removed while a runtime may be using them. Participating
identity keys additionally prevent concurrent records at different filenames from claiming the same
global ID. There is no project lock, Store lock or session-wide MCP apply lock.

Metadata discovery identifies reference candidates without treating all candidates as protected
inputs. Full bodies are loaded only for targets and actual validation dependencies. Ordinary
related-ticket references are not a transitive read set; supersession reachability is transitive
where cycle validation requires it. Attachments contribute contained regular-file existence, not
attachment contents. The default delivery board is not blocked by an unrelated invalid request or
record merely because it shares an investigation. Explicit audit still reports those diagnostics.

Strategy history written by new runtimes uses schema 2 without a global `expected_store_revision`.
The reader also accepts schema 1 and retains its historical global revision and operation metadata.
Existing history is not rewritten to upgrade it. Legacy replay preserves original bytes and values;
old and new history records can coexist. Root activation and other planning record formats remain
unchanged. Older Casefile runtimes need not understand the new history format.

The coordination boundary is cooperating Casefile mutations. Existing containment, symlink, target
revision and multi-file rollback checks remain. This does not add crash recovery, a persistent
planning mirror, a background service, or coordination for arbitrary external writers. Updating
source and protocol declarations does not install or update a user's runtime.
