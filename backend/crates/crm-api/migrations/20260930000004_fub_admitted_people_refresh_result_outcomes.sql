-- A closed preview outcome is still a durable execution result.  The worker
-- settles it without a native write so a mixed eligible/held cohort can reach
-- its terminal state; preserve every declared non-mutating outcome here.
ALTER TABLE migration_admitted_people_refresh_result
  DROP CONSTRAINT migration_admitted_people_refresh_result_disposition_check;

ALTER TABLE migration_admitted_people_refresh_result
  ADD CONSTRAINT migration_admitted_people_refresh_result_disposition_check
  CHECK (disposition IN (
    'settled', 'settled_noop', 'held_stale', 'held_local_change',
    'held_evidence_gap', 'held_mapping_gap', 'held_target_missing',
    'held_original_hold', 'excluded_source_only', 'not_seen_again', 'cancelled'
  ));
