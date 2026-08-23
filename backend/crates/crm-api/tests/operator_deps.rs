//! The D-028 §5 fence (docs/specs/SLICE_005.md §3): `crm-operator` must
//! never depend on `sqlx`, `axum`, or `crm-api`. Cheap, and it keeps the
//! inverted dependency visible in CI.

#[test]
fn crm_operator_has_no_sqlx_axum_or_crm_api_dependency() {
    let manifest = include_str!("../../crm-operator/Cargo.toml");
    // Only the dependency sections matter; the crate-level comment names
    // the forbidden crates on purpose.
    let deps: String = manifest
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    let deps_section = deps
        .split("[dependencies]")
        .nth(1)
        .expect("crm-operator has a [dependencies] section");
    for forbidden in ["sqlx", "axum", "crm-api", "crm_api"] {
        let direct = deps_section
            .lines()
            .any(|line| line.trim_start().starts_with(forbidden));
        let renamed = deps_section.contains(&format!("package = \"{forbidden}\""));
        let table = deps_section.contains(&format!("dependencies.{forbidden}"));
        assert!(
            !direct && !renamed && !table,
            "crm-operator must not depend on {forbidden} (D-028 §5)"
        );
    }
}

/// The 006a fence (docs/specs/SLICE_006a.md §3): `crm-app` must never
/// depend on `axum`, `crm-operator`, or `crm-api` in `[dependencies]`.
/// `[dev-dependencies]` may name axum (the fake LiveKit server test), so
/// unlike the check above this one stops at the next section header.
#[test]
fn crm_app_has_no_axum_crm_operator_or_crm_api_dependency() {
    let manifest = include_str!("../../crm-app/Cargo.toml");
    let deps: String = manifest
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    let after = deps
        .split("[dependencies]")
        .nth(1)
        .expect("crm-app has a [dependencies] section");
    // `[dependencies]` only: cut at the next `[section]` line.
    let deps_section = match after.lines().position(|l| l.trim_start().starts_with('[')) {
        Some(idx) => after
            .lines()
            .take(idx)
            .collect::<Vec<_>>()
            .join("\n"),
        None => after.to_string(),
    };
    for forbidden in ["axum", "crm-operator", "crm_operator", "crm-api", "crm_api"] {
        let direct = deps_section
            .lines()
            .any(|line| line.trim_start().starts_with(forbidden));
        let renamed = deps_section.contains(&format!("package = \"{forbidden}\""));
        // exact `[dependencies.X]` so `[dev-dependencies.axum]` cannot match
        let table = after.contains(&format!("[dependencies.{forbidden}]"));
        assert!(
            !direct && !renamed && !table,
            "crm-app must not depend on {forbidden} (docs/specs/SLICE_006a.md §3)"
        );
    }
}
