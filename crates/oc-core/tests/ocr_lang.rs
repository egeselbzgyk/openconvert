//! Row 13.9: which Tesseract language data a page is read with (PHASE 13 detail 5).

use std::collections::BTreeSet;

use oc_core::ocr::lang::{
    install_hint, select_explicit, select_langs, stack_second, LangSpec, Os, W_OCR_LANG_MISSING,
};
use oc_core::thresholds::T;
use oc_model::lang::LangTag;

fn installed(codes: &[&str]) -> BTreeSet<String> {
    codes.iter().map(|code| (*code).to_owned()).collect()
}

/// Row 13.9, as a table: `de→deu`, `tr→tur`, `en→eng`, anything else and no verdict at all → `eng`;
/// a language whose traineddata is missing falls back to `eng` with a named warning, never silently.
#[test]
fn language_selection_maps_and_falls_back() {
    let all = installed(&["deu", "eng", "osd", "tur"]);
    let table: [(Option<LangTag>, &str); 7] = [
        (Some(LangTag::DE), "deu"),
        (Some(LangTag::TR), "tur"),
        (Some(LangTag::EN), "eng"),
        (Some(LangTag::new("de-AT")), "deu"),
        (Some(LangTag::new("fr")), "eng"),
        (Some(LangTag::UND), "eng"),
        (None, "eng"),
    ];
    for (tag, want) in table {
        let (spec, warnings) = select_langs(tag.as_ref(), &all);
        assert_eq!(spec.arg(), want, "{tag:?}");
        assert!(warnings.is_empty(), "{tag:?}: {warnings:?}");
    }

    // German asked for, only English installed: English, and a warning that names the pack and
    // says how to get it.
    let english_only = installed(&["eng", "osd"]);
    let (spec, warnings) = select_langs(Some(&LangTag::DE), &english_only);
    assert_eq!(spec.arg(), "eng");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    let warning = &warnings[0];
    assert_eq!(warning.code, W_OCR_LANG_MISSING);
    assert_eq!(warning.args.get("lang").map(String::as_str), Some("deu"));
    assert!(
        warning
            .args
            .get("hint")
            .is_some_and(|hint| hint.contains("deu") || hint.contains("German")),
        "{warning:?}"
    );

    // An explicit `--ocr-lang deu+tur` keeps what is installed and says what is not.
    let spec = LangSpec::parse("deu+tur").expect("a spec");
    let (kept, warnings) = select_explicit(&spec, &installed(&["deu", "eng"]));
    assert_eq!(kept.arg(), "deu");
    assert_eq!(warnings.len(), 1);
    assert_eq!(
        warnings[0].args.get("lang").map(String::as_str),
        Some("tur")
    );
    // Nothing of it installed: English, and one warning per missing pack.
    let (kept, warnings) = select_explicit(&spec, &english_only);
    assert_eq!(kept.arg(), "eng");
    assert_eq!(warnings.len(), 2);

    // A spec is a `+`-joined list of traineddata names and nothing else: no shell, no paths.
    assert!(LangSpec::parse("").is_none());
    assert!(LangSpec::parse("deu+").is_none());
    assert!(LangSpec::parse("../deu").is_none());
    assert!(LangSpec::parse("deu; rm -rf /").is_none());
    assert_eq!(
        LangSpec::parse("deu+eng").map(|s| s.arg()),
        Some("deu+eng".to_owned())
    );

    // Stacking needs a second language on at least `ocr.second_lang_block_share` of the blocks.
    let share = T.ocr.second_lang_block_share;
    let mostly_german: Vec<LangTag> = std::iter::repeat_n(LangTag::DE, 9)
        .chain(std::iter::once(LangTag::EN))
        .collect();
    let (primary, _) = select_langs(Some(&LangTag::DE), &all);
    assert_eq!(
        stack_second(primary.clone(), &mostly_german, &all, share).arg(),
        "deu",
        "one English block in ten is not a second language"
    );
    let bilingual: Vec<LangTag> = std::iter::repeat_n(LangTag::DE, 6)
        .chain(std::iter::repeat_n(LangTag::TR, 4))
        .collect();
    assert_eq!(
        stack_second(primary.clone(), &bilingual, &all, share).arg(),
        "deu+tur"
    );
    assert_eq!(
        stack_second(primary, &bilingual, &installed(&["deu", "eng"]), share).arg(),
        "deu",
        "a second language that is not installed is not stacked"
    );

    // The hint is a command the reader can paste, per platform.
    assert!(install_hint(Os::Linux, "deu").contains("apt install tesseract-ocr-deu"));
    assert!(install_hint(Os::MacOs, "deu").contains("brew install tesseract-lang"));
    // Every hint names the pack it asks for, on every OS: the fallback warning is read on the
    // machine it was raised on, and "install the languages" alone does not say which one is missing.
    for os in [Os::Linux, Os::MacOs, Os::Windows] {
        assert!(install_hint(os, "deu").contains("deu"), "{os:?}");
    }
    assert!(install_hint(Os::Windows, "deu").contains("UB-Mannheim"));
}
