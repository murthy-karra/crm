# Decision reading index

Navigation only, checked against the log on 2026-09-15: D-001–D-086 and O-001–O-015.
[DECISION_LOG.md](DECISION_LOG.md) remains the highest authority. Labels identify
where to read; they do not summarize complete policy, grant approval, or override
later amendments. Open/resolved labels are copied from the log's headings;
read the full section for partial resolutions and remaining blockers.

## Reading procedure

1. Read/retain AGENTS.md's core invariants (D-001–D-012) and full D-015/D-050.
2. Scan this table and the log's headings (`rg '^### [DO]-' docs/decisions/DECISION_LOG.md`).
   Read full sections for applicable topics, decisions named by the task/spec,
   and any new or changed heading absent here. Include all amendments/release
   follow-ups within each section and follow referenced dependencies.
3. Check adjacent domains: mobile writes still depend on native command/Today
   rules; migration depends on privacy, source families and the review hold;
   release work depends on environment, secrets and exact release authorization.
   Missing a tag is not evidence that a decision is irrelevant. Widen reading
   when uncertain; a full-log read remains appropriate for a broad policy audit.
4. Reuse unchanged sections already in the task context. At handoff, preserve
   the relevant IDs, source revision, approvals and unresolved questions.

Maintain every ID, exact title/link and applicability label when the log changes.
The original decision sections, including superseded choices, remain intact.

## Accepted-decision locations

