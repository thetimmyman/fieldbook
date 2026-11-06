//! Deterministic BM25F scoring and integer impact calculation (`docs/reference/search.md#ranking`).
//!
//! Uses only IEEE-754 addition, subtraction, multiplication and division on `f64`, with a
//! hand-written natural logarithm (`ln`) summing 15 odd-power terms (`1, 3, ..., 29`) and
//! `std::f64::consts::LN_2`.

use super::units::{Unit, UnitFields};

/// BM25F term-frequency saturation parameter (`k1 = 1.2`).
pub const K1: f64 = 1.2;

/// BM25F field-length normalization parameter (`b = 0.75`).
pub const B: f64 = 0.75;

/// Fixed BM25F field weights in the order `[id, title, headings, claim, body]`.
pub const FIELD_WEIGHTS: [f64; 5] = [6.0, 4.0, 2.0, 2.0, 1.0];

/// Deterministic natural logarithm for `x >= 1.0` (`docs/reference/search.md#ranking`).
///
/// Divides `x` by `2` until it is below `2`, counting divisions as `e`; lets `m` be the result
/// and `z = (m - 1) / (m + 1)`; then computes
/// `e * LN_2 + 2 * (z + z^3/3 + z^5/5 + ... + z^29/29)` summing exponents `1, 3, 5, ..., 29` in
/// that order (15 terms) using only `+`, `-`, `*`, and `/` on `f64`.
#[must_use]
pub fn ln(x: f64) -> f64 {
    debug_assert!(x >= 1.0, "bm25f::ln requires x >= 1.0, got {x}");
    if x <= 1.0 {
        return 0.0;
    }
    let mut m = x;
    let mut e: u32 = 0;
    while m >= 2.0 {
        m /= 2.0;
        e += 1;
    }
    let z = (m - 1.0) / (m + 1.0);
    let z2 = z * z;
    let mut power = z;
    let mut series = 0.0_f64;
    let mut exp: u32 = 1;
    while exp <= 29 {
        series += power / f64::from(exp);
        power *= z2;
        exp += 2;
    }
    f64::from(e) * std::f64::consts::LN_2 + 2.0 * series
}

/// Computes `idf(t) = ln(1 + (N - n_t + 0.5) / (n_t + 0.5))` for `num_units = N` and
/// `doc_freq = n_t`.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "unit and document-frequency counts fit well within f64's 53-bit mantissa"
)]
pub fn idf(num_units: usize, doc_freq: usize) -> f64 {
    if num_units == 0 || doc_freq == 0 || doc_freq > num_units {
        return 0.0;
    }
    let n = num_units as f64;
    let nt = doc_freq as f64;
    ln(1.0 + (n - nt + 0.5) / (nt + 0.5))
}

/// Computes `avgl_f` for each of the five fields `[id, title, headings, claim, body]` across
/// `units`.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "token counts and unit counts fit well within f64's 53-bit mantissa"
)]
pub fn average_field_lengths(units: &[Unit]) -> [f64; 5] {
    if units.is_empty() {
        return [0.0; 5];
    }
    let mut totals = [0usize; 5];
    for unit in units {
        for (f_idx, tokens) in unit.fields.as_array().into_iter().enumerate() {
            totals[f_idx] += tokens.len();
        }
    }
    let n = units.len() as f64;
    [
        (totals[0] as f64) / n,
        (totals[1] as f64) / n,
        (totals[2] as f64) / n,
        (totals[3] as f64) / n,
        (totals[4] as f64) / n,
    ]
}

/// Computes `tf'(d, t)` by summing over `[id, title, headings, claim, body]` in that fixed order,
/// skipping any field whose `avgl_f == 0.0`.
#[must_use]
pub fn tf_prime(fields: &UnitFields, avgl: &[f64; 5], token: &str) -> f64 {
    let mut counts = [0usize; 5];
    let mut lens = [0usize; 5];
    for (f_idx, field_tokens) in fields.as_array().into_iter().enumerate() {
        lens[f_idx] = field_tokens.len();
        counts[f_idx] = field_tokens.iter().filter(|t| t.as_str() == token).count();
    }
    tf_prime_from_counts(&counts, &lens, avgl)
}

/// Computes `tf'(d, t)` from the per-field term counts and field lengths of one unit, in the fixed
/// order `[id, title, headings, claim, body]`, with the expression order of
/// `docs/reference/search.md#ranking`: `weight * tf / (1 - b + b * length / avgl)`.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "field token lengths and term frequencies fit well within f64's 53-bit mantissa"
)]
pub fn tf_prime_from_counts(counts: &[usize; 5], lens: &[usize; 5], avgl: &[f64; 5]) -> f64 {
    let mut sum = 0.0_f64;
    for f_idx in 0..5 {
        let avg = avgl[f_idx];
        if avg == 0.0 || counts[f_idx] == 0 {
            continue;
        }
        let denom = 1.0 - B + B * (lens[f_idx] as f64) / avg;
        sum += FIELD_WEIGHTS[f_idx] * (counts[f_idx] as f64) / denom;
    }
    sum
}

