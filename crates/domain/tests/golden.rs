//! Frozen codec vectors. Once labels print, the exact (key, flock, seq) →
//! code mapping must never change — not across crate upgrades, not across
//! refactors. If this test fails, the change breaks every printed duck.

use domain::{DuckCodec, FlockCode, FlockSeq, KeyGeneration};

#[test]
fn codec_output_is_frozen() {
    let codec = DuckCodec::new([(KeyGeneration::new(0), [0x42u8; 32])], KeyGeneration::new(0))
        .expect("valid codec");
    let vectors: Vec<(&str, u32, String)> = [("QK7", 1), ("QK7", 2), ("QK7", 100), ("QK7", 10_000), ("00A", 1), ("ZZZ", 4_242)]
        .into_iter()
        .map(|(flock, seq)| {
            let code = codec
                .encode(
                    KeyGeneration::new(0),
                    &FlockCode::parse(flock).unwrap(),
                    FlockSeq::new(seq).unwrap(),
                )
                .unwrap();
            (flock, seq, code.as_str().to_owned())
        })
        .collect();
    let expected: Vec<(&str, u32, String)> = EXPECTED
        .iter()
        .map(|&(f, s, c)| (f, s, c.to_owned()))
        .collect();
    assert_eq!(vectors, expected);
}

/// Recorded 2026-07-22 from the first green run under fpe 0.5.1. Append-only.
const EXPECTED: [(&str, u32, &str); 6] = [
    ("QK7", 1, "QK7XFRZ"),
    ("QK7", 2, "QK7RXOT"),
    ("QK7", 100, "QK7QVM1"),
    ("QK7", 10_000, "QK7APN7"),
    ("00A", 1, "00A6ESV"),
    ("ZZZ", 4_242, "ZZZF5L5"),
];
