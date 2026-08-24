//! The Organization intake address (docs/specs/SLICE_007a.md §4): stored
//! as `(slug, token)`, rendered by configuration so the scheme can flip
//! (O-014 decision 1, blocking only at the DNS rung) without touching a
//! row. `parse_recipient` accepts BOTH forms regardless of the configured
//! scheme for the same reason. Neither function logs its input.

use crate::config::{IntakeAddressScheme, IntakeMailConfig};

/// The local-part prefix of the subdomain form and the subdomain label of
/// the local-part form.
const LEADS: &str = "leads";
/// Tokens match the DB CHECK alphabet (`[a-z0-9]{8}`), which is wider
/// than the mint alphabet (`[a-z2-7]`) so backfilled hex tokens resolve.
const TOKEN_LEN: usize = 8;
const SLUG_MAX_LEN: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntakeAddress {
    pub slug: String,
    pub token: String,
}

impl IntakeAddress {
    /// Subdomain: `leads-<token>@<slug>.<domain>`;
    /// LocalPart: `<slug>-<token>@leads.<domain>`.
    pub fn render(&self, cfg: &IntakeMailConfig) -> String {
        match cfg.scheme {
            IntakeAddressScheme::Subdomain => {
                format!("{LEADS}-{}@{}.{}", self.token, self.slug, cfg.domain)
            }
            IntakeAddressScheme::LocalPart => {
                format!("{}-{}@{LEADS}.{}", self.slug, self.token, cfg.domain)
            }
        }
    }

    /// Case-insensitive; a bare address only (no display name, no `+tag`,
    /// no extra labels, exact configured domain). `None` on anything else.
    pub fn parse_recipient(addr: &str, cfg: &IntakeMailConfig) -> Option<IntakeAddress> {
        let addr = addr.trim().to_ascii_lowercase();
        let (local, host) = addr.split_once('@')?;
        if local.is_empty() || host.is_empty() {
            return None;
        }
        let domain = cfg.domain.to_ascii_lowercase();
        let sub = host.strip_suffix(&domain)?.strip_suffix('.')?;
        if sub.is_empty() || sub.contains('.') {
            return None;
        }
        // Subdomain form: local = leads-<token>, sub = <slug>.
        if let Some(token) = local.strip_prefix(&format!("{LEADS}-")) {
            if is_token(token) && is_slug(sub) {
                return Some(IntakeAddress {
                    slug: sub.to_string(),
                    token: token.to_string(),
                });
            }
        }
        // Local-part form: local = <slug>-<token>, sub = leads.
        if sub == LEADS {
            let (slug, token) = local.rsplit_once('-')?;
            if is_token(token) && is_slug(slug) {
                return Some(IntakeAddress {
                    slug: slug.to_string(),
                    token: token.to_string(),
                });
            }
        }
        None
    }
}

