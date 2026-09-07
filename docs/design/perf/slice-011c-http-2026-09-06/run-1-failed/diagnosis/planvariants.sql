\set QUIET on
\pset pager off
SET jit = off;
PREPARE builtin(uuid, uuid, timestamptz) AS WITH outcome_call AS (
               SELECT DISTINCT ON (c.person_id) c.person_id, c.id, c.ended_at
               FROM call c
               WHERE c.organization_id = $1
                 AND c.caller_user_id = $2
                 AND c.status IN ('ended', 'failed')
                 AND c.ended_at IS NOT NULL
                 AND EXISTS (
                     SELECT 1 FROM contact_attempted root
                     WHERE root.organization_id = $1
                       AND root.causation_id = c.id
                       AND root.corrects_id IS NULL
                       AND NOT EXISTS (SELECT 1 FROM contact_attempted x WHERE x.corrects_id = root.id)
                 )
               ORDER BY c.person_id, c.ended_at DESC, c.id DESC
           )
           SELECT
             p.id, p.first_name, p.last_name, p.created_at,
             s.id as stage_id, s.name as stage_name,
             u.id as "assigned_user_id?", u.display_name as "assigned_user_display_name?",
             (SELECT cm.value FROM contact_method cm
                WHERE cm.person_id = p.id AND cm.kind = 'email'
                ORDER BY cm.created_at ASC LIMIT 1) as "primary_email?",
             (SELECT cm.value FROM contact_method cm
                WHERE cm.person_id = p.id AND cm.kind = 'phone'
                ORDER BY cm.created_at ASC LIMIT 1) as "primary_phone?",
             (SELECT count(*) FROM inquiry i
                WHERE i.person_id = p.id AND i.organization_id = p.organization_id) as "inquiry_count!",
             latest.id as "latest_inquiry_id?",
             latest.source as "latest_inquiry_source?",
             latest.received_at as "latest_inquiry_received_at?",
             last_attempt.id as "last_attempt_id?",
             last_attempt.channel as "last_attempt_channel?",
             last_attempt.outcome as "last_attempt_outcome?",
             last_attempt.occurred_at as "last_attempt_occurred_at?",
             CASE WHEN reply_membership.by_reply THEN last_inbound.occurred_at
                  WHEN membership.by_inquiry THEN waiting.received_at
                  ELSE oc.ended_at END
                 as "waiting_since?",
             (CASE WHEN reply_membership.by_reply
                        THEN last_inbound.occurred_at > $3::timestamptz - interval '24 hours'
                   WHEN membership.by_inquiry
                        THEN latest.received_at > $3::timestamptz - interval '24 hours'
                   ELSE false END)
                 as "fresh?",
             membership.by_inquiry as "by_inquiry?",
             oc.id as "outcome_call_id?",
             oc.ended_at as "outcome_call_ended_at?",
             CASE WHEN reply_membership.by_reply THEN last_inbound.occurred_at ELSE NULL END
                 as "client_replied_occurred_at?"
           FROM person p
           JOIN stage s ON s.id = p.stage_id
           LEFT JOIN app_user u ON u.id = p.assigned_user_id
           LEFT JOIN LATERAL (
               SELECT i.id, i.source, i.received_at
               FROM inquiry i
               WHERE i.person_id = p.id AND i.organization_id = p.organization_id
               ORDER BY i.received_at DESC, i.id DESC
               LIMIT 1
           ) latest ON true
           LEFT JOIN LATERAL (
               SELECT ca.id, ca.channel, ca.outcome, ca.occurred_at
               FROM contact_attempted ca
               WHERE ca.person_id = p.id
                 AND ca.organization_id = p.organization_id
                 AND NOT EXISTS (SELECT 1 FROM contact_attempted c WHERE c.corrects_id = ca.id)
               ORDER BY ca.occurred_at DESC, ca.id DESC
               LIMIT 1
           ) last_attempt ON true
           LEFT JOIN LATERAL (
               SELECT i2.received_at
               FROM inquiry i2
               WHERE i2.person_id = p.id AND i2.organization_id = p.organization_id
                 AND i2.received_at > COALESCE(last_attempt.occurred_at, '-infinity'::timestamptz)
               ORDER BY i2.received_at ASC
               LIMIT 1
           ) waiting ON true
           LEFT JOIN outcome_call oc ON oc.person_id = p.id
           -- Slice 009 (docs/specs/SLICE_009.md §6): the latest inbound/
           -- outbound correspondence per Person, feeding the client_replied
           -- arm below. Every fact probe includes its organization predicate
           -- (`latest`/`waiting`/count, effective attempts, and both
           -- correspondence directions), while the outer
           -- `p.organization_id = $1` remains the Person tenant boundary.
           LEFT JOIN LATERAL (
               SELECT cc.occurred_at
               FROM correspondence_captured cc
               WHERE cc.person_id = p.id
                 AND cc.organization_id = p.organization_id
                 AND cc.direction = 'inbound'
               ORDER BY cc.occurred_at DESC, cc.id DESC
               LIMIT 1
           ) last_inbound ON true
           LEFT JOIN LATERAL (
               SELECT cc.occurred_at
               FROM correspondence_captured cc
               WHERE cc.person_id = p.id
                 AND cc.organization_id = p.organization_id
                 AND cc.direction = 'outbound'
               ORDER BY cc.occurred_at DESC, cc.id DESC
               LIMIT 1
           ) last_outbound ON true
           CROSS JOIN LATERAL (
               SELECT (COALESCE(p.assigned_user_id = $2, false) AND waiting.received_at IS NOT NULL) as by_inquiry
           ) membership
           -- client_replied (spec §6): assigned to the viewer, an inbound
           -- correspondence exists, and it is later than every effective
           -- contact_attempted AND every outbound correspondence — checking
           -- only the LATEST of each is equivalent to "later than every"
           -- (a later attempt/outbound would itself be the latest, so it
           -- alone is sufficient to disqualify).
           CROSS JOIN LATERAL (
               SELECT (
                   COALESCE(p.assigned_user_id = $2, false)
                   AND last_inbound.occurred_at IS NOT NULL
                   AND last_inbound.occurred_at > COALESCE(last_attempt.occurred_at, '-infinity'::timestamptz)
                   AND last_inbound.occurred_at > COALESCE(last_outbound.occurred_at, '-infinity'::timestamptz)
               ) as by_reply
           ) reply_membership
           WHERE p.organization_id = $1
             AND (p.assigned_user_id = $2 OR p.id IN (SELECT person_id FROM outcome_call))
             AND latest.id IS NOT NULL
             AND (membership.by_inquiry OR oc.id IS NOT NULL OR reply_membership.by_reply)
           ORDER BY (membership.by_inquiry OR reply_membership.by_reply) DESC,
                    "fresh?" DESC,
                    CASE WHEN reply_membership.by_reply THEN last_inbound.occurred_at
                         WHEN membership.by_inquiry THEN waiting.received_at
                         ELSE oc.ended_at END ASC,
                    p.id ASC
           LIMIT 201;
