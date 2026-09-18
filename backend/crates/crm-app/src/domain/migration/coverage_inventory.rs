//! Static, source-qualified migration coverage boundaries.
//!
//! This is deliberately not an account inventory or a reconciliation result.
//! Dynamic readers must keep captured counts, holds, result heads and source
//! completeness separate from these fixed product boundaries.

use serde::Serialize;

/// A stable migration coverage surface. These are report vocabulary, not source
/// endpoint names or a promise that every account contains the family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageFamily {
    PeopleContacts,
    Stages,
    SourceUsers,
    EmbeddedPersonTags,
    StandaloneTagCatalog,
    CustomFields,
    Notes,
    Tasks,
    HistoricalEvents,
    Calls,
    Texts,
    Addresses,
    Relationships,
    Appointments,
    Deals,
    Emails,
    Recordings,
    ExternalFiles,
    AutomationSettings,
    SourceDeletionSemantics,
    WorkspaceActivation,
    PrivacyErasure,
}

/// The best current product path for a family.
///
/// `Implemented` means a typed retained-evidence import/refresh path exists;
/// it never means a source account is completely captured, qualified or ready
/// to activate. `MetadataOnly` similarly never permits content/body claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoveragePath {
    Implemented,
    MetadataOnly,
    PreservedUnrepresented,
    UnqualifiedSource,
    NotCaptured,
    Unknown,
    DeferredPrerequisite,
}

/// The static source-evidence boundary that supports the descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceEvidence {
    RetainedCoreProfile,
    RetainedHistoryProfile,
    EmbeddedPersonRecord,
    UnqualifiedCatalog,
    NotCaptured,
    Unknown,
    DeferredPolicy,
}

/// The current destination/read-model surface; this is distinct from whether a
/// particular retained item was applied, held or excluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DestinationSurface {
    TypedImport,
    MetadataOnly,
    PreservedUnrepresented,
    Unavailable,
    Deferred,
}

/// An immutable descriptor consumed by migration review/reconciliation readers.
/// `blocker_codes` are stable, safe report vocabulary; they contain no source
/// content, credentials or account-specific state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CoverageDescriptor {
    pub family: CoverageFamily,
    pub path: CoveragePath,
    pub source_evidence: SourceEvidence,
    pub destination_surface: DestinationSurface,
    pub blocker_codes: &'static [&'static str],
}

