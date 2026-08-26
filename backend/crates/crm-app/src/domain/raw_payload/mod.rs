//! `raw_payload` persistence (`store`), encryption (`crypto`), and the
//! two enums that replace its stringly-typed `resolution` and
//! `payload_format` columns at the Rust boundary (hardening chunk S1;
//! docs/design/type-safety-hardening.md).

pub mod crypto;
pub mod store;

/// `raw_payload.resolution` (docs/specs/SLICE_002.md §3; the `discarded`
/// variant added by docs/specs/SLICE_007e.md §3). `CHECK`-constrained at
/// the database (`raw_payload_resolution_check`,
/// migrations/20260830000001_raw_payload_discard.sql) — defense in
/// depth; this type is the compile-time enforcement, so a transposed
/// literal or a typo'd match arm is a compile error instead of a
/// runtime surprise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    Pending,
    Resolved,
    Unresolved,
    Discarded,
}

impl Resolution {
    pub fn as_str(self) -> &'static str {
        match self {
            Resolution::Pending => "pending",
            Resolution::Resolved => "resolved",
            Resolution::Unresolved => "unresolved",
            Resolution::Discarded => "discarded",
        }
    }

    /// Fails closed on an unrecognized value (mirrors
    /// `RoutingStrategy::from_str`'s posture): the `CHECK` constraint
    /// should make this unreachable in practice, but a read path must
    /// never crash the process on unexpected data.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Resolution::Pending),
            "resolved" => Some(Resolution::Resolved),
            "unresolved" => Some(Resolution::Unresolved),
            "discarded" => Some(Resolution::Discarded),
            _ => None,
        }
    }
}

/// `raw_payload.payload_format` (docs/specs/SLICE_002.md §3's
/// `generic_v1`; docs/specs/SLICE_007d.md §4 adds `rfc822_v1`). Not
/// `CHECK`-constrained at the database — `insert_pending`'s typed
/// parameter is this vocabulary's actual enforcement on the write side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadFormat {
    GenericV1,
    Rfc822V1,
}

impl PayloadFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            PayloadFormat::GenericV1 => "generic_v1",
            PayloadFormat::Rfc822V1 => "rfc822_v1",
        }
    }

    /// `None` on an unrecognized value. Deliberately fallible rather
    /// than paired with an infallible row-boundary decode: the
    /// workbench detail read stays fail-open for an unrecognized format
    /// (docs/specs/SLICE_007e.md §4's forward-compatible display
    /// default — lossy raw text rather than a 500 — so a future format
    /// added at the database before the display code knows about it
    /// still renders); retry deliberately fails closed on the same
    /// `None` instead.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "generic_v1" => Some(PayloadFormat::GenericV1),
            "rfc822_v1" => Some(PayloadFormat::Rfc822V1),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolution_round_trips_every_variant() {
        for resolution in [
            Resolution::Pending,
            Resolution::Resolved,
            Resolution::Unresolved,
            Resolution::Discarded,
        ] {
            assert_eq!(Resolution::parse(resolution.as_str()), Some(resolution));
        }
    }

    #[test]
    fn resolution_parse_fails_closed_on_an_unknown_value() {
        assert_eq!(Resolution::parse("bogus"), None);
    }

    #[test]
    fn payload_format_round_trips_every_variant() {
        for format in [PayloadFormat::GenericV1, PayloadFormat::Rfc822V1] {
            assert_eq!(PayloadFormat::parse(format.as_str()), Some(format));
        }
    }

    #[test]
    fn payload_format_parse_fails_closed_on_an_unknown_value() {
        assert_eq!(PayloadFormat::parse("bogus"), None);
    }
}
