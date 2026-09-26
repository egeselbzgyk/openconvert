//! One stylesheet, and what it deliberately does not say (D13.11, R5 §A8).
//!
//! The rule that matters is a rule about absence. No `font-family`, no absolute size, no
//! `position: absolute`. Setting fonts and sizes is the most common source of "this ebook
//! looks wrong on my device", and it fights the reader's own accessibility settings — a
//! reader who has chosen a dyslexia-friendly face at 18 pt has said what they want, and a
//! converter that overrides them has decided it knows better.
//!
//! `position: absolute` is the sharper case. Carrying PDF coordinates into CSS technically
//! "works" in a webview and destroys the entire value proposition of reflow: the output is a
//! picture of a page rather than a book. It is the single thing this project exists not to do.
//!
//! So the stylesheet is short, it is relative throughout, and it says only what the markup
//! cannot: which classes exist and what they mean.

use crate::xhtml::CssClass;

/// Where the stylesheet lives inside the container.
pub const STYLE_PATH: &str = "style.css";

/// The whole stylesheet.
///
/// Every class in [`CssClass::ALL`] has a rule here, and a test proves it: the markup and the
/// stylesheet are two files that have to agree, and a class named in one and not the other
/// fails silently — the markup is valid, the rule simply never matches, and nobody finds out
/// until a reader sees a poem set as prose.
pub fn stylesheet() -> String {
    let mut out = String::new();

    out.push_str(
        "/* OpenConvert. Relative units throughout. This sheet names no typeface, no\n\
         \x20  absolute size and no coordinates: the reader's own settings win. */\n\n",
    );

    // `section[epub|type~="chapter"]` is a namespaced attribute selector and is simply
    // invalid without this: an undeclared prefix makes the whole rule a parse error, so the
    // chapter break would silently never apply.
    out.push_str("@namespace epub \"http://www.idpf.org/2007/ops\";\n\n");

    out.push_str(
        "html { font-size: 100%; }\n\
         body { margin: 0 5%; line-height: 1.4; widows: 2; orphans: 2; }\n\
         p { margin: 0; text-indent: 1.2em; }\n\
         p:first-child, h1 + p, h2 + p, h3 + p, h4 + p, h5 + p, h6 + p { text-indent: 0; }\n\
         h1, h2, h3, h4, h5, h6 { line-height: 1.2; text-indent: 0; page-break-after: avoid; \
         break-after: avoid; }\n\
         h1 { font-size: 1.6em; margin: 1.2em 0 0.8em; }\n\
         h2 { font-size: 1.4em; margin: 1.1em 0 0.7em; }\n\
         h3 { font-size: 1.2em; margin: 1em 0 0.6em; }\n\
         h4, h5, h6 { font-size: 1.05em; margin: 1em 0 0.5em; }\n\n",
    );

    // A chapter starts on a new page in print and should start on a new screen here. Both
    // spellings, because e-ink firmware in the field still reads the prefixed-era property.
    out.push_str(
        "section[epub|type~=\"chapter\"], section[epub|type~=\"part\"] {\n\
         \x20 page-break-before: always; break-before: page;\n\
         }\n\n",
    );

    out.push_str(
        "blockquote { margin: 1em 2em; }\n\
         blockquote p { text-indent: 0; }\n\
         figure { margin: 1em 0; text-align: center; }\n\
         figure img { max-width: 100%; height: auto; }\n\
         section[epub|type~=\"cover\"] figure { margin: 0; }\n\
         section[epub|type~=\"cover\"] img { max-height: 97vh; object-fit: contain; }\n\
         table { border-collapse: collapse; margin: 1em 0; }\n\
         th, td { border: 1px solid; padding: 0.3em 0.5em; text-align: left; }\n\
         pre { white-space: pre-wrap; overflow-wrap: break-word; }\n\
         aside[epub|type~=\"footnote\"] { font-size: 0.9em; }\n\n",
    );

    for class in CssClass::ALL {
        out.push_str(&rule(*class));
    }

    out
}

