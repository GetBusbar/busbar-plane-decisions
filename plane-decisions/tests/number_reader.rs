// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026 Busbar Inc and contributors

//! THE NUMBER READER IS sonic-number 0.1.3's `parse_number`, AND IT READS EVERY NUMBER AS THE PORT
//! IT REPLACED DID. The plane carried a port of sonic-number 0.1.2 (`src/sonic_number`). Before the
//! port was deleted, its verdict on each number of the corpus (`number_corpus`) was frozen in
//! `tests/fixtures/number_verdicts.txt`. The verdict is the integer, the double by its bits, or the
//! refusal. Here the crate the plane now calls reproduces every row.

mod number_corpus;

use number_corpus::{line, rows, Verdict};

/// The verdict of the reader the plane calls, over one number's bytes, read as the plane reads it.
fn verdict(number: &str) -> Verdict {
    let bytes = number.as_bytes();
    let negative = bytes.first() == Some(&b'-');
    let mut at = usize::from(negative);
    match sonic_number::parse_number(bytes, &mut at, negative) {
        Ok(sonic_number::ParserNumber::Unsigned(n)) => Verdict::Unsigned(n),
        Ok(sonic_number::ParserNumber::Signed(n)) => Verdict::Signed(n),
        Ok(sonic_number::ParserNumber::Float(f)) => Verdict::Float(f.to_bits()),
        Err(_) => Verdict::Refused,
    }
}

#[test]
fn the_reader_reproduces_every_frozen_verdict_of_the_port() {
    let frozen = include_str!("fixtures/number_verdicts.txt");
    let frozen: Vec<&str> = frozen
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    let rows = rows();
    assert_eq!(frozen.len(), rows.len(), "one fixture line per corpus row");
    let mut numbers = 0;
    for ((label, row), want) in rows.iter().zip(&frozen) {
        let got: Vec<Verdict> = row.iter().map(|n| verdict(n)).collect();
        numbers += row.len();
        if line(label, &got) != *want {
            let want: Vec<&str> = want
                .split('\t')
                .nth(1)
                .unwrap_or_default()
                .split(' ')
                .collect();
            for ((number, got), want) in row.iter().zip(&got).zip(&want) {
                assert_eq!(
                    got.to_string(),
                    *want,
                    "{label}: {}",
                    number.get(..60).unwrap_or(number)
                );
            }
            panic!("{label}: the row's shape differs from the fixture's");
        }
    }
    assert_eq!(numbers, 1085, "the whole corpus is read");
}