const INVENTORY: &[CoverageDescriptor] = &[
    CoverageDescriptor {
        family: CoverageFamily::PeopleContacts,
        path: CoveragePath::Implemented,
        source_evidence: SourceEvidence::RetainedCoreProfile,
        destination_surface: DestinationSurface::TypedImport,
        blocker_codes: &[
            "source_scope_not_live_qualified",
            "source_deletion_semantics_unknown",
            "migration_review_only",
        ],
    },
    CoverageDescriptor {
        family: CoverageFamily::Stages,
        path: CoveragePath::Implemented,
        source_evidence: SourceEvidence::RetainedCoreProfile,
        destination_surface: DestinationSurface::TypedImport,
        blocker_codes: &["source_scope_not_live_qualified", "migration_review_only"],
    },
    CoverageDescriptor {
        family: CoverageFamily::SourceUsers,
        path: CoveragePath::Implemented,
        source_evidence: SourceEvidence::RetainedCoreProfile,
        destination_surface: DestinationSurface::TypedImport,
        blocker_codes: &[
            "source_users_do_not_create_identities",
            "source_scope_not_live_qualified",
            "migration_review_only",
        ],
    },
    CoverageDescriptor {
        family: CoverageFamily::EmbeddedPersonTags,
        path: CoveragePath::Implemented,
        source_evidence: SourceEvidence::EmbeddedPersonRecord,
        destination_surface: DestinationSurface::TypedImport,
        blocker_codes: &[
            "embedded_person_tags_only",
            "source_scope_not_live_qualified",
            "migration_review_only",
        ],
    },
    CoverageDescriptor {
        family: CoverageFamily::StandaloneTagCatalog,
        path: CoveragePath::UnqualifiedSource,
        source_evidence: SourceEvidence::UnqualifiedCatalog,
        destination_surface: DestinationSurface::Unavailable,
        blocker_codes: &["standalone_tag_catalog_unqualified"],
    },
    CoverageDescriptor {
        family: CoverageFamily::CustomFields,
        path: CoveragePath::Implemented,
        source_evidence: SourceEvidence::RetainedCoreProfile,
        destination_surface: DestinationSurface::TypedImport,
        blocker_codes: &[
            "source_scope_not_live_qualified",
            "unsupported_values_or_mapping_holds",
            "migration_review_only",
        ],
    },
    CoverageDescriptor {
        family: CoverageFamily::Notes,
        path: CoveragePath::Implemented,
        source_evidence: SourceEvidence::RetainedCoreProfile,
        destination_surface: DestinationSurface::TypedImport,
        blocker_codes: &[
            "restricted_note_detail_possible",
            "unsupported_replies_or_attachments_possible",
            "migration_review_only",
        ],
    },
    CoverageDescriptor {
        family: CoverageFamily::Tasks,
        path: CoveragePath::Implemented,
        source_evidence: SourceEvidence::RetainedCoreProfile,
        destination_surface: DestinationSurface::TypedImport,
        blocker_codes: &[
            "unsupported_task_semantics_possible",
            "migration_review_only",
        ],
    },
    CoverageDescriptor {
        family: CoverageFamily::HistoricalEvents,
        path: CoveragePath::MetadataOnly,
        source_evidence: SourceEvidence::RetainedHistoryProfile,
        destination_surface: DestinationSurface::MetadataOnly,
        blocker_codes: &[
            "external_facts_only",
            "history_content_not_imported",
            "api_restricted_records_unknown",
            "migration_review_only",
        ],
    },
    CoverageDescriptor {
        family: CoverageFamily::Calls,
        path: CoveragePath::MetadataOnly,
        source_evidence: SourceEvidence::RetainedHistoryProfile,
        destination_surface: DestinationSurface::MetadataOnly,
        blocker_codes: &[
            "call_api_restricted_records_unknown",
            "call_content_and_media_not_imported",
            "migration_review_only",
        ],
    },
    CoverageDescriptor {
        family: CoverageFamily::Texts,
        path: CoveragePath::MetadataOnly,
        source_evidence: SourceEvidence::RetainedHistoryProfile,
        destination_surface: DestinationSurface::MetadataOnly,
        blocker_codes: &[
            "text_api_restricted_records_unknown",
            "text_content_not_imported",
            "migration_review_only",
        ],
    },
    CoverageDescriptor {
        family: CoverageFamily::Addresses,
        path: CoveragePath::PreservedUnrepresented,
        source_evidence: SourceEvidence::EmbeddedPersonRecord,
        destination_surface: DestinationSurface::PreservedUnrepresented,
        blocker_codes: &["destination_model_missing", "retained_only_when_returned"],
    },
    CoverageDescriptor {
        family: CoverageFamily::Relationships,
        path: CoveragePath::PreservedUnrepresented,
        source_evidence: SourceEvidence::EmbeddedPersonRecord,
        destination_surface: DestinationSurface::PreservedUnrepresented,
        blocker_codes: &["destination_model_missing", "retained_only_when_returned"],
    },
    CoverageDescriptor {
        family: CoverageFamily::Appointments,
        path: CoveragePath::NotCaptured,
        source_evidence: SourceEvidence::NotCaptured,
        destination_surface: DestinationSurface::Unavailable,
        blocker_codes: &["source_profile_not_captured", "destination_model_missing"],
    },
    CoverageDescriptor {
        family: CoverageFamily::Deals,
        path: CoveragePath::NotCaptured,
        source_evidence: SourceEvidence::NotCaptured,
        destination_surface: DestinationSurface::Unavailable,
        blocker_codes: &["source_profile_not_captured", "destination_model_missing"],
    },
    CoverageDescriptor {
        family: CoverageFamily::Emails,
        path: CoveragePath::NotCaptured,
        source_evidence: SourceEvidence::NotCaptured,
        destination_surface: DestinationSurface::Unavailable,
        blocker_codes: &[
            "source_profile_not_captured",
            "email_bulk_storage_unresolved",
        ],
    },
    CoverageDescriptor {
        family: CoverageFamily::Recordings,
        path: CoveragePath::NotCaptured,
        source_evidence: SourceEvidence::NotCaptured,
        destination_surface: DestinationSurface::Unavailable,
        blocker_codes: &["source_profile_not_captured", "recording_policy_unresolved"],
    },
    CoverageDescriptor {
        family: CoverageFamily::ExternalFiles,
        path: CoveragePath::NotCaptured,
        source_evidence: SourceEvidence::NotCaptured,
        destination_surface: DestinationSurface::Unavailable,
        blocker_codes: &["source_profile_not_captured", "destination_model_missing"],
    },
    CoverageDescriptor {
        family: CoverageFamily::AutomationSettings,
        path: CoveragePath::NotCaptured,
        source_evidence: SourceEvidence::NotCaptured,
        destination_surface: DestinationSurface::Unavailable,
        blocker_codes: &["source_profile_not_captured", "destination_model_missing"],
    },
    CoverageDescriptor {
        family: CoverageFamily::SourceDeletionSemantics,
        path: CoveragePath::Unknown,
        source_evidence: SourceEvidence::Unknown,
        destination_surface: DestinationSurface::Unavailable,
        blocker_codes: &["source_deletion_semantics_unknown"],
    },
    CoverageDescriptor {
        family: CoverageFamily::WorkspaceActivation,
        path: CoveragePath::DeferredPrerequisite,
        source_evidence: SourceEvidence::DeferredPolicy,
        destination_surface: DestinationSurface::Deferred,
        blocker_codes: &["activation_not_implemented", "migration_review_only"],
    },
    CoverageDescriptor {
        family: CoverageFamily::PrivacyErasure,
        path: CoveragePath::DeferredPrerequisite,
        source_evidence: SourceEvidence::DeferredPolicy,
        destination_surface: DestinationSurface::Deferred,
        blocker_codes: &["o_012_open", "o_013_open"],
    },
];

