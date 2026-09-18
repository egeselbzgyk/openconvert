//! Localised warning templates, filled deterministically (PIPELINE §13, R10 §6.20).
//!
//! **Warnings are never model-phrased, and the reason is stronger than cost.** A warning is a
//! *factual claim about what the software did* — "3 tables were rendered as images because their
//! structure could not be recovered" — and a paraphrase that misstates it is a trust bug, not a
//! style issue. Templates are also translatable, reviewable, assertable and reproducible; and a
//! 1–4B model's German and Turkish are worse than its English, so the users most in need of clarity
//! would get the worst text.
//!
//! The templates ship as TOML beside this file and are parsed once, lazily. They are *not* baked
//! into the binary by a build script: a translation is text a translator should be able to correct
//! without understanding the build, and the CI check that every code has a template in every locale
//! is a test rather than a compile error for the same reason.

pub mod codes;

use std::collections::BTreeMap;
use std::sync::OnceLock;

pub use codes::{spec, WarningSpec, CODES};

/// The locales v1 ships (D13.11's `locale` job-spec field defaults to `en`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Locale {
    En,
    De,
    Tr,
}

impl Locale {
    /// Every locale, in a fixed order.
    pub const ALL: [Locale; 3] = [Locale::En, Locale::De, Locale::Tr];

    /// The BCP-47 primary subtag.
    pub fn as_str(self) -> &'static str {
        match self {
            Locale::En => "en",
            Locale::De => "de",
            Locale::Tr => "tr",
        }
    }

    /// The locale a tag names, falling back to English.
    ///
    /// Falls back rather than failing: a job spec asking for a locale v1 does not ship should get a
    /// book with English warnings, not no book.
    pub fn from_tag(tag: &str) -> Locale {
        match tag
            .split(['-', '_'])
            .next()
            .unwrap_or(tag)
            .to_ascii_lowercase()
            .as_str()
        {
            "de" => Locale::De,
            "tr" => Locale::Tr,
            _ => Locale::En,
        }
    }

    fn source(self) -> &'static str {
        match self {
            Locale::En => include_str!("templates_en.toml"),
            Locale::De => include_str!("templates_de.toml"),
            Locale::Tr => include_str!("templates_tr.toml"),
        }
    }
}

/// The parsed template table for one locale: code → template text.
fn table(locale: Locale) -> &'static BTreeMap<String, String> {
    static TABLES: OnceLock<BTreeMap<Locale, BTreeMap<String, String>>> = OnceLock::new();
    let tables = TABLES.get_or_init(|| {
        Locale::ALL
            .into_iter()
            .map(|locale| (locale, parse(locale.source())))
            .collect()
    });
    tables.get(&locale).unwrap_or_else(|| {
        // Unreachable: `TABLES` is built from `Locale::ALL`. Written as a fallback rather than an
        // `expect` because `#![forbid]`-clean production code does not panic (CLAUDE.md §2).
        static EMPTY: OnceLock<BTreeMap<String, String>> = OnceLock::new();
        EMPTY.get_or_init(BTreeMap::new)
    })
}

/// `KEY = "value"` pairs, as TOML. Malformed input yields an empty table rather than a panic; the
/// test in this file is what makes malformed input impossible to ship.
fn parse(source: &str) -> BTreeMap<String, String> {
    let Ok(value) = toml::from_str::<toml::Value>(source) else {
        return BTreeMap::new();
    };
    let Some(table) = value.as_table() else {
        return BTreeMap::new();
    };
    table
        .iter()
        .filter_map(|(key, value)| value.as_str().map(|text| (key.clone(), text.to_owned())))
        .collect()
}

/// The raw template for one code in one locale.
pub fn template(locale: Locale, code: &str) -> Option<&'static str> {
    table(locale).get(code).map(String::as_str)
}

/// Fill a code's template with its arguments.
///
/// A slot with no argument is left as written — `{pages}` stays `{pages}` — rather than being
/// silently emptied: a warning missing a number is a warning that has stopped being a factual
/// claim, and a visible brace is a bug report where an empty space is a mystery.
///
/// `None` when the locale has no template for the code, which the CI check makes impossible for a
/// code the registry knows.
pub fn render(locale: Locale, code: &str, args: &BTreeMap<String, String>) -> Option<String> {
    let template = template(locale, code)?;
    Some(fill(template, args))
}

fn fill(template: &str, args: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) => {
                let name = &after[..close];
                match args.get(name) {
                    Some(value) => out.push_str(value),
                    None => {
                        out.push('{');
                        out.push_str(name);
                        out.push('}');
                    }
                }
                rest = &after[close + 1..];
            }
            None => {
                // An unclosed brace is text, not a slot.
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Every `{slot}` a template names, in order of appearance and without repeats.
pub fn slots(template: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) => {
                let name = after[..close].to_owned();
                if !found.contains(&name) {
                    found.push(name);
                }
                rest = &after[close + 1..];
            }
            None => break,
        }
    }
    found
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Row 6.11 of the Phase 6 table, plus the
// properties of the registry itself that the row depends on.
// ---------------------------------------------------------------------------

