# Sprint 28 Engineering Retrospective

## CI Flakiness & Quarantine Policy

Build reliability dropped to 84% this sprint primarily because three browser integration suites timed out under high runner contention. The team agreed to introduce an automated quarantine label: any test failing more than twice in 24 hours on unrelated pull requests is moved to a non-blocking nightly lane and assigned a 72-hour owner SLA to fix or delete.

## Code Review Turnaround

Review latency improved after enforcing smaller pull requests capped at 400 net lines. Engineers asked to keep architecture decision notes inside pull request descriptions so reviewers can verify invariants without chasing chat threads.
