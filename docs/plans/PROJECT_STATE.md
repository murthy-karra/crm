# Project State

Last updated: 2026-08-20

## Current phase

Bootstrap — documentation governance established; no application code exists.

## Current slice

None active. The next slice is Slice 000: executable repository foundation
(workspace layout, scripts, checks) with no CRM product behavior, per the
product thesis §16.

## Current branch

`main` (single writer; no worktrees active).

## Last accepted decision

D-016 (2026-08-20): development environment is the developer's Mac (M1
Max) — API and Vite as local services, PostgreSQL and Centrifugo in
Docker, local username/password auth behind the production auth
abstraction, Cloudflare tunnel on tarams.org protected by Cloudflare
Access. Production remains Kubernetes per D-001.

## Completed work

- 2026-08-20: Repository initialized; initial commit of carried-over
  documents (AGENTS.md, CLAUDE.md, README.md, product thesis, event-sourcing
  research).
- 2026-08-20: Documentation bootstrap committed (`e5af54c`): canonical
  `docs/` structure, decision log (D-001–D-012, O-001–O-005), architecture
  baseline, this state file.
- 2026-08-20: D-013 accepted and recorded; AGENTS.md/README secrets
  statements reconciled; `.gitignore` added.

## Pending work

1. Implementation gate approval for Slice 000 (spec ACCEPTED 2026-08-20).

## Blocking decisions

None for Slice 000. O-006 (outbound messaging consent) blocks the SMS
slice; O-002 (recording consent) blocks recording features.

## Safe defaults adopted

- The event-sourced aggregates document is classified as research
  (precedence level 6), not accepted architecture, because its scope
  conflicts with the thesis's deferred capabilities and D-007. Recorded as
  open decision O-005.
- Root README's development instructions describe the target foundation to
  be rebuilt by Slice 000; its Current State section was updated to reflect
  the fresh reset.

## Latest verification

Not applicable — no code exists. Documentation-only phase.

## Next recommended action

Commit the decision recordings, then plan Slice 000.

## Approval currently required

Implementation gate for Slice 000 ("Proceed with implementation?").