fn is_token(s: &str) -> bool {
    s.len() == TOKEN_LEN
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

/// The DB CHECK: `^[a-z0-9]([a-z0-9-]{0,38}[a-z0-9])?$`.
pub fn is_slug(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() || bytes.len() > SLUG_MAX_LEN {
        return false;
    }
    let ok = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    ok(bytes[0]) && ok(bytes[bytes.len() - 1]) && bytes.iter().all(|&b| ok(b) || b == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(scheme: IntakeAddressScheme) -> IntakeMailConfig {
        IntakeMailConfig {
            domain: "elysianfeld.com".to_string(),
            scheme,
        }
    }

    fn addr() -> IntakeAddress {
        IntakeAddress {
            slug: "cypress-bay-realty".into(),
            token: "k7f3q2wd".into(),
        }
    }

    #[test]
    fn renders_both_schemes() {
        assert_eq!(
            addr().render(&cfg(IntakeAddressScheme::Subdomain)),
            "leads-k7f3q2wd@cypress-bay-realty.elysianfeld.com"
        );
        assert_eq!(
            addr().render(&cfg(IntakeAddressScheme::LocalPart)),
            "cypress-bay-realty-k7f3q2wd@leads.elysianfeld.com"
        );
    }

    #[test]
    fn parses_both_forms_regardless_of_configured_scheme() {
        for scheme in [
            IntakeAddressScheme::Subdomain,
            IntakeAddressScheme::LocalPart,
        ] {
            let c = cfg(scheme);
            assert_eq!(
                IntakeAddress::parse_recipient(
                    "leads-k7f3q2wd@cypress-bay-realty.elysianfeld.com",
                    &c
                ),
                Some(addr())
            );
            assert_eq!(
                IntakeAddress::parse_recipient(
                    "cypress-bay-realty-k7f3q2wd@leads.elysianfeld.com",
                    &c
                ),
                Some(addr())
            );
        }
    }

    #[test]
    fn accepts_uppercase_whitespace_and_hex_tokens() {
        let c = cfg(IntakeAddressScheme::Subdomain);
        assert_eq!(
            IntakeAddress::parse_recipient(
                "  LEADS-K7F3Q2WD@Cypress-Bay-Realty.ElysianFeld.com ",
                &c
            ),
            Some(addr())
        );
        // Backfilled orgs carry md5-hex tokens: in the CHECK alphabet.
        assert!(
            IntakeAddress::parse_recipient("leads-9f86d081@org-12345678.elysianfeld.com", &c)
                .is_some()
        );
    }

    #[test]
    fn rejects_wrong_domain_extra_labels_tags_and_malformed_parts() {
        let c = cfg(IntakeAddressScheme::Subdomain);
        for bad in [
            "leads-k7f3q2wd@evil.com",
            "leads-k7f3q2wd@cypress-bay-realty.elysianfeld.com.evil.com",
            "leads-k7f3q2wd@a.cypress-bay-realty.elysianfeld.com",
            "leads-k7f3q2wd@elysianfeld.com",
            "leads-k7f3q2wd+tag@cypress-bay-realty.elysianfeld.com",
            "leads-k7f3q2w@cypress-bay-realty.elysianfeld.com",
            "leads-k7f3q2wdx@cypress-bay-realty.elysianfeld.com",
            "leads-K7F3-2WD@cypress-bay-realty.elysianfeld.com",
            "Grace <leads-k7f3q2wd@cypress-bay-realty.elysianfeld.com>",
            "cypress-bay-realty-k7f3q2wd@leads.evil.com",
            "-bad-k7f3q2wd@leads.elysianfeld.com",
            "@cypress-bay-realty.elysianfeld.com",
            "leads-k7f3q2wd@",
            "",
            "leads-k7f3q2wd@x@cypress-bay-realty.elysianfeld.com",
            "leads-k7f3q2wd@cypress-bay-realty.elysianfeld.com.",
            "leads-k7f3q2wd @cypress-bay-realty.elysianfeld.com",
            "leads-k7f3q2wd@cypress-bay-realty.xelysianfeld.com",
        ] {
            assert_eq!(IntakeAddress::parse_recipient(bad, &c), None, "{bad}");
        }
    }

    #[test]
    fn an_org_whose_slug_is_leads_is_unambiguous_across_both_forms() {
        let c = cfg(IntakeAddressScheme::Subdomain);
        let expected = Some(IntakeAddress {
            slug: "leads".into(),
            token: "abcdefgh".into(),
        });
        assert_eq!(
            IntakeAddress::parse_recipient("leads-abcdefgh@leads.elysianfeld.com", &c),
            expected
        );
    }

    #[test]
    fn slug_check_mirrors_the_db_constraint() {
        assert!(is_slug("a"));
        assert!(is_slug("acme-realty"));
        assert!(is_slug(&"a".repeat(40)));
        assert!(!is_slug(&"a".repeat(41)));
        assert!(!is_slug("-acme"));
        assert!(!is_slug("acme-"));
        assert!(!is_slug("Acme"));
        assert!(!is_slug("acme realty"));
        assert!(!is_slug(""));
    }
}
