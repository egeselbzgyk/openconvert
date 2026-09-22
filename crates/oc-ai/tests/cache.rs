//! The content-addressed decision cache (D13.8, ARCHITECTURE §9.4): one keying rule, shared with
//! the cassettes (Appendix B.1), and one JSON file per key.

mod common;

use std::path::PathBuf;

use oc_ai::cache::{cache_key, CacheError, CachedAnswer, FileCache};
use oc_ai::digest::{hex, sha256};
use oc_ai::prompt::PROMPT_VERSION;

const MODEL: &str = "qwen3-1.7b-q4_k_m";

fn roles() -> oc_ai::provider::LlmRequest {
    common::requests().remove(1)
}

/// Test 8.10. A prompt-version bump must reach every cached decision: the new version asks a
/// different question, and an answer to the old one is not an answer to it (Appendix B.4, rule 3).
#[test]
fn cache_key_changes_with_prompt_version() {
    let request = roles();
    let grammar = request.grammar_sha256();
    let current = cache_key(MODEL, PROMPT_VERSION, &grammar, &request.user);
    let bumped = cache_key(MODEL, PROMPT_VERSION + 1, &grammar, &request.user);
    assert_ne!(current, bumped);
    assert_eq!(
        current,
        request.cache_key(MODEL),
        "the request's key is the same rule"
    );
}

/// Every component of the key is load-bearing, and the same inputs always give the same key.
#[test]
fn every_component_of_the_key_is_load_bearing() {
    let request = roles();
    let grammar = request.grammar_sha256();
    let key = cache_key(MODEL, PROMPT_VERSION, &grammar, &request.user);

    assert_eq!(
        key,
        cache_key(MODEL, PROMPT_VERSION, &grammar, &request.user)
    );
    assert_ne!(
        key,
        cache_key("qwen3-4b-q4_k_m", PROMPT_VERSION, &grammar, &request.user)
    );
    assert_ne!(
        key,
        cache_key(
            MODEL,
            PROMPT_VERSION,
            &sha256(b"another grammar"),
            &request.user
        )
    );
    assert_ne!(
        key,
        cache_key(
            MODEL,
            PROMPT_VERSION,
            &grammar,
            &format!("{} ", request.user)
        )
    );
}

/// `‖` is concatenation, and bare concatenation is ambiguous: model `ab` with input `c` and model
/// `a` with input `bc` are the same bytes. The key length-prefixes the one variable-width field
/// that is not last, so no two distinct inputs can share their bytes.
#[test]
fn the_key_cannot_confuse_where_one_field_ends() {
    let grammar = sha256(b"g");
    assert_ne!(
        cache_key("ab", 1, &grammar, "c"),
        cache_key("a", 1, &grammar, "bc")
    );
}

/// The key's byte layout, stated independently of the implementation, and the worked example's key
/// pinned. The cassettes are named by this key, so a change to its derivation orphans every one of
/// them and every cached decision — the change has to be this deliberate.
#[test]
fn the_key_is_the_documented_layout() {
    let request = roles();
    let grammar = request.grammar_sha256();

    let mut layout = Vec::new();
    layout.extend_from_slice(&u64::try_from(MODEL.len()).expect("short").to_le_bytes());
    layout.extend_from_slice(MODEL.as_bytes());
    layout.extend_from_slice(&PROMPT_VERSION.to_le_bytes());
    layout.extend_from_slice(&grammar);
    layout.extend_from_slice(request.user.as_bytes());
    assert_eq!(
        cache_key(MODEL, PROMPT_VERSION, &grammar, &request.user),
        sha256(&layout)
    );

    assert_eq!(
        hex(&request.cache_key(MODEL)),
        "f503a998a37e94b689d0700c85a8cfe0ba35a5ef16391fdf3efe511399e0f9a3"
    );
}

fn scratch(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("oc-ai-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    directory
}

fn answer(request: &oc_ai::provider::LlmRequest) -> CachedAnswer {
    CachedAnswer::new(MODEL, request, common::HEADING_ROLES_ANSWER, 412, 38)
}

/// One JSON file per key, at `<root>/<key[0..2]>/<key>.json` (ARCHITECTURE §9.4), and what comes
/// back is what went in.
#[test]
fn a_cached_answer_round_trips_through_its_file() {
    let root = scratch("roundtrip");
    let cache = FileCache::new(&root);
    let request = roles();
    let key = request.cache_key(MODEL);

    assert_eq!(cache.get(&key), Ok(None), "an empty cache misses");
    cache.put(&answer(&request)).expect("the entry is written");

    let name = hex(&key);
    assert_eq!(
        cache.path(&key),
        root.join(&name[..2]).join(format!("{name}.json"))
    );
    assert!(cache.path(&key).is_file());
    assert_eq!(cache.get(&key), Ok(Some(answer(&request))));

    let _ = std::fs::remove_dir_all(&root);
}

/// A damaged file is reported, never read as a miss: a miss would quietly spend a call the book
/// already paid for, and a damaged cache is something the user should hear about. An entry filed
/// under a key that is not its own is damage too.
#[test]
fn a_damaged_or_misfiled_entry_is_reported() {
    let root = scratch("damaged");
    let cache = FileCache::new(&root);
    let request = roles();
    let key = request.cache_key(MODEL);

    std::fs::create_dir_all(cache.path(&key).parent().expect("a parent")).expect("mkdir");
    std::fs::write(cache.path(&key), "{ not json").expect("write");
    assert!(matches!(cache.get(&key), Err(CacheError::Damaged { .. })));

    // Another question's answer, moved into this question's file.
    let verse = common::requests().remove(3);
    let other = verse.cache_key(MODEL);
    cache.put(&answer(&verse)).expect("written");
    std::fs::rename(cache.path(&other), cache.path(&key)).expect("misfile it");
    assert!(matches!(cache.get(&key), Err(CacheError::Damaged { .. })));

    let _ = std::fs::remove_dir_all(&root);
}
