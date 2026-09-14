//! How alike two strings are.
//!
//! One measure, and it has two consumers with the same need: `furniture` asks whether the
//! head on page 4 is the head on page 5 despite a different folio in it, and `structure` asks
//! whether an outline entry names a heading candidate despite a numbering prefix the producer
//! wrote into one and not the other. Both want "nearly the same", tolerant of length.

/// Levenshtein distance over the longer string's length, so a one-character difference in a
/// four-character head is not the same as one in a forty-character head.
pub fn normalised_edit_distance(a: &str, b: &str) -> f32 {
    if a == b {
        return 0.0;
    }
    let left: Vec<char> = a.chars().collect();
    let right: Vec<char> = b.chars().collect();
    let longest = left.len().max(right.len());
    if longest == 0 {
        return 0.0;
    }
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0usize; right.len() + 1];
    for (i, lc) in left.iter().enumerate() {
        current[0] = i + 1;
        for (j, rc) in right.iter().enumerate() {
            let cost = usize::from(lc != rc);
            current[j + 1] = (previous[j] + cost)
                .min(previous[j + 1] + 1)
                .min(current[j] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()] as f32 / longest as f32
}

#[test]
fn distance_is_zero_for_equal_strings_and_scaled_by_the_longer() {
    assert_eq!(normalised_edit_distance("abc", "abc"), 0.0);
    assert_eq!(normalised_edit_distance("", ""), 0.0);
    // One edit in three characters is a third; one in forty is a fortieth. That scaling is
    // the whole point of normalising by the longer string rather than by the shorter.
    assert!((normalised_edit_distance("abc", "abd") - 1.0 / 3.0).abs() < 0.001);
    assert!(normalised_edit_distance("chapterone", "chaptertwo") > 0.15);
    assert!(normalised_edit_distance("thetestbook", "thetestbok") < 0.15);
}
