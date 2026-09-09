//! `cargo xtask thresholds-lint` — D17's provenance rule, run as a CI gate.
//!
//! The rule itself lives in `oc_core::thresholds::lint`, not here. One implementation, two
//! callers: this task and the test that asserts the committed file passes. A rule with two
//! implementations is a rule with two behaviours.

use std::path::Path;

use anyhow::{bail, Context, Result};

pub fn run(workspace_root: &Path) -> Result<()> {
    let path = workspace_root.join("thresholds.toml");
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read {}", path.display()))?;

    // UTC, deliberately: a threshold does not expire in one timezone and not another, and a
    // CI runner's local time is not a thing anyone should have to reason about.
    let today = today_utc();
    let findings = oc_core::thresholds::lint(&text, &today)
        .with_context(|| format!("{} is not valid TOML", path.display()))?;

    if findings.is_empty() {
        println!("thresholds-lint: clean ({today})");
        return Ok(());
    }
    for finding in &findings {
        eprintln!("thresholds.toml: {} — {}", finding.key, finding.problem);
    }
    bail!(
        "thresholds-lint found {} entry/entries failing D17 (today is {today})",
        findings.len()
    )
}

/// Today's date as `YYYY-MM-DD`, UTC.
///
/// Computed from the system clock rather than pulled in with a date crate: `oc-core` is
/// deliberately clock-free — `lint` takes the date as an argument — and this is the only
/// place in the shipped-adjacent tooling that needs one. The civil-from-days conversion is
/// Howard Hinnant's, whose constants are calendar facts rather than tunable numbers.
fn today_utc() -> String {
    const SECONDS_PER_DAY: u64 = 86_400;
    // Days from 0000-03-01 to 1970-01-01, the shift that makes leap years fall at the end
    // of the cycle and the arithmetic branch-free.
    const EPOCH_SHIFT_DAYS: i64 = 719_468;
    const DAYS_PER_400_YEARS: i64 = 146_097;
    const DAYS_PER_100_YEARS: i64 = 36_524;
    const DAYS_PER_4_YEARS: i64 = 1_461;

    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    let days = (seconds / SECONDS_PER_DAY) as i64 + EPOCH_SHIFT_DAYS;

    let era = days.div_euclid(DAYS_PER_400_YEARS);
    let day_of_era = days.rem_euclid(DAYS_PER_400_YEARS);
    let year_of_era = (day_of_era - day_of_era / (DAYS_PER_4_YEARS - 1)
        + day_of_era / DAYS_PER_100_YEARS
        - day_of_era / (DAYS_PER_400_YEARS - 1))
        / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    // Months are numbered from March, so that February's variable length lands last.
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
    let month = if month_from_march < 10 {
        month_from_march + 3
    } else {
        month_from_march - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);

    format!("{year:04}-{month:02}-{day:02}")
}

/// Today, as the task computes it. Public so a test can assert the task reports it.
pub fn today_for_test() -> String {
    today_utc()
}
