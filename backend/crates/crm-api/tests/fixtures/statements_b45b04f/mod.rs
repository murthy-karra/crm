//! Slice 012 (docs/specs/SLICE_012.md §4): the frozen pre-switch text of
//! the fourteen statements this slice will eventually switch to read the
//! new `last_*_at` columns. See `README.md` for the freeze rationale and
//! provenance. Used only by `tests/db_statement_equivalence.rs`.

pub mod person_sql;
pub mod system_feeds_sql;
pub mod today_sql;