/// Returns the complete, fixed taxonomy in stable report order.
pub fn inventory() -> &'static [CoverageDescriptor] {
    INVENTORY
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor(family: CoverageFamily) -> CoverageDescriptor {
        *inventory()
            .iter()
            .find(|descriptor| descriptor.family == family)
            .expect("every asserted coverage family is inventoried")
    }

    #[test]
    fn inventory_is_unique_and_has_no_false_ready_surface() {
        let mut families = inventory()
            .iter()
            .map(|descriptor| descriptor.family as u8)
            .collect::<Vec<_>>();
        families.sort_unstable();
        families.dedup();
        assert_eq!(families.len(), inventory().len());
        assert!(inventory().iter().all(|descriptor| {
            descriptor.path != CoveragePath::Implemented
                || descriptor
                    .blocker_codes
                    .iter()
                    .any(|code| *code == "migration_review_only")
        }));
    }

    #[test]
    fn embedded_tags_and_standalone_catalog_are_not_conflated() {
        let embedded = descriptor(CoverageFamily::EmbeddedPersonTags);
        assert_eq!(embedded.path, CoveragePath::Implemented);
        assert_eq!(
            embedded.source_evidence,
            SourceEvidence::EmbeddedPersonRecord
        );
        assert!(embedded
            .blocker_codes
            .contains(&"embedded_person_tags_only"));

        let catalog = descriptor(CoverageFamily::StandaloneTagCatalog);
        assert_eq!(catalog.path, CoveragePath::UnqualifiedSource);
        assert_eq!(catalog.source_evidence, SourceEvidence::UnqualifiedCatalog);
        assert_eq!(catalog.destination_surface, DestinationSurface::Unavailable);
        assert!(catalog
            .blocker_codes
            .contains(&"standalone_tag_catalog_unqualified"));
    }

    #[test]
    fn unsupported_families_remain_explicit_and_history_stays_metadata_only() {
        for family in [
            CoverageFamily::Appointments,
            CoverageFamily::Deals,
            CoverageFamily::Emails,
            CoverageFamily::Recordings,
            CoverageFamily::ExternalFiles,
            CoverageFamily::AutomationSettings,
        ] {
            assert_eq!(descriptor(family).path, CoveragePath::NotCaptured);
        }
        for family in [
            CoverageFamily::HistoricalEvents,
            CoverageFamily::Calls,
            CoverageFamily::Texts,
        ] {
            let history = descriptor(family);
            assert_eq!(history.path, CoveragePath::MetadataOnly);
            assert_eq!(
                history.destination_surface,
                DestinationSurface::MetadataOnly
            );
        }
        assert_eq!(
            descriptor(CoverageFamily::SourceDeletionSemantics).path,
            CoveragePath::Unknown
        );
        assert_eq!(
            descriptor(CoverageFamily::WorkspaceActivation).path,
            CoveragePath::DeferredPrerequisite
        );
    }

    #[test]
    fn wire_vocabulary_is_stable_and_safe() {
        let catalog = serde_json::to_value(descriptor(CoverageFamily::StandaloneTagCatalog))
            .expect("closed descriptor serializes");
        assert_eq!(catalog["family"], "standalone_tag_catalog");
        assert_eq!(catalog["path"], "unqualified_source");
        assert_eq!(catalog["source_evidence"], "unqualified_catalog");
        assert_eq!(catalog["destination_surface"], "unavailable");
        assert_eq!(
            catalog["blocker_codes"],
            serde_json::json!(["standalone_tag_catalog_unqualified"])
        );
    }
}
