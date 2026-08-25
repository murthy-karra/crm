//! Pinned-format email parsing (docs/specs/SLICE_007d.md §4). Layering:
//! `mime.rs` is the ONLY module in the workspace allowed to name the MIME
//! crate (enforced by the directory-walk fence test below); `format.rs`
//! defines the `EmailFormat` trait, the registry, and the
//! `ExtractedLead → ParsedLead` normalization; `formats/` holds one module
//! per pinned template (007h adds more).

pub mod format;
pub mod formats;
pub mod mime;

pub use format::detect;
pub use mime::ParsedMail;

use crate::domain::inquiry::parse::{ParsedLead, Source, UnresolvedReason};

/// The email parse pipeline — MIME → format detection → field extraction
/// → normalization — as one named function so the delivery path
/// (`receive_inbound_email`) and the workbench retry
/// (docs/specs/SLICE_007e.md §4) cannot drift. The static format name is
/// the one format-derived value observability may record
/// (docs/specs/SLICE_007d.md §8); recording is a no-op on spans that do
/// not declare the field.
pub(crate) fn parse_payload(bytes: &[u8]) -> Result<(Source, ParsedLead), UnresolvedReason> {
    let mail = mime::parse(bytes).ok_or(UnresolvedReason::EmailUnparsed)?;
    let email_format = detect(&mail).ok_or(UnresolvedReason::EmailUnrecognizedFormat)?;
    tracing::Span::current().record("format", email_format.name());
    format::to_parsed_lead(email_format.extract(&mail))
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
        let email_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/domain/intake/email");
        let mut checked = 0usize;
        walk(&email_dir, &needle, &mut checked);
        // Guard against the walk silently checking nothing (e.g. a moved
        // directory): this module alone is mod.rs + mime.rs + format.rs +
        // formats/mod.rs + formats/cypress_bay.rs.
        assert!(checked >= 4, "fence walked only {checked} files");

        fn walk(dir: &Path, needle: &str, checked: &mut usize) {
            for entry in std::fs::read_dir(dir).expect("read email module dir") {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    walk(&path, needle, checked);
                    continue;
                }
                if path.extension().is_none_or(|e| e != "rs") {
                    continue;
                }
                *checked += 1;
                if path.file_name().is_some_and(|n| n == "mime.rs") {
                    continue;
                }
                let contents = std::fs::read_to_string(&path).expect("read source file");
                assert!(
                    !contents.contains(needle),
                    "{} references the MIME crate outside the mime.rs fence",
                    path.display()
                );
            }
        }
    }
}
