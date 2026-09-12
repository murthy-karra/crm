//! Versioned non-executing HTML-to-text profile for retained main notes.
//!
//! html5ever performs HTML tokenization/entity decoding. A small strict stack
//! rejects malformed/unsupported structure instead of repairing it by dropping
//! content. There is no DOM, URL parser, resource fetch or script execution.

use std::cell::RefCell;

use html5ever::tendril::StrTendril;
use html5ever::tokenizer::{
    BufferQueue, Tag, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts,
};

const MAX_INPUT_BYTES: usize = 4 * 1024 * 1024;
const MAX_OUTPUT_BYTES: usize = 40_000;
const MAX_DEPTH: usize = 64;
const MAX_TOKENS: usize = 100_000;
const MAX_ATTRIBUTES: usize = 128;

pub(crate) struct Converted {
    pub text: String,
    pub comments: u64,
}

struct Element {
    name: String,
    href: Option<String>,
    next_number: Option<i64>,
}

#[derive(Default)]
struct State {
    text: String,
    stack: Vec<Element>,
    tokens: usize,
    comments: u64,
    error: Option<&'static str>,
    pending_space: bool,
    pending_breaks: usize,
}

#[derive(Default)]
struct Sink(RefCell<State>);

pub(crate) fn convert(raw: &str) -> Result<Converted, &'static str> {
    if raw.len() > MAX_INPUT_BYTES {
        return Err("html_input_budget_exceeded");
    }
    if raw
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\r' | '\n' | '\t'))
    {
        return Err("html_invalid_control");
    }
    let input = BufferQueue::default();
    input.push_back(StrTendril::from_slice(raw));
    let tokenizer = Tokenizer::new(
        Sink::default(),
        TokenizerOpts {
            exact_errors: true,
            discard_bom: false,
            ..TokenizerOpts::default()
        },
    );
    let _ = tokenizer.feed(&input);
    tokenizer.end();
    let state = tokenizer.sink.0.into_inner();
    if let Some(error) = state.error {
        return Err(error);
    }
    if !state.stack.is_empty() {
        return Err("html_malformed_structure");
    }
    Ok(Converted {
        text: state.text,
        comments: state.comments,
    })
}

impl TokenSink for Sink {
    type Handle = ();

    fn process_token(&self, token: Token, _line_number: u64) -> TokenSinkResult<()> {
        let mut state = self.0.borrow_mut();
        // Continue consuming bounded source after failure; keep no further text,
        // names, attributes, diagnostic strings or stack allocations.
        if state.error.is_some() {
            return TokenSinkResult::Continue;
        }
        state.tokens += 1;
        if state.tokens > MAX_TOKENS {
            state.error = Some("html_token_budget_exceeded");
            return TokenSinkResult::Continue;
        }
        let result = match token {
            Token::TagToken(tag) => state.tag(tag),
            Token::CharacterTokens(text) => state.characters(&text),
            Token::CommentToken(_) => {
                state.comments += 1;
                Ok(())
            }
            Token::EOFToken => Ok(()),
            Token::NullCharacterToken | Token::ParseError(_) => Err("html_malformed_structure"),
            Token::DoctypeToken(_) => Err("html_unsupported_element"),
        };
        if let Err(error) = result {
            state.error = Some(error);
        }
        TokenSinkResult::Continue
    }
}

fn block(name: &str) -> bool {
    matches!(
        name,
        "p" | "div" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "blockquote" | "pre"
    )
}

fn supported(name: &str) -> bool {
    block(name)
        || matches!(
            name,
            "br" | "ul"
                | "ol"
                | "li"
                | "a"
                | "span"
                | "strong"
                | "em"
                | "b"
                | "i"
                | "u"
                | "s"
                | "strike"
                | "del"
                | "ins"
                | "code"
                | "small"
                | "mark"
                | "sub"
                | "sup"
                | "html"
                | "body"
        )
}