/// Computes `score(d, t) = idf(t) * tf'(d, t) * (k1 + 1) / (tf'(d, t) + k1)`.
#[must_use]
pub fn score(idf_val: f64, tf_prime_val: f64) -> f64 {
    if tf_prime_val <= 0.0 {
        return 0.0;
    }
    idf_val * tf_prime_val * (K1 + 1.0) / (tf_prime_val + K1)
}

/// Computes `impact(d, t) = max(1, round(1000 * score(d, t)))` with round half away from zero,
/// using only `+`, `-`, `*`, and `/` on `f64`.
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "score is non-negative and 1000 * score fits well within u32"
)]
pub fn impact(idf_val: f64, tf_prime_val: f64) -> u32 {
    let s = score(idf_val, tf_prime_val);
    let rounded = ((1000.0 * s) + 0.5) as u32;
    rounded.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::lexer::index_tokens;

    #[test]
    #[expect(
        clippy::approx_constant,
        reason = "the table holds the literal reference values of docs/reference/search.md"
    )]
    fn ln_matches_the_table_within_1e_12() {
        let table = [
            (2.0_f64, 0.693_147_180_559_945_3_f64),
            (3.0_f64, 1.098_612_288_668_109_8_f64),
            (10.0_f64, 2.302_585_092_994_046_f64),
        ];
        for (x, expected) in table {
            let actual = ln(x);
            let diff = if actual >= expected {
                actual - expected
            } else {
                expected - actual
            };
            assert!(
                diff <= 1e-12,
                "ln({x}) = {actual:.17}, expected {expected:.17}, diff = {diff:e}"
            );
        }
    }

    #[test]
    fn worked_example_impacts_match_the_reference() {
        let units = vec![
            Unit::new(
                "CL-CMP-0011",
                "claim",
                UnitFields {
                    id: index_tokens("CL-CMP-0011"),
                    title: Vec::new(),
                    headings: Vec::new(),
                    claim: index_tokens(
                        "The contracting officer may withhold 5 percent of payments",
                    ),
                    body: Vec::new(),
                },
            ),
            Unit::new(
                "cmp-withholding",
                "page",
                UnitFields {
                    id: index_tokens("cmp-withholding"),
                    title: index_tokens("Payment withholding"),
                    headings: Vec::new(),
                    claim: Vec::new(),
                    body: index_tokens("Withholding applies to business system deficiencies"),
                },
            ),
            Unit::new(
                "cmp-withholding#the-rule",
                "page",
                UnitFields {
                    id: Vec::new(),
                    title: Vec::new(),
                    headings: index_tokens("the rule"),
                    claim: Vec::new(),
                    body: index_tokens(
                        "Withholding is permitted when a significant deficiency exists",
                    ),
                },
            ),
        ];

        let avgl = average_field_lengths(&units);
        let idf_2 = idf(3, 2);
        let idf_1 = idf(3, 1);

        // withholding: n_t = 2 -> unit 1 = 858, unit 2 = 364
        assert_eq!(
            impact(idf_2, tf_prime(&units[1].fields, &avgl, "withholding")),
            858
        );
        assert_eq!(
            impact(idf_2, tf_prime(&units[2].fields, &avgl, "withholding")),
            364
        );

        // withhold: n_t = 1 -> unit 0 = 863
        assert_eq!(
            impact(idf_1, tf_prime(&units[0].fields, &avgl, "withhold")),
            863
        );

        // the: n_t = 2 -> unit 0 = 414, unit 2 = 414
        assert_eq!(impact(idf_2, tf_prime(&units[0].fields, &avgl, "the")), 414);
        assert_eq!(impact(idf_2, tf_prime(&units[2].fields, &avgl, "the")), 414);

        // rule: n_t = 1 -> unit 2 = 863
        assert_eq!(
            impact(idf_1, tf_prime(&units[2].fields, &avgl, "rule")),
            863
        );

        // payment: n_t = 1 -> unit 1 = 1233
        assert_eq!(
            impact(idf_1, tf_prime(&units[1].fields, &avgl, "payment")),
            1233
        );

        // cmp: n_t = 1 -> unit 1 = 1598
        assert_eq!(
            impact(idf_1, tf_prime(&units[1].fields, &avgl, "cmp")),
            1598
        );
    }

    #[test]
    fn impact_is_never_below_one_however_small_the_score() {
        // With tf' = 1 the score equals the idf. These rounds are to 0 before the minimum, and a
        // posting of 0 would make the unit vanish from every query that holds the term.
        assert_eq!(impact(0.0001, 1.0), 1);
        assert_eq!(impact(0.0004, 1.0), 1);
        assert_eq!(impact(0.0, 1.0), 1);
        assert_eq!(impact(0.5, 0.0), 1);
        // Above the minimum the value is the rounded 1000 * score, not 1.
        assert_eq!(impact(0.0016, 1.0), 2);
        assert_eq!(impact(0.0024, 1.0), 2);
        assert_eq!(impact(0.0026, 1.0), 3);
        assert_eq!(impact(1.0, 1.0), 1000);
    }
}
