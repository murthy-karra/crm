//! Pinned-format email parsing (docs/specs/SLICE_007d.md §4). Layering:
//! `mime.rs` is the ONLY module in the workspace allowed to name the MIME
//! crate (enforced by the directory-walk fence test below); `format.rs`
//! defines the `EmailFormat` trait, the registry, and the
//! `ExtractedLead → ParsedLead` normalization; `formats/` holds one module
//! per pinned template (007h adds more).

pub mod format;
pub mod formats;
pub mod forward;
pub mod mime;

pub use format::detect;
pub use forward::SenderTrust;
pub use mime::ParsedMail;

use crate::domain::inquiry::parse::{ParsedLead, Source, UnresolvedReason};

/// The email parse pipeline — MIME → direct format detection → (on
/// no-match) forwarded-wrapper resolve → re-detection → field extraction
/// → normalization — as one named function so the delivery path
/// (`receive_inbound_email`) and the workbench retry
/// (docs/specs/SLICE_007e.md §4) cannot drift. Direct detection runs
/// FIRST (docs/specs/SLICE_007h1.md §3, reviewer S-2): a genuine direct
/// mail whose body quotes a forward banner keeps its deterministic
/// parse; the unwrapper only ever sees mail nothing matched as
/// delivered. The static format/style names are the only derived values
/// observability may record (SLICE_007d §8); recording is a no-op on
/// spans that do not declare the fields.
pub(crate) fn parse_payload(bytes: &[u8]) -> Result<(Source, ParsedLead), UnresolvedReason> {
    let mail = mime::parse(bytes).ok_or(UnresolvedReason::EmailUnparsed)?;

    if let Some(email_format) = detect(&mail, SenderTrust::Direct) {
        tracing::Span::current().record("format", email_format.name());
        return format::to_parsed_lead(email_format.extract(&mail));
    }

    let resolved = forward::resolve(mail);
    let forward::SenderTrust::ForwardedClaim { depth } = resolved.trust else {
        return Err(UnresolvedReason::EmailUnrecognizedFormat);
    };
    let span = tracing::Span::current();
    span.record("forwarded", true);
    span.record("forward_depth", depth);
    if let Some(style) = resolved.style {
        span.record("forward_style", style);
    }
    let email_format =
        detect(&resolved.mail, resolved.trust).ok_or(UnresolvedReason::EmailUnrecognizedFormat)?;
    span.record("format", email_format.name());
    format::to_parsed_lead(email_format.extract(&resolved.mail))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    /// The fence (docs/specs/SLICE_007d.md §4a): only `mime.rs` may
    /// mention the MIME crate. A directory walk, not `include_str!` on
    /// known paths — the realistic future leak site is a `formats/*.rs`
    /// file that does not exist yet (007h), and a static file list would
    /// silently miss it. The needle is assembled at runtime so this test
    /// file's own source never contains it.
    #[test]
    fn only_mime_rs_references_the_mime_crate() {
        let needle = ["mail", "parser"].join("_");
        assert_fenced(
            "src/domain/intake/email",
            &needle,
            "mime.rs",
            "references the MIME crate",
        );
    }

    /// The unwrap seam fence (docs/specs/SLICE_007h1.md §3): only
    /// `forward.rs` may match the forward banner — a second
    /// banner-matching site would let pinned matching and the LLM input
    /// builder drift apart, the exact failure the shared `resolve` seam
    /// exists to prevent. Walks the whole intake module (reviewer F2):
    /// the likeliest drift site is the extraction worker, not a formats
    /// file — test fixtures there assemble the banner at runtime.
    #[test]
    fn only_forward_rs_matches_the_forward_banner() {
        let needle = ["Forwarded", "message"].join(" ");
        assert_fenced(
            "src/domain/intake",
            &needle,
            "forward.rs",
            "matches the forward banner",
        );
    }

    fn assert_fenced(dir: &str, needle: &str, allowed_file: &str, violation: &str) {
        let email_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(dir);
        let mut checked = 0usize;
        walk(&email_dir, needle, allowed_file, violation, &mut checked);
        // Guard against the walk silently checking nothing (e.g. a moved
        // directory): this module alone is mod.rs + mime.rs + format.rs +
        // forward.rs + formats/mod.rs + formats/cypress_bay.rs.
        assert!(checked >= 4, "fence walked only {checked} files");

        fn walk(
            dir: &Path,
            needle: &str,
            allowed_file: &str,
            violation: &str,
            checked: &mut usize,
        ) {
            for entry in std::fs::read_dir(dir).expect("read email module dir") {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    walk(&path, needle, allowed_file, violation, checked);
                    continue;
                }
                if path.extension().is_none_or(|e| e != "rs") {
                    continue;
                }
                *checked += 1;
                if path.file_name().is_some_and(|n| n == allowed_file) {
                    continue;
                }
                let contents = std::fs::read_to_string(&path).expect("read source file");
                assert!(
                    !contents.contains(needle),
                    "{} {violation} outside the {allowed_file} fence",
                    path.display()
                );
            }
        }
    }
}
