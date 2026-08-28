//! The direction/attribution ladder (docs/specs/SLICE_009.md §5) — pure
//! decision logic, gathering-free and DB-free by design (mirrors
//! `domain::today::rank`'s split from `domain::today::queries`): every
//! branch is exhaustively unit-testable here with synthetic inputs, and
//! `domain/capture/pipeline.rs` is the thin, untested-by-necessity DB
//! wrapper that gathers the four inputs and calls [`classify`].
//!
//! The spec's four numbered steps read, taken completely literally, as a
//! simple if/elif chain whose first two branches ("→ outbound", "→
//! inbound from that Person") don't individually explain HOW an outbound
//! row's Person is found. Read together with step 4's held-queue rule —
//! "counterparty = inner-or-outer From for presumed-inbound, first
//! non-member recipient OTHERWISE" — the intended shape is:
//!
//! 1. **From is an active member's login** (we KNOW this is outbound,
//!    strong evidence) → the Person(s) still come from matching
//!    recipients (the same mechanism step 3 uses) — because "outbound"
//!    means "sent TO the client", and the client is always identified via
//!    recipients, never the (known-to-be-ours) sender. No recipient
//!    match → held, with the OUTBOUND presumption (step 4's "otherwise").
//! 2. **Else** From matches a Person (weaker evidence than an exact member
//!    login, but still a direct hit) → inbound from that Person. Takes
//!    priority over step 3 so an unrecognized sender can never eclipse a
//!    real match.
//! 3. **Else** any recipient matches a Person (weakest evidence — the
//!    "mailing-list/assistant edge": an unrecognized sender routing
//!    through a known recipient is still classified outbound, spec-stated
//!    as semantically imperfect but structurally consistent) → outbound,
//!    one row per matched recipient Person.
//! 4. **Else** unmatched → held, with the INBOUND presumption (the
//!    default — most held mail in practice is an unmatched reply or a new
//!    inquiry-like sender), From as the counterparty when present, else
//!    the first non-member recipient.

use crate::ids::PersonId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Inbound,
    Outbound,
}