/// Row 6.11, and the CI gate. Every code in the registry has a template in every locale, and no
/// locale carries a template for a code the registry does not know.
///
/// Both directions matter. A missing template is a user who gets a bare code; an extra one is a
/// translation of something that no longer exists, which is how a translator's time gets wasted and
/// how a stale claim survives a rename.
#[test]
fn every_warning_code_has_all_locale_templates() {
    for locale in Locale::ALL {
        let table = table(locale);
        assert!(
            !table.is_empty(),
            "{}: the template file did not parse",
            locale.as_str()
        );

        for spec in CODES {
            assert!(
                table.contains_key(spec.code),
                "{}: no template for {}",
                locale.as_str(),
                spec.code
            );
        }
        for code in table.keys() {
            assert!(
                spec(code).is_some(),
                "{}: a template for {code}, which the registry does not know",
                locale.as_str()
            );
        }
    }
}

/// Every slot a template names is an argument the code declares. A translation writing `{page}`
/// where the code carries `{pages}` would otherwise render a literal brace into a user's report.
#[test]
fn every_template_slot_is_an_argument_the_code_declares() {
    for locale in Locale::ALL {
        for spec in CODES {
            let template = template(locale, spec.code).unwrap_or_default();
            for slot in slots(template) {
                assert!(
                    spec.args.contains(&slot.as_str()),
                    "{}: {} names {{{slot}}}, which it does not carry — it has {:?}",
                    locale.as_str(),
                    spec.code,
                    spec.args
                );
            }
        }
    }
}

/// And the other direction, for English only: an argument the raising site sets and the template
/// never mentions is a number computed and thrown away.
///
/// English only, deliberately. A translation is allowed to leave a detail out where the language
/// reads better without it; the source text is where the claim has to be complete.
#[test]
fn the_english_template_uses_every_argument_its_code_carries() {
    for spec in CODES {
        let template = template(Locale::En, spec.code).unwrap_or_default();
        let named = slots(template);
        for arg in spec.args {
            assert!(
                named.iter().any(|slot| slot == arg),
                "W_… {}: carries {arg:?} and the English template never says it",
                spec.code
            );
        }
    }
}

/// The registry is sorted and names each code once.
#[test]
fn the_registry_is_sorted_and_names_each_code_once() {
    let codes: Vec<&str> = CODES.iter().map(|spec| spec.code).collect();
    let mut sorted = codes.clone();
    sorted.sort_unstable();
    assert_eq!(codes, sorted, "the registry is not in code order");

    let mut deduped = sorted;
    deduped.dedup();
    assert_eq!(deduped.len(), codes.len(), "a code is listed twice");
}

/// Filling substitutes what it has and leaves what it has not, visibly.
#[test]
fn a_slot_with_no_argument_stays_visible() {
    let args: BTreeMap<String, String> = [("retention".to_owned(), "0.950".to_owned())]
        .into_iter()
        .collect();
    let filled = render(Locale::En, "W_LOW_RETENTION", &args).expect("English has the template");

    assert!(filled.contains("0.950"), "{filled}");
    assert!(
        filled.contains("{floor}"),
        "the missing argument is visible rather than an empty space: {filled}"
    );
}

/// Every locale renders the same code, and none of them renders the code itself.
#[test]
fn every_locale_renders_a_warning_rather_than_its_code() {
    let args: BTreeMap<String, String> = [
        ("retention".to_owned(), "0.950".to_owned()),
        ("floor".to_owned(), "0.980".to_owned()),
    ]
    .into_iter()
    .collect();

    for locale in Locale::ALL {
        let filled =
            render(locale, "W_LOW_RETENTION", &args).unwrap_or_else(|| panic!("{:?}", locale));
        assert!(
            filled.contains("0.950") && filled.contains("0.980"),
            "{filled}"
        );
        assert!(
            !filled.contains("W_LOW_RETENTION"),
            "{}: the code leaked into the text",
            locale.as_str()
        );
        assert!(
            !filled.contains('{'),
            "{}: an unfilled slot",
            locale.as_str()
        );
    }
}

/// An unknown locale tag falls back to English rather than to nothing: a job spec asking for a
/// locale v1 does not ship should get a book with English warnings, not no book.
#[test]
fn an_unknown_locale_tag_falls_back_to_english() {
    assert_eq!(Locale::from_tag("de-DE"), Locale::De);
    assert_eq!(Locale::from_tag("tr"), Locale::Tr);
    assert_eq!(Locale::from_tag("TR_tr"), Locale::Tr);
    assert_eq!(Locale::from_tag("fr"), Locale::En);
    assert_eq!(Locale::from_tag(""), Locale::En);
}
