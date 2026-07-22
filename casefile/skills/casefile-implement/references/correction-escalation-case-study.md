# Correction escalation: HMD-021

HMD-021 began with an ambiguous requirement for transactional binding replacement and retained
history. Review findings expanded that phrase into a crash-recovery journal, a parallel history
tree, and repeated correction rounds. Those were agent interpretations, not an accepted product
architecture.

The human identified the scope drift:

> It's starting to look like we're overcomplicating this if we need transaction recovery.

After further recovery work, the human made the direction explicit:

> This is getting very complicated. Simplify your approach.

The correct escalation point was the first proposal for new durable recovery state. At the latest,
the same concern surviving one correction had become a contention. The root should have drained the
workflow and asked the human instead of sending another correction to the writer. The human rejected
the expansion and selected one-file atomic replacement with Git as the only history authority.

When an agent then proposed a dedicated workflow-test harness, the human classified that as another
unnecessary layer:

> No focused workflow contract tests are needed; that's overengineering the harness. A short case
> study is the required level of evidence.

The human also set the evidence style:

> If you use human evidence in the case study, include my key prompts verbatim. Summaries should be
> the default for your turns and inconsequential exchanges. You may lightly correct grammar and
> wording to make the prompts clearer.

Agent actions and routine approvals are therefore summarized here. The resulting boundary is small:
reviewers propose a classification, root decides, consequential expansion waits for the human, and
optional hardening becomes a non-blocking follow-up.