impl Direction {
    pub fn as_str(self) -> &'static str {
        match self {
            Direction::Inbound => "inbound",
            Direction::Outbound => "outbound",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LadderOutcome {
    /// Steps 1 or 3 (outbound) or step 2 (inbound): the fact row(s) to
    /// create. `persons` is never empty when this variant is constructed
    /// (see `classify`) — one row per entry (spec §5's "one row per
    /// matched recipient Person"; a single-entry `Vec` for the step-2
    /// inbound case).
    Matched {
        direction: Direction,
        persons: Vec<PersonId>,
    },
    /// Step 4: nobody matched. `direction_hint` and `counterparty` are
    /// exactly `capture_message`'s own columns (spec §8: the link
    /// endpoint reads `direction_hint` directly rather than re-deriving
    /// it). `counterparty` is `None` only in the pathological case where
    /// NEITHER a From address NOR any non-member recipient exists
    /// (accepted, stated — `capture_message.counterparty_email` is
    /// nullable for exactly this edge, independent of the terminal-state
    /// nulling D-015 §4 rule).
    Held {
        direction_hint: Direction,
        counterparty: Option<String>,
    },
}

/// The pure ladder (docs/specs/SLICE_009.md §5). All four inputs are
/// pre-gathered by the caller (`pipeline.rs`) under `PersonVisibilityScope
/// ::Organization` — this function trusts them as given and does no I/O.
///
/// - `from_addr`: the working view's (possibly forward-unwrapped) From
///   address, lowercased.
/// - `from_is_active_member`: step 1's condition — does `from_addr` equal
///   an ACTIVE member's login email in this Organization.
/// - `from_matched_person`: step 2's result (org-scoped normalize+match,
///   match-never-create) — meaningful only when `!from_is_active_member`
///   (step 1 takes priority regardless of this value; see the module doc
///   for why even a coincidental match here must not override step 1).
/// - `recipient_matched_persons`: step 3's result — the DISTINCT Persons
///   matched among To/Cc (minus the capture address itself, capped),
///   in the caller's discovery order. Reused by step 1's fallback.
/// - `first_non_member_recipient`: the first To/Cc address (capped set)
///   that is NOT an active member's login — step 4's outbound-presumed
///   counterparty candidate, used both when step 1 fires with no
///   recipient match and when step 4 itself is reached with no usable
///   From.
pub fn classify(
    from_addr: Option<&str>,
    from_is_active_member: bool,
    from_matched_person: Option<PersonId>,
    recipient_matched_persons: &[PersonId],
    first_non_member_recipient: Option<&str>,
) -> LadderOutcome {
    if from_is_active_member {
        if !recipient_matched_persons.is_empty() {
            return LadderOutcome::Matched {
                direction: Direction::Outbound,
                persons: recipient_matched_persons.to_vec(),
            };
        }
        return LadderOutcome::Held {
            direction_hint: Direction::Outbound,
            counterparty: first_non_member_recipient.map(str::to_string),
        };
    }

    if let Some(person_id) = from_matched_person {
        return LadderOutcome::Matched {
            direction: Direction::Inbound,
            persons: vec![person_id],
        };
    }

    if !recipient_matched_persons.is_empty() {
        return LadderOutcome::Matched {
            direction: Direction::Outbound,
            persons: recipient_matched_persons.to_vec(),
        };
    }

    LadderOutcome::Held {
        direction_hint: Direction::Inbound,
        counterparty: from_addr
            .map(str::to_string)
            .or_else(|| first_non_member_recipient.map(str::to_string)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn pid(byte: u8) -> PersonId {
        PersonId::new(Uuid::from_bytes([byte; 16]))
    }

    // --- Step 1: From is an active member ------------------------------

    #[test]
    fn step1_member_from_with_one_matched_recipient_is_outbound() {
        let p1 = pid(1);
        let outcome = classify(Some("agent@ourfirm.com"), true, None, &[p1], None);
        assert_eq!(
            outcome,
            LadderOutcome::Matched {
                direction: Direction::Outbound,
                persons: vec![p1]
            }
        );
    }

    #[test]
    fn step1_member_from_with_multiple_matched_recipients_creates_one_row_each() {
        let p1 = pid(1);
        let p2 = pid(2);
        let outcome = classify(Some("agent@ourfirm.com"), true, None, &[p1, p2], None);
        assert_eq!(
            outcome,
            LadderOutcome::Matched {
                direction: Direction::Outbound,
                persons: vec![p1, p2]
            }
        );
    }

    #[test]
    fn step1_member_from_with_no_recipient_match_is_held_outbound_presumed() {
        let outcome = classify(
            Some("agent@ourfirm.com"),
            true,
            None,
            &[],
            Some("stranger@example.com"),
        );
        assert_eq!(
            outcome,
            LadderOutcome::Held {
                direction_hint: Direction::Outbound,
                counterparty: Some("stranger@example.com".to_string()),
            }
        );
    }

    #[test]
    fn step1_member_from_with_no_recipient_match_and_no_candidate_is_held_with_no_counterparty() {
        let outcome = classify(Some("agent@ourfirm.com"), true, None, &[], None);
        assert_eq!(
            outcome,
            LadderOutcome::Held {
                direction_hint: Direction::Outbound,
                counterparty: None,
            }
        );
    }

    #[test]
    fn step1_takes_priority_over_step2_even_when_from_also_matches_a_person() {
        // A coincidental Person-contact-method match on the agent's own
        // login email must NOT flip this to inbound — step 1 short-
        // circuits before step 2's evidence is even consulted.
        let member_person = pid(9);
        let recipient_person = pid(1);
        let outcome = classify(
            Some("agent@ourfirm.com"),
            true,
            Some(member_person),
            &[recipient_person],
            None,
        );
        assert_eq!(
            outcome,
            LadderOutcome::Matched {
                direction: Direction::Outbound,
                persons: vec![recipient_person],
            }
        );
    }

    // --- Step 2: From matches a Person ----------------------------------

    #[test]
    fn step2_from_matches_a_person_is_inbound_regardless_of_recipient_matches() {
        let from_person = pid(1);
        let recipient_person = pid(2);
        // Step 2 wins over step 3 even though a recipient ALSO matched.
        let outcome = classify(
            Some("client@example.com"),
            false,
            Some(from_person),
            &[recipient_person],
            None,
        );
        assert_eq!(
            outcome,
            LadderOutcome::Matched {
                direction: Direction::Inbound,
                persons: vec![from_person],
            }
        );
    }

    // --- Step 3: recipient matches (the "mailing-list/assistant edge") -

    #[test]
    fn step3_unmatched_from_with_matched_recipients_is_outbound() {
        let p1 = pid(1);
        let outcome = classify(Some("noreply@mailinglist.com"), false, None, &[p1], None);
        assert_eq!(
            outcome,
            LadderOutcome::Matched {
                direction: Direction::Outbound,
                persons: vec![p1],
            }
        );
    }

    #[test]
    fn step3_fires_even_with_no_from_address_at_all() {
        let p1 = pid(1);
        let outcome = classify(None, false, None, &[p1], None);
        assert_eq!(
            outcome,
            LadderOutcome::Matched {
                direction: Direction::Outbound,
                persons: vec![p1],
            }
        );
    }

    // --- Step 4: fully unmatched, held queue ----------------------------

    #[test]
    fn step4_unmatched_from_present_is_held_inbound_presumed_using_from_as_counterparty() {
        let outcome = classify(
            Some("stranger@example.com"),
            false,
            None,
            &[],
            Some("also-not-us@example.com"),
        );
        // From, when present, wins over the recipient fallback even
        // though one was supplied — "presumed-inbound" always prefers
        // From (spec §5 step 4).
        assert_eq!(
            outcome,
            LadderOutcome::Held {
                direction_hint: Direction::Inbound,
                counterparty: Some("stranger@example.com".to_string()),
            }
        );
    }

    #[test]
    fn step4_no_from_falls_back_to_the_first_non_member_recipient() {
        let outcome = classify(None, false, None, &[], Some("candidate@example.com"));
        assert_eq!(
            outcome,
            LadderOutcome::Held {
                direction_hint: Direction::Inbound,
                counterparty: Some("candidate@example.com".to_string()),
            }
        );
    }

    #[test]
    fn step4_no_from_and_no_recipient_candidate_is_held_with_no_counterparty() {
        let outcome = classify(None, false, None, &[], None);
        assert_eq!(
            outcome,
            LadderOutcome::Held {
                direction_hint: Direction::Inbound,
                counterparty: None,
            }
        );
    }

    // --- Direction round trip -------------------------------------------

    #[test]
    fn direction_as_str_matches_the_column_vocabulary() {
        assert_eq!(Direction::Inbound.as_str(), "inbound");
        assert_eq!(Direction::Outbound.as_str(), "outbound");
    }
}