| Decision | Applicability | Exact heading (dates omitted) |
|---|---|---|
| [D-001](DECISION_LOG.md#d-001--technology-stack) | core | Technology stack |
| [D-002](DECISION_LOG.md#d-002--modular-application-not-microservices) | core | Modular application, not microservices |
| [D-003](DECISION_LOG.md#d-003--authentication-and-authorization-are-separate) | core | Authentication and authorization are separate |
| [D-004](DECISION_LOG.md#d-004--organization-is-the-tenant-boundary) | core | Organization is the tenant boundary |
| [D-005](DECISION_LOG.md#d-005--initial-person-visibility-is-organization-wide) | core | Initial Person visibility is Organization-wide |
| [D-006](DECISION_LOG.md#d-006--person-inquiry-deal-and-identity-are-distinct) | core | Person, Inquiry, Deal, and Identity are distinct |
| [D-007](DECISION_LOG.md#d-007--hybrid-persistence) | core | Hybrid persistence |
| [D-008](DECISION_LOG.md#d-008--one-typed-command-layer-for-all-clients) | core | One typed command layer for all clients |
| [D-009](DECISION_LOG.md#d-009--application-enforced-operator-action-risk) | core | Application-enforced Operator action risk |
| [D-010](DECISION_LOG.md#d-010--deterministic-explainable-today-ranking) | core | Deterministic, explainable Today ranking |
| [D-011](DECISION_LOG.md#d-011--realtime-is-delivery-not-truth) | core | Realtime is delivery, not truth |
| [D-012](DECISION_LOG.md#d-012--migration-fidelity-is-a-product-capability) | core | Migration fidelity is a product capability |
| [D-013](DECISION_LOG.md#d-013--development-secrets-use-a-local-env-file-2026-08-20) | dev; secrets | Development secrets use a local .env file |
| [D-014](DECISION_LOG.md#d-014--production-secrets-manager-is-openbao-2026-08-20) | production; secrets | Production secrets manager is OpenBao |
| [D-015](DECISION_LOG.md#d-015--event-sourcing-scope-and-pii-handling-2026-08-20) | core; privacy; history | Event-sourcing scope and PII handling |
| [D-016](DECISION_LOG.md#d-016--development-environment-2026-08-20) | dev; identity; services | Development environment |
| [D-017](DECISION_LOG.md#d-017--web-frontend-styling-and-data-stack-2026-08-21) | Web; state | Web frontend styling and data stack |
| [D-018](DECISION_LOG.md#d-018--production-ingress-cloudflare-tunnel-to-cilium-gateway-2026-08-21) | production; ingress | Production ingress: Cloudflare Tunnel to Cilium Gateway |
| [D-019](DECISION_LOG.md#d-019--person-stages-are-a-per-organization-list-not-a-fixed-enum-2026-08-21) | stages | Person stages are a per-Organization list, not a fixed enum |
| [D-020](DECISION_LOG.md#d-020--hot-prospect-carries-a-stage-marker-no-other-stage-does-2026-08-21) | stages; Today | Hot Prospect carries a stage marker; no other stage does |
| [D-021](DECISION_LOG.md#d-021--slice-004-is-administration-platform-admin-invitations-no-direct-database-writes-2026-08-21) | admin; invitations | Slice 004 is administration: platform admin, invitations, no direct database writes |
| [D-022](DECISION_LOG.md#d-022--a-contact-attempt-is-a-recorded-fact-and-the-unit-of-response-for-today-2026-08-21) | contact; Today | A contact attempt is a recorded fact and the unit of response for Today |
| [D-023](DECISION_LOG.md#d-023--realtime-model-organization-channel-server-side-subscriptions-ids-only-events-2026-08-21) | realtime | Realtime model: Organization channel, server-side subscriptions, ids-only events |
| [D-024](DECISION_LOG.md#d-024--cloudflare-access-removed-from-the-dev-tunnel-2026-08-22) | dev; ingress | Cloudflare Access removed from the dev tunnel |
| [D-025](DECISION_LOG.md#d-025--the-dev-cloudflare-tunnel-is-dashboard-managed-not-file-managed-2026-08-22) | dev; ingress | The dev Cloudflare Tunnel is dashboard-managed, not file-managed |
| [D-026](DECISION_LOG.md#d-026--organization-admin-continuity-last-admin-protection-platform-admin-recovery-members-unaffected-2026-08-22) | admin; recovery | Organization admin continuity: last-admin protection, platform-admin recovery, members unaffected |
| [D-027](DECISION_LOG.md#d-027--membership-deactivation-instead-of-removal-organization-suspension-reserved-2026-08-22) | membership; auth | Membership deactivation instead of removal; Organization suspension reserved |
| [D-028](DECISION_LOG.md#d-028--the-ai-operator-is-an-in-process-crate-not-a-separate-service-2026-08-22) | Operator; architecture | The AI Operator is an in-process crate, not a separate service |
| [D-029](DECISION_LOG.md#d-029--operator-turns-are-audited-as-a-pii-free-ledger-transcripts-are-not-stored-2026-08-22) | Operator; audit | Operator turns are audited as a PII-free ledger; transcripts are not stored |
| [D-030](DECISION_LOG.md#d-030--slice-006-is-human-initiated-outbound-browser-calling-operator-calling-is-006b-2026-08-22) | calling; scope | Slice 006 is human-initiated outbound browser calling; Operator calling is 006b |
| [D-031](DECISION_LOG.md#d-031--a-call-becomes-the-contact-attempt-at-answerfailure-time-2026-08-22) | calling; contact | A call becomes the contact attempt at answer/failure time |
| [D-032](DECISION_LOG.md#d-032--agents-may-correct-a-calls-outcome-after-the-call-2026-08-22) | calling; corrections | Agents may correct a call's outcome after the call |
| [D-033](DECISION_LOG.md#d-033--the-agent-must-choose-every-calls-outcome-until-then-the-call-is-incomplete-and-the-person-stays-on-today-in-a-lowest-outcome-needed-tier-2026-08-23) | calling; Today | The agent must choose every call's outcome; until then the call is incomplete and the Person stays on Today in a lowest "outcome needed" tier |
| [D-034](DECISION_LOG.md#d-034--006b-adds-no-crm-operator---crm-app-edge-mutation-tools-go-through-the-toolbackend-seam-2026-08-23) | Operator; commands | 006b adds no `crm-operator -> crm-app` edge; mutation tools go through the ToolBackend seam |
| [D-035](DECISION_LOG.md#d-035--unattended-intake-routes-to-an-admin-set-organization-default-assignee-2026-08-24) | intake; assignment | Unattended intake routes to an admin-set Organization default assignee |
| [D-036](DECISION_LOG.md#d-036--forged-mail-posture-for-pinned-email-formats-2026-08-25) | email; trust | Forged-mail posture for pinned email formats |
| [D-037](DECISION_LOG.md#d-037--raw-unresolved-content-is-readable-by-organization-admins-only-2026-08-25) | raw content; auth | Raw unresolved content is readable by Organization admins only |
| [D-038](DECISION_LOG.md#d-038--inbound-lead-email-content-may-be-sent-to-groq-for-extraction-under-a-fixed-scope-2026-08-25) | email; AI; privacy | Inbound lead-email content may be sent to Groq for extraction, under a fixed scope |
| [D-039](DECISION_LOG.md#d-039--final-intake-address-scheme-is-local-part-receiving-via-cloudflare-email-routing--an-email-worker-relay-2026-08-25) | email; routing | Final intake address scheme is local-part; receiving via Cloudflare Email Routing + an Email Worker relay |
| [D-040](DECISION_LOG.md#d-040--unwrapped-forwarded-mail-may-match-pinned-formats-with-typed-provenance-2026-08-25) | email; provenance | Unwrapped forwarded mail may match pinned formats, with typed provenance |
| [D-041](DECISION_LOG.md#d-041--intake-routing-modes-three-mode-picker-with-round-robin-2026-08-26) | intake; routing | Intake routing modes: three-mode picker with round-robin |
| [D-042](DECISION_LOG.md#d-042--correspondence-capture-v1-the-six-scoping-decisions-2026-08-27) | email; capture | Correspondence capture v1: the six scoping decisions |
| [D-043](DECISION_LOG.md#d-043--smart-lists-are-first-class-and-become-todays-configurable-feed-2026-08-28) | lists; Today | Smart lists are first-class and become Today's configurable feed |
| [D-044](DECISION_LOG.md#d-044--elysium-crm-identity-and-threshold-e-logo-direction-2026-09-03) | Web; brand | Elysium CRM identity and Threshold E logo direction |
| [D-045](DECISION_LOG.md#d-045--attio-inspired-white-workspace-with-selective-glass-2026-09-06) | Web; design | Attio-inspired white workspace with selective glass |
| [D-046](DECISION_LOG.md#d-046--saved-list-privacy-and-separate-personalshared-limits-2026-09-06) | lists; privacy; limits | Saved-list privacy and separate personal/shared limits |
| [D-047](DECISION_LOG.md#d-047--today-source-limit-and-partial-availability-2026-09-06) | Today; limits | Today source limit and partial availability |
| [D-048](DECISION_LOG.md#d-048--a-saved-lists-sort-order-is-part-of-its-definition-2026-09-06) | lists; sort | A saved list's sort order is part of its definition |
| [D-049](DECISION_LOG.md#d-049--slice-011d-ships-as-one-l-rung-with-parallel-backend-and-web-lanes-2026-09-06) | 011d; ownership | Slice 011d ships as one L rung with parallel backend and web lanes |
| [D-050](DECISION_LOG.md#d-050--operating-envelope-verification-budget-and-performance-gating-2026-09-07) | core; verification; performance | Operating envelope, verification budget and performance gating |
| [D-051](DECISION_LOG.md#d-051--tag-rename-and-delete-admins-plus-the-creator-while-the-tag-is-unused-2026-09-07) | tags; auth | Tag rename and delete: admins, plus the creator while the tag is unused |
| [D-052](DECISION_LOG.md#d-052--trigger-maintained-derived-columns-are-a-read-model-mechanism-2026-09-08) | read models; DB | Trigger-maintained derived columns are a read-model mechanism |
| [D-053](DECISION_LOG.md#d-053--notes-ship-as-plaintext-erasable-crud-o-012-amended-2026-09-08) | notes; erasure | Notes ship as plaintext erasable CRUD; O-012 amended |
| [D-054](DECISION_LOG.md#d-054--due-tasks-reach-today-through-a-fixed-built-in-axis-plus-a-task-panel-2026-09-09) | tasks; Today | Due tasks reach Today through a fixed built-in axis plus a task panel |
| [D-055](DECISION_LOG.md#d-055--development-telephony-host-on-ec2-egress-present-but-dormant-2026-09-09) | telephony; dev | Development telephony host on EC2; egress present but dormant |
| [D-056](DECISION_LOG.md#d-056--inbound-mail-size-cap-at-cloudflares-25-mib-ceiling-the-relay-streams-2026-09-09) | email; size limits | Inbound mail size cap at Cloudflare's 25 MiB ceiling; the relay streams |
| [D-057](DECISION_LOG.md#d-057--operator-complete_task-executes-with-a-receipt-and-undo-create_task-proposes-2026-09-10) | Operator; tasks | Operator `complete_task` executes with a receipt and Undo; `create_task` proposes |
| [D-058](DECISION_LOG.md#d-058--custom-fields-v1-four-typed-kinds-member-set-values-archive-only-definitions-filtering-deferred-2026-09-10) | custom fields | Custom fields v1: four typed kinds, member-set values, archive-only definitions, filtering deferred |
| [D-059](DECISION_LOG.md#d-059--first-fub-migration-targets-a-new-empty-organization-2026-09-10) | migration; destination | First FUB migration targets a new, empty Organization |
| [D-060](DECISION_LOG.md#d-060--fub-assessment-uses-api-first-access-and-encrypted-saved-credentials-2026-09-10) | migration; assessment; credentials | FUB assessment uses API-first access and encrypted saved credentials |
| [D-061](DECISION_LOG.md#d-061--010b-captures-core-records-first-and-explicitly-tracks-remaining-data-2026-09-11) | migration; core capture | 010b captures core records first and explicitly tracks remaining data |
| [D-062](DECISION_LOG.md#d-062--email-bulk-content-belongs-outside-postgresql-2026-09-11) | migration; email; storage | Email bulk content belongs outside PostgreSQL |
| [D-063](DECISION_LOG.md#d-063--revised-010b-specification-and-implementation-approved-2026-09-11) | migration; 010b approval | Revised 010b specification and implementation approved |
| [D-064](DECISION_LOG.md#d-064--010c-preserves-separate-people-approved-stages-and-review-only-use-2026-09-11) | migration; visibility; review hold | 010c preserves separate People, approved stages and review-only use |
| [D-065](DECISION_LOG.md#d-065--reviewed-010c-specification-and-implementation-approved-2026-09-11) | migration; 010c approval/release | Reviewed 010c specification and implementation approved |
| [D-066](DECISION_LOG.md#d-066--reviewed-010f1-specification-and-implementation-approved-2026-09-11) | migration; 010f1 approval/release | Reviewed 010f1 specification and implementation approved |
| [D-067](DECISION_LOG.md#d-067--notestasks-import-planning-readable-notes-and-explicit-date-only-timezone-2026-09-11) | migration; notes/tasks; timezone | Notes/tasks import planning: readable notes and explicit date-only timezone |
| [D-068](DECISION_LOG.md#d-068--reviewed-010f2-specification-and-implementation-approved-2026-09-11) | migration; 010f2 approval/release | Reviewed 010f2 specification and implementation approved |
| [D-069](DECISION_LOG.md#d-069--historical-migration-planning-split-2026-09-12) | migration; history sequence | Historical migration planning split |
| [D-070](DECISION_LOG.md#d-070--reviewed-010d1-specification-and-implementation-approved-2026-09-12) | migration; 010d1 approval/release | Reviewed 010d1 specification and implementation approved |
| [D-071](DECISION_LOG.md#d-071--010d2-planning-and-metadata-first-scope-2026-09-12) | migration; timeline scope | 010d2 planning and metadata-first scope |
| [D-072](DECISION_LOG.md#d-072--reviewed-010d2-contracts-and-implementation-approved-2026-09-12) | migration; 010d2 approval/release | Reviewed 010d2 contracts and implementation approved |
| [D-073](DECISION_LOG.md#d-073--offline-native-mobile-foundation-and-seven-day-access-2026-09-12) | mobile; offline access | Offline native mobile foundation and seven-day access |
| [D-074](DECISION_LOG.md#d-074--reviewed-mobile-001-implementation-and-parallel-work-approved-2026-09-12) | Mobile001 approval | Reviewed Mobile 001 implementation and parallel work approved |
| [D-075](DECISION_LOG.md#d-075--reviewed-010e1-and-coordinated-implementation-approved-2026-09-12) | migration 010e1; approval/release | Reviewed 010e1 and coordinated implementation approved |
| [D-076](DECISION_LOG.md#d-076--mobile-002-and-migration-010e2-implementation-approved-2026-09-12) | Mobile002; 010e2; approval/release | Mobile 002 and migration 010e2 implementation approved |
| [D-077](DECISION_LOG.md#d-077--mobile-003-and-migration-010e3-planning-authorized-2026-09-13) | Mobile003; 010e3; planning | Mobile 003 and migration 010e3 planning authorized |
| [D-078](DECISION_LOG.md#d-078--mobile-003-and-migration-010e3-implementation-approved-2026-09-13) | Mobile003; 010e3; approval/release | Mobile 003 and migration 010e3 implementation approved |
| [D-079](DECISION_LOG.md#d-079--mobile-004-and-sequential-admitted-people-migration-planning-2026-09-13) | Mobile004; 010e4; planning; family sequence | Mobile 004 and sequential admitted-People migration planning |
| [D-080](DECISION_LOG.md#d-080--mobile-004-and-admitted-people-refresh-implementation-approved-2026-09-13) | Mobile004; 010e4; implementation approval | Mobile 004 and admitted-People refresh implementation approved |
| [D-081](DECISION_LOG.md#d-081--mobile-005-and-admitted-people-metadata-planning-authorized-2026-09-13) | Mobile005; 010f3; metadata; parallel planning | Mobile 005 and admitted-People metadata planning authorized |
| [D-082](DECISION_LOG.md#d-082--mobile-005-and-admitted-people-metadata-implementation-approved-2026-09-14) | Mobile005; 010f3; implementation approval | Mobile 005 and admitted-People metadata implementation approved |
| [D-083](DECISION_LOG.md#d-083--mobile-006-and-admitted-people-activity-planning-authorized-2026-09-14) | Mobile006; 010f4; notes/tasks; planning | Mobile 006 and admitted-People activity planning authorized |
| [D-084](DECISION_LOG.md#d-084--mobile-006-and-admitted-people-activity-implementation-approved-2026-09-14) | Mobile006; 010f4; implementation approval/release | Mobile 006 and admitted-People activity implementation approved |
| [D-085](DECISION_LOG.md#d-085--mobile-007-discovery-and-admitted-people-history-planning-2026-09-15) | Mobile007; 010d3; discovery; history; planning | Mobile 007 discovery and admitted-People history planning |
| [D-086](DECISION_LOG.md#d-086--mobile-007-and-admitted-history-implementation-accepted-2026-09-15) | Mobile007; 010d3; implementation approval | Mobile 007 and admitted history implementation accepted |
| [D-087](DECISION_LOG.md#d-087--existing-people-stage-and-agent-mapping-repair-planning-2026-09-15) | migration; 010e5; mapping repair; planning | Existing-People stage and agent mapping repair planning |
| [D-088](DECISION_LOG.md#d-088--existing-people-mapping-repair-implementation-accepted-2026-09-15) | migration; 010e5; mapping repair; implementation/review | Existing-People mapping repair implementation accepted |

## Open-decision register (includes resolved entries)

| Decision | Applicability | Exact heading |
|---|---|---|
| [O-001](DECISION_LOG.md#o-001--resolved) | secrets; resolved | RESOLVED |
| [O-002](DECISION_LOG.md#o-002--call-recording-consent-policy-open) | recording; consent | Call recording consent policy (OPEN) |
| [O-003](DECISION_LOG.md#o-003--autonomous-ai-calling-open) | AI calling | Autonomous AI calling (OPEN) |
| [O-004](DECISION_LOG.md#o-004--conversation-and-data-ownership-on-agent-departure-open) | departure; ownership | Conversation and data ownership on agent departure (OPEN) |
| [O-005](DECISION_LOG.md#o-005--role-of-the-event-sourced-compliance-model-resolved-by-d-015) | history; resolved | Role of the event-sourced compliance model (RESOLVED by D-015) |
| [O-006](DECISION_LOG.md#o-006--outbound-messaging-consent-policy-open) | messaging; consent | Outbound messaging consent policy (OPEN) |
| [O-007](DECISION_LOG.md#o-007--slice-004-administration-design-defaults-resolved--confirmed-by-slice-004-d-026-d-027) | admin; resolved | Slice 004 administration design defaults (RESOLVED — confirmed by Slice 004, D-026, D-027) |
| [O-008](DECISION_LOG.md#o-008--ai-next-step-suggestions-after-each-communication-and-daily-open--intent-recorded-design-open) | AI; Today | AI next-step suggestions after each communication and daily (OPEN — intent recorded, design open) |
| [O-009](DECISION_LOG.md#o-009--organization-suspension-semantics-open) | suspension; auth | Organization suspension semantics (OPEN) |
| [O-010](DECISION_LOG.md#o-010--search-fuzzy-matching-and-a-search-layer-open--deferred-until-needs-are-known) | search | Search: fuzzy matching and a search layer (OPEN — deferred until needs are known) |
| [O-011](DECISION_LOG.md#o-011--outbound-calling-compliance-open) | calling; compliance | Outbound calling compliance (OPEN) |
| [O-012](DECISION_LOG.md#o-012--pii-content-blobs-per-person-keys-and-crypto-shred-open) | privacy; keys | PII content blobs: per-Person keys and crypto-shred (OPEN) |
| [O-013](DECISION_LOG.md#o-013--delete-my-data-person-erasure-suppression-and-who-fields-the-request-open--must-be-addressed-not-immediately) | erasure; privacy | "Delete my data": Person erasure, suppression, and who fields the request (OPEN — must be addressed, not immediately) |
| [O-014](DECISION_LOG.md#o-014--email-intake-first-then-capture-mailbox-access-models-open--expected-to-be-a-major-epic) | email; mailbox | Email: intake first, then capture; mailbox access models (OPEN — expected to be a major epic) |
| [O-015](DECISION_LOG.md#o-015--correspondencepayload-blob-size-storage-location-and-retention-open) | storage; retention | Correspondence/payload blob size, storage location, and retention (OPEN) |