\echo === baseline ===
RESET enable_nestloop; RESET enable_incremental_sort; RESET enable_memoize; RESET enable_indexscan; RESET enable_indexonlyscan;

EXPLAIN (ANALYZE, BUFFERS, TIMING OFF) EXECUTE builtin('40ca6bb3-5e13-487f-859a-bef64207a330', '79b710c9-f440-4299-aa35-6982e466ade0', '2026-09-06T12:00:00Z');
\echo === nestloop_off ===
RESET enable_nestloop; RESET enable_incremental_sort; RESET enable_memoize; RESET enable_indexscan; RESET enable_indexonlyscan;
SET enable_nestloop = off;
EXPLAIN (ANALYZE, BUFFERS, TIMING OFF) EXECUTE builtin('40ca6bb3-5e13-487f-859a-bef64207a330', '79b710c9-f440-4299-aa35-6982e466ade0', '2026-09-06T12:00:00Z');
\echo === incsort_off ===
RESET enable_nestloop; RESET enable_incremental_sort; RESET enable_memoize; RESET enable_indexscan; RESET enable_indexonlyscan;
SET enable_incremental_sort = off;
EXPLAIN (ANALYZE, BUFFERS, TIMING OFF) EXECUTE builtin('40ca6bb3-5e13-487f-859a-bef64207a330', '79b710c9-f440-4299-aa35-6982e466ade0', '2026-09-06T12:00:00Z');
\echo === memoize_off ===
RESET enable_nestloop; RESET enable_incremental_sort; RESET enable_memoize; RESET enable_indexscan; RESET enable_indexonlyscan;
SET enable_memoize = off;
EXPLAIN (ANALYZE, BUFFERS, TIMING OFF) EXECUTE builtin('40ca6bb3-5e13-487f-859a-bef64207a330', '79b710c9-f440-4299-aa35-6982e466ade0', '2026-09-06T12:00:00Z');
\echo === indexscan_off ===
RESET enable_nestloop; RESET enable_incremental_sort; RESET enable_memoize; RESET enable_indexscan; RESET enable_indexonlyscan;
SET enable_indexscan = off; SET enable_indexonlyscan = off;
EXPLAIN (ANALYZE, BUFFERS, TIMING OFF) EXECUTE builtin('40ca6bb3-5e13-487f-859a-bef64207a330', '79b710c9-f440-4299-aa35-6982e466ade0', '2026-09-06T12:00:00Z');