impl State {
    fn append(&mut self, text: &str) -> Result<(), &'static str> {
        // Defer structural trailing line breaks until another content token.
        // They would be trimmed by NoteBody; storing them eagerly would reject
        // a valid 10,000-character astral body solely for its closing </p>.
        let breaks = if self.text.is_empty() {
            0
        } else {
            self.pending_breaks
        };
        if self
            .text
            .len()
            .saturating_add(text.len())
            .saturating_add(breaks)
            > MAX_OUTPUT_BYTES
        {
            return Err("html_output_budget_exceeded");
        }
        for _ in 0..breaks {
            self.text.push('\n');
        }
        self.pending_breaks = 0;
        self.text.push_str(text);
        Ok(())
    }

    fn boundary(&mut self, force: bool) -> Result<(), &'static str> {
        self.pending_space = false;
        if force {
            self.pending_breaks += 1;
        } else if !self.text.is_empty() && !self.text.ends_with('\n') && self.pending_breaks == 0 {
            self.pending_breaks = 1;
        }
        Ok(())
    }

    fn characters(&mut self, text: &str) -> Result<(), &'static str> {
        if self
            .stack
            .last()
            .is_some_and(|element| matches!(element.name.as_str(), "ul" | "ol"))
        {
            return if text
                .chars()
                .all(|c| matches!(c, ' ' | '\t' | '\r' | '\n' | '\u{c}'))
            {
                Ok(())
            } else {
                Err("html_malformed_structure")
            };
        }
        if self.stack.iter().any(|element| element.name == "pre") {
            self.pending_space = false;
            return self.append(text);
        }
        for character in text.chars() {
            if matches!(character, ' ' | '\t' | '\r' | '\n' | '\u{c}') {
                self.pending_space = true;
            } else {
                if self.pending_space
                    && self.pending_breaks == 0
                    && !self.text.is_empty()
                    && !self.text.ends_with(['\n', ' '])
                {
                    self.append(" ")?;
                }
                self.pending_space = false;
                self.append(character.encode_utf8(&mut [0; 4]))?;
            }
        }
        Ok(())
    }

    fn tag(&mut self, tag: Tag) -> Result<(), &'static str> {
        let name = tag.name.as_ref();
        if !supported(name) {
            return Err("html_unsupported_element");
        }
        if tag.attrs.len() > MAX_ATTRIBUTES {
            return Err("html_attribute_budget_exceeded");
        }
        if tag.kind == TagKind::EndTag {
            if !tag.attrs.is_empty() || tag.self_closing {
                return Err("html_malformed_structure");
            }
            let element = self.stack.pop().ok_or("html_malformed_structure")?;
            if element.name != name {
                return Err("html_malformed_structure");
            }
            if let Some(href) = element.href {
                // Literal decoded attribute, even a non-HTTP destination. This
                // is plain untrusted text, never a clickable/loaded resource.
                self.pending_space = false;
                self.append(" (")?;
                self.append(&href)?;
                self.append(")")?;
            }
            if block(name) || matches!(name, "li" | "ol" | "ul") {
                self.boundary(false)?;
            }
            return Ok(());
        }
        if tag.self_closing && name != "br" {
            return Err("html_malformed_structure");
        }
        let mut href = None;
        let mut next_number = (name == "ol").then_some(1_i64);
        let mut item_number = None;
        for attribute in &tag.attrs {
            if !attribute.name.ns.is_empty() || attribute.name.prefix.is_some() {
                return Err("html_unsupported_attribute");
            }
            let key = attribute.name.local.as_ref();
            let value = attribute.value.as_ref();
            match (name, key) {
                ("a", "href") => {
                    if value.chars().any(char::is_control) {
                        return Err("html_unsupported_attribute");
                    }
                    href = Some(value.to_owned());
                }
                ("ol", "start") => {
                    next_number = Some(list_number(value)?);
                }
                ("li", "value") => {
                    item_number = Some(list_number(value)?);
                }
                // These only annotate/style a supported element; all styling
                // loss is disclosed. Inline CSS is held: it can hide content,
                // reference resources or override pre whitespace semantics.
                (_, "class" | "id" | "lang" | "dir") => {}
                // Link browsing hints never become native navigation behavior.
                ("a", "target" | "rel") => {}
                _ => return Err("html_unsupported_attribute"),
            }
        }
        if name == "br" {
            return self.boundary(true);
        }
        if self.stack.len() >= MAX_DEPTH {
            return Err("html_depth_budget_exceeded");
        }
        if name == "a" && self.stack.iter().any(|element| element.name == "a") {
            return Err("html_malformed_structure");
        }
        if block(name) && self.stack.iter().any(|element| element.name == "p") {
            // HTML implicitly closes a p around blocks. That repair can change
            // the meaning of malformed input; this profile requires exact nesting.
            return Err("html_malformed_structure");
        }
        if block(name) || matches!(name, "ul" | "ol" | "li") {
            self.boundary(false)?;
        }
        if name == "li" {
            let parent = self.stack.last_mut().ok_or("html_malformed_structure")?;
            let marker = match parent.name.as_str() {
                "ul" if item_number.is_none() => "• ".to_owned(),
                "ol" => {
                    let number = item_number
                        .or(parent.next_number)
                        .ok_or("html_invalid_list")?;
                    parent.next_number = Some(number.checked_add(1).ok_or("html_invalid_list")?);
                    format!("{number}. ")
                }
                _ => return Err("html_malformed_structure"),
            };
            let nested = self
                .stack
                .iter()
                .filter(|element| matches!(element.name.as_str(), "ol" | "ul"))
                .count()
                .saturating_sub(1);
            self.append(&"  ".repeat(nested))?;
            self.append(&marker)?;
        } else if self
            .stack
            .last()
            .is_some_and(|element| matches!(element.name.as_str(), "ol" | "ul"))
        {
            return Err("html_malformed_structure");
        }
        self.stack.push(Element {
            name: name.to_owned(),
            href,
            next_number,
        });
        Ok(())
    }
}

