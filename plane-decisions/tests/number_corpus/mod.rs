// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026 Busbar Inc and contributors

//! The number corpus the reader's verdicts are frozen over (`tests/fixtures/number_verdicts.txt`):
//! 1,085 JSON numbers at the edges a number reader parts on. They cover the double's range
//! (`1e308`, `1e309`, the largest and the smallest doubles and their neighbours), the `u64` and
//! `i64` limits, mantissas past 19 significant digits, exponents past 308 and past 1000 (the
//! reader's exponent cap), `-0`, and fractions and integers hundreds or thousands of digits long.
//! Every number is held to the JSON grammar, as the plane only reads a number the grammar admitted.

/// The mantissas of the grid: a digit, the `u64` and `i64` limits and past them, a mantissa past
/// 19 digits, and the doubles at the edges of the range.
pub const MANTISSAS: [&str; 19] = [
    "0",
    "1",
    "9",
    "17",
    "123456789",
    "18446744073709551615",
    "18446744073709551616",
    "9223372036854775807",
    "9223372036854775808",
    "12345678901234567890123",
    "1.5",
    "0.1",
    "1.7976931348623157",
    "1.7976931348623158",
    "1.7976931348623159",
    "4.9406564584124654",
    "2.4703282292062328",
    "2.2250738585072014",
    "0.30000000000000004",
];

/// The exponents of the grid, none among them: at and past the double's range, at and past the
/// reader's exponent cap (1000), and one past `i32`.
pub const EXPONENTS: [&str; 27] = [
    "",
    "e0",
    "e1",
    "e-1",
    "e19",
    "e22",
    "e23",
    "e-307",
    "e-308",
    "e-323",
    "e-324",
    "e-325",
    "e306",
    "e307",
    "e308",
    "e309",
    "e400",
    "e999",
    "e1000",
    "e1001",
    "e-1000",
    "e-1001",
    "e10000",
    "e-10000",
    "e99999999999999999999",
    "E+5",
    "e+308",
];

/// The digit counts of the long numbers.
pub const LONG: [usize; 8] = [100, 290, 308, 309, 400, 700, 800, 2000];

/// The corpus, in rows: each row's label and its numbers, in the fixture's order. A grid row is
/// one mantissa under every exponent, unsigned then negative; a long row is the seven long forms
/// of one digit count; the last row holds the cases the exponent cap decides.
#[must_use]
pub fn rows() -> Vec<(String, Vec<String>)> {
    let zeros = |n: usize| "0".repeat(n);
    let mut rows = Vec::new();
    for m in MANTISSAS {
        let numbers = EXPONENTS
            .iter()
            .flat_map(|e| ["", "-"].map(|s| format!("{s}{m}{e}")))
            .collect();
        rows.push((format!("grid {m}"), numbers));
    }
    for n in LONG {
        rows.push((
            format!("long {n}"),
            vec![
                format!("1{}", zeros(n)),
                format!("0.{}1", zeros(n)),
                format!("0.{}1e10000", zeros(n)),
                format!("0.{}1e1000", zeros(n)),
                format!("1{}e-1000", zeros(n)),
                format!("9.{}", "9".repeat(n)),
                format!("1{}e-14800", zeros(n * 7)),
            ],
        ));
    }
    rows.push((
        "capped".to_string(),
        vec![
            format!("0.{}1e10000", zeros(690)),
            format!("0.{}1e10000", zeros(691)),
            format!("0.{}1e12300", zeros(2000)),
        ],
    ));
    rows
}

/// What the reader made of one number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// A non-negative integer.
    Unsigned(u64),
    /// A negative integer.
    Signed(i64),
    /// A finite double, by its bits.
    Float(u64),
    /// Refused: not finite (`FloatMustBeFinite`) or no number.
    Refused,
}

impl core::fmt::Display for Verdict {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Unsigned(n) => write!(f, "u{n}"),
            Self::Signed(n) => write!(f, "i{n}"),
            Self::Float(bits) => write!(f, "f{bits:016x}"),
            Self::Refused => f.write_str("refused"),
        }
    }
}

/// The fixture's line for one row: its label, a tab, and each number's verdict, space-separated.
#[must_use]
pub fn line(label: &str, verdicts: &[Verdict]) -> String {
    let verdicts: Vec<String> = verdicts.iter().map(ToString::to_string).collect();
    format!("{label}\t{}", verdicts.join(" "))
}
