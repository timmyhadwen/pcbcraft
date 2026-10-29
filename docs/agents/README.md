# Agent role briefs

Token-sized summaries for parallel headless workers. Canonical rules: [AGENTS.md](../../AGENTS.md), [CONTRIBUTING.md](../../CONTRIBUTING.md), fork onboarding [issue #1](https://github.com/wdunn001/pcbcraft/issues/1).

| Role | Find work | Output | Lease |
| --- | --- | --- | --- |
| Researcher | `stage:research` | `### Research` comment, `research:done` | 2 h |
| Enricher | `stage:enrich` | `### Enrichment`, full Tags line, `ready` | 2 h |
| Worker | `ready` + `status:open` (+ `conflict:parallel-safe` if shared host) | branch + green fork Actions + build report | 6 h |
| Reviewer | branch with report | `### Review` (read/run only, no edits) | 2 h |
| Integrator | approved branches | merge to fork `main`, green Actions there | maintainer |

**Parallelism:** prefer many workers on `conflict:parallel-safe` issues; one agent per clone; never share checkouts. **CI:** fork Actions is proof; local `cargo xtask ci` optional.

Role files (optional detail): [researcher.md](researcher.md), [enricher.md](enricher.md), [worker.md](worker.md), [reviewer.md](reviewer.md).

## Fleet rules

## Fleet rules

Workers (Cloud Agent fleet on the fork wdunn001/pcbcraft) push a `<kind>-<slug>` or `feature/<slug>-db36` branch only. Green GitHub Actions on that branch is the proof. Post the upstream build report ending with `PR pending owner review`. Do not open a pull request on the fork and do not use forge PR tools to create one. The maintainer or integrator lands work on fork `main` with a merge commit after review.

Integrator (same fleet): merge contributor branches into fork `main` with `git merge` (not `gh pr merge`). Close any stray worker-opened fork PRs with `closed` status so the branch and the Actions run remain the record. The upstream PR to tangency-labs waits until fork `main` is green and the owner has run the packaged test gate. Never ask the owner to close PRs or merge manually.
