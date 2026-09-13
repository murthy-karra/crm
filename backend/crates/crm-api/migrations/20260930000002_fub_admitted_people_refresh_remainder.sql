-- D-080's exact-boundary remainder is intentional after a terminal attempt.
-- Monotonic confirmation remains enforced by the feature's locked source
-- interval check; a uniqueness constraint on the snapshot would make a
-- completed partial run unrecoverable.
DROP INDEX migration_admitted_people_refresh_boundary;
