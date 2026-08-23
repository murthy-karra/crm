//! The session-token *format* check, and nothing else of the session
//! layer: `domain::admin::commands::token` needs it, so it lives in the
//! application layer (docs/specs/SLICE_006a.md §4) while session
//! creation/verification, cookies, and `SessionSecret` stay in crm-api
//! (D-028 §1: the Operator never reaches the session layer).

/// Base64url length of the 32 random token bytes.
pub const TOKEN_STR_LEN: usize = 43;

pub fn is_valid_token_format(token: &str) -> bool {
    token.len() == TOKEN_STR_LEN
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_wrong_length_token_format() {
        assert!(!is_valid_token_format("too-short"));
        assert!(!is_valid_token_format(&"a".repeat(44)));
    }

    #[test]
    fn rejects_non_base64url_characters() {
        let mut bad = "a".repeat(TOKEN_STR_LEN - 1);
        bad.push('+'); // not in the URL-safe alphabet
        assert!(!is_valid_token_format(&bad));
    }
}