/// The rule for one class. A `match` rather than a table, so a new variant is a compile error
/// here and not a silently missing rule.
fn rule(class: CssClass) -> String {
    let selector = format!(".{}", class.as_str());
    let body = match class {
        // A poem's lines are its own; the container only stops the reader's indent rule from
        // applying to them.
        CssClass::Verse => "margin: 1em 0 1em 2em;",
        CssClass::Stanza => "text-indent: 0; margin: 0 0 1em; white-space: pre-line;",
        // A drop cap is a typographic flourish, and on a 4-inch screen it is a liability. The
        // initial is styled, never positioned.
        CssClass::DropCap => "font-size: 2.5em; line-height: 1; float: left; padding-right: 0.1em;",
        CssClass::SmallCaps => "font-variant: small-caps;",
        CssClass::Caption => "text-indent: 0; font-size: 0.9em; text-align: center;",
        // The page-break marker holds no text and must occupy no space: its label is an
        // attribute, and a reading system that draws it would show a folio the book removed.
        CssClass::PageBreak => "display: none;",
        // The book printed its own markers and `structure` left them in the item text, so the
        // reading system must not draw a second set (`docs/DECISIONS_LOG.md`).
        CssClass::ListPrintedMarkers => "list-style-type: none; padding-left: 0;",
        CssClass::TableFallback => "margin: 0.5em 0; font-size: 0.9em;",
    };
    format!("{selector} {{ {body} }}\n")
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Row 5.12.
// ---------------------------------------------------------------------------

/// Row 5.12. The stylesheet's job is mostly to keep quiet: a `font-family` or an absolute
/// size overrides a reader who has already said what they want, and it is the most common
/// cause of "this ebook looks wrong on my device" (R5 §A8).
#[test]
fn css_has_no_font_family_or_absolute_size() {
    let css = stylesheet();

    assert!(
        !css.contains("font-family"),
        "the reader's face is the reader's choice"
    );
    assert!(
        !css.contains("position:"),
        "PDF coordinates in CSS are a picture of a page, not a book"
    );

    // Absolute length units, in the places CSS accepts one. `1px` borders on table cells are
    // the one exception people argue for; a hairline is `1px` everywhere and scales with
    // nothing, so it is allowed and nothing else is.
    for line in css.lines() {
        let measured = line.replace("1px solid", "");
        for unit in ["pt", "px", "cm", "mm", "in", "pc"] {
            for (index, _) in measured.match_indices(unit) {
                let before = measured[..index].chars().next_back().unwrap_or(' ');
                assert!(!before.is_ascii_digit(), "absolute unit {unit} in: {line}");
            }
        }
    }
}

/// A class named in the markup and missing from the stylesheet is valid XHTML whose rule never
/// matches — a poem set as prose, with no error anywhere. The two files have to agree, and
/// this is what makes them.
#[test]
fn every_class_the_builder_can_emit_has_a_rule() {
    let css = stylesheet();
    for class in CssClass::ALL {
        assert!(
            css.contains(&format!(".{} {{", class.as_str())),
            "no rule for .{}",
            class.as_str()
        );
    }
}

/// A chapter starts on a new screen, in both spellings, because e-ink firmware in the field
/// still reads only the older one — and the selector that does it needs a namespace prefix
/// that is declared, or the whole rule is a parse error and nothing breaks at all.
#[test]
fn a_chapter_breaks_the_page_before_it() {
    let css = stylesheet();
    assert!(css.contains("page-break-before: always"));
    assert!(css.contains("break-before: page"));

    let namespace = css
        .find("@namespace epub \"http://www.idpf.org/2007/ops\";")
        .expect("the epub prefix is declared");
    let selector = css.find("epub|type").expect("and is used");
    assert!(namespace < selector, "declared before it is used");
}
