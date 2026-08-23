-- Slice 006c §2: call outcome correction. Widens the `outcome` vocabulary
-- with the two agent statements that must not vanish into `no_answer`
-- (`busy`, `wrong_number`), and makes `corrects_id` chains linear: a row
-- is corrected at most once, so the effective attempt is the one with no
-- corrector. The partial unique index also serves the `NOT EXISTS`
-- corrector lookups (Today's effective row, the head-attempt lookup).
-- Append-only triggers and grants are untouched (D-015).

ALTER TABLE contact_attempted DROP CONSTRAINT contact_attempted_outcome_check;
ALTER TABLE contact_attempted ADD CONSTRAINT contact_attempted_outcome_check
    CHECK (outcome IN ('reached','no_answer','left_message','sent','busy','wrong_number'));
-- Linear correction chains: a row is corrected at most once.
CREATE UNIQUE INDEX contact_attempted_corrects_once
    ON contact_attempted (corrects_id) WHERE corrects_id IS NOT NULL;
