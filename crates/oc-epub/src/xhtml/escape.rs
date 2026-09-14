//! Turning text into XML text, and refusing what XML cannot carry.
//!
//! Two jobs, and the second is the one that matters. Escaping `&`, `<` and `>` is table
//! stakes; the interesting case is a character XML 1.0 has no spelling for at all — a C0
//! control other than tab, line feed and carriage return, or one of the two noncharacters at
//! the end of the BMP. There is no character reference for those: `&#1;` is as ill-formed as
//! the raw byte, so an escaper cannot rescue them.
//!
//! The emitter therefore **refuses** rather than dropping them, because `epub` is Conserving
//! with an empty ledger (D13.4, I-3) and a stage that silently deleted a scalar would be
//! removing text it cannot account for. Refusing is also the honest signal: a control
//! character in the body flow means a page that decoded to garbage reached the text path, and
//! PIPELINE §2 routes those pages to OCR or to a page image precisely so that they do not.
//! "There is no fallback path. An emitter failure is a bug" (PIPELINE §10).

/// A character XML 1.0 cannot represent, and where it was found.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("U+{:04X} cannot appear in XML 1.0 text (in {context:?})", *.ch as u32)]
pub struct IllegalChar {
    pub ch: char,
    /// The text it was found in, so a report can quote the neighbourhood.
    pub context: String,
}

/// Whether XML 1.0 §2.2 permits this scalar in a document at all.
///
/// Tab, line feed and carriage return are the three C0 controls that are allowed; every other
/// one is not, and neither are U+FFFE and U+FFFF. Surrogates cannot occur in a Rust `str`.
pub fn is_xml_char(ch: char) -> bool {
    matches!(ch, '\t' | '\n' | '\r')
        || matches!(ch, ' '..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')
}

/// The first character of `text` XML cannot carry, if there is one.
pub fn check(text: &str) -> Result<(), IllegalChar> {
    match text.chars().find(|ch| !is_xml_char(*ch)) {
        Some(ch) => Err(IllegalChar {
            ch,
            context: text.chars().take(40).collect(),
        }),
        None => Ok(()),
    }
}

/// Escape text for an element's content.
///
/// `>` is escaped as well as `<` and `&`, which XML does not require outside a `]]>` sequence.
/// It costs three bytes and removes the one case where the requirement bites, so there is no
/// reason to be clever about it.
pub fn text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            other => out.push(other),
        }
    }
    out
}

/// Escape text for a double-quoted attribute value.
///
/// Both quote characters are escaped, not only the delimiter: an attribute value is read by
/// people as well as parsers, and a value that is safe under one quoting style and not the
/// other is a trap for the next person who changes the serialiser.
pub fn attribute(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // A literal newline or tab in an attribute is normalised away by every XML parser
            // (XML 1.0 §3.3.3), so a label containing one would silently change on the way in.
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\t' => out.push_str("&#9;"),
            other => out.push(other),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2)
// ---------------------------------------------------------------------------

/// The three characters that end a book when they are not escaped, plus the one case people
/// forget: `&` inside an attribute.
#[test]
fn the_markup_characters_are_escaped_on_both_sides_of_the_tag() {
    assert_eq!(text("a & b < c > d"), "a &amp; b &lt; c &gt; d");
    assert_eq!(
        attribute("Q&A \"quoted\" 'single'"),
        "Q&amp;A &quot;quoted&quot; &apos;single&apos;"
    );
    // Escaping is idempotent only in the sense that it never loses: escaping twice produces
    // visible entities, which is what a double-escape bug looks like in a reader.
    assert_eq!(text("&amp;"), "&amp;amp;");
}

/// Attribute-value normalisation turns a literal newline into a space before any consumer
/// sees it, so a `page-list` label printed across two lines would change on the way in.
#[test]
fn whitespace_in_an_attribute_survives_as_a_character_reference() {
    assert_eq!(attribute("two\nlines"), "two&#10;lines");
    assert_eq!(attribute("a\tb"), "a&#9;b");
}

/// The characters XML has no spelling for. An escaper cannot rescue them — `&#1;` is as
/// ill-formed as the byte — so they have to be refused, and refused with enough context to
/// find them.
#[test]
fn a_character_xml_cannot_carry_is_refused_rather_than_dropped() {
    assert!(check("ordinary text\twith\ttabs\nand newlines").is_ok());
    assert!(check("emoji 🜁 and 中文 are fine").is_ok());

    let refused = check("before\u{1}after").expect_err("U+0001 is not an XML character");
    assert_eq!(refused.ch, '\u{1}');
    assert!(refused.context.contains("before"));

    assert!(check("\u{fffe}").is_err());
    assert!(
        check("\u{b}").is_err(),
        "vertical tab is not one of the three"
    );
    assert!(
        check("\u{fffd}").is_ok(),
        "the replacement character is legal"
    );
}