fn list_number(value: &str) -> Result<i64, &'static str> {
    value
        .parse::<i64>()
        .ok()
        .filter(|number| (-1_000_000_000..=1_000_000_000).contains(number))
        .ok_or("html_invalid_list")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(raw: &str) -> String {
        convert(raw).unwrap().text.trim().to_owned()
    }

    #[test]
    fn paragraphs_headings_entities_emphasis_and_breaks_keep_text() {
        assert_eq!(
            text("<h2>Résumé &amp; search</h2><div>A <strong>good</strong>\t home<br>for &lt;5 people.</div>"),
            "Résumé & search\nA good home\nfor <5 people."
        );
        assert_eq!(text("a<br><br>b"), "a\n\nb");
        assert_eq!(text("<span>one </span><b> two</b>"), "one two");
    }

    #[test]
    fn lists_keep_order_nesting_and_number_overrides() {
        assert_eq!(
            text("<ol start=3><li>First<ul><li>Nested</li></ul></li><li value=7>Second</li><li>Third</li></ol>"),
            "3. First\n  • Nested\n7. Second\n8. Third"
        );
    }

    #[test]
    fn pre_spacing_and_literal_anchor_destination_are_not_executed() {
        assert_eq!(
            text("<p>Code:</p><pre>  a  b\n\tc</pre><p><a href=\"javascript:untrusted()\">label</a></p>"),
            "Code:\n  a  b\n\tc\nlabel (javascript:untrusted())"
        );
        assert_eq!(
            text("<a href=\"https://example.test/?a=1&amp;b=2\">Site</a>"),
            "Site (https://example.test/?a=1&b=2)"
        );
    }

    #[test]
    fn unsupported_content_or_attributes_hold_instead_of_being_dropped() {
        for raw in [
            "before<img src=x alt='customer content'>after",
            "<table><tr><td>data</td></tr></table>",
            "<script>alert(1)</script>",
            "<style>body {content:'text'}</style>",
            "<iframe src=x></iframe>",
            "<svg><text>content</text></svg>",
            "<div style='display:none'>content</div>",
            "<span title='extra content'>text</span>",
            "<span onclick='alert(1)'>text</span>",
            "<ol reversed><li>three</li></ol>",
        ] {
            assert!(convert(raw).is_err());
        }
    }

    #[test]
    fn malformed_html_and_duplicate_attributes_are_held() {
        for raw in [
            "<p>unclosed",
            "<p><b>x</p></b>",
            "<p>a<div>b</div></p>",
            "<a href='one' href='two'>x</a>",
            "<a href='one><b>x</b>",
            "<li>outside</li>",
            "<ul>orphan content<li>one</li></ul>",
            "<div/>lost?",
            "<a><a>nested</a></a>",
            "x &notARealEntity; y",
            "bad\0value",
        ] {
            assert!(convert(raw).is_err());
        }
    }

    #[test]
    fn parser_budgets_hold_without_returning_truncated_output() {
        assert!(convert(&"x".repeat(MAX_INPUT_BYTES + 1)).is_err());
        assert!(convert(&"x".repeat(MAX_OUTPUT_BYTES + 1)).is_err());
        assert!(convert(&"<!---->".repeat(MAX_TOKENS + 1)).is_err());
        assert!(convert(&format!("{}x{}", "<div>".repeat(65), "</div>".repeat(65))).is_err());
        let attributes = (0..129)
            .map(|n| format!(" data-{n}='x'"))
            .collect::<String>();
        assert!(convert(&format!("<div{attributes}>x</div>")).is_err());
    }

    #[test]
    fn comments_remain_separately_countable() {
        let converted = convert("<p>a<!-- retained comment -->b</p>").unwrap();
        assert_eq!(converted.text.trim(), "ab");
        assert_eq!(converted.comments, 1);
    }
}
