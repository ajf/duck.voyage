//! Codec property tests required by the design doc (§14): round-trip,
//! distinctness, tweak independence, Damm detection, case-insensitive parsing.

use std::collections::HashSet;

use domain::{Base36, Damm36, DuckCode, DuckCodec, FlockCode, FlockSeq, KeyGeneration};
use proptest::prelude::*;

fn codec() -> DuckCodec {
    DuckCodec::new(
        [
            (KeyGeneration::new(0), [0x42u8; 32]),
            (KeyGeneration::new(1), [0xA7u8; 32]),
        ],
        KeyGeneration::new(1),
    )
    .expect("valid test codec")
}

#[test]
fn round_trip_exhaustive_over_a_full_flock() {
    let codec = codec();
    [
        (KeyGeneration::new(0), FlockCode::parse("QK7").unwrap()),
        (KeyGeneration::new(1), FlockCode::parse("00A").unwrap()),
    ]
    .iter()
    .for_each(|(generation, flock)| {
        (1..=u32::from(FlockSeq::MAX)).for_each(|raw| {
            let seq = FlockSeq::new(raw).unwrap();
            let code = codec.encode(*generation, flock, seq).unwrap();
            assert_eq!(codec.decode(*generation, &code).unwrap(), seq, "seq {raw}");
        });
    });
}

#[test]
fn full_flock_yields_distinct_valid_codes() {
    let codec = codec();
    let flock = FlockCode::parse("ZZZ").unwrap();
    let codes: HashSet<String> = (1..=u32::from(FlockSeq::MAX))
        .map(|raw| {
            let code = codec
                .encode(KeyGeneration::new(0), &flock, FlockSeq::new(raw).unwrap())
                .unwrap();
            // Every minted code re-parses (charset + checksum hold).
            assert_eq!(DuckCode::parse(code.as_str()).unwrap(), code);
            code.as_str().to_owned()
        })
        .collect();
    assert_eq!(codes.len(), usize::from(FlockSeq::MAX));
}

#[test]
fn tweak_gives_flocks_independent_permutations() {
    let codec = codec();
    let (a, b) = (
        FlockCode::parse("AAA").unwrap(),
        FlockCode::parse("AAB").unwrap(),
    );
    let differing = (1..=100u32)
        .filter(|&raw| {
            let seq = FlockSeq::new(raw).unwrap();
            let generation = KeyGeneration::new(0);
            codec.encode(generation, &a, seq).unwrap().payload_digits()
                != codec.encode(generation, &b, seq).unwrap().payload_digits()
        })
        .count();
    // Same key, same seqs: only the tweak separates the mappings. Two
    // independent permutations agree on a given point with p ≈ 1/46656.
    assert!(differing >= 95, "only {differing}/100 payloads differ");
}

#[test]
fn generations_are_independent_and_unknown_generation_errors() {
    let codec = codec();
    let flock = FlockCode::parse("G3N").unwrap();
    let seq = FlockSeq::new(4242).unwrap();
    let gen0 = codec.encode(KeyGeneration::new(0), &flock, seq).unwrap();
    let gen1 = codec.encode(KeyGeneration::new(1), &flock, seq).unwrap();
    assert_ne!(gen0.payload_digits(), gen1.payload_digits());
    assert!(codec.decode(KeyGeneration::new(9), &gen0).is_err());
}

#[test]
fn out_of_range_payloads_are_rejected_on_decode() {
    let codec = codec();
    let flock = FlockCode::parse("QK7").unwrap();
    // Hand-build well-formed codes (valid charset + check char) whose payload
    // decrypts beyond the 10k flock ceiling; most of the 46,656-payload space
    // qualifies, so scanning a few hundred candidates must hit both outcomes.
    let outcomes: HashSet<bool> = (0u32..300)
        .map(|payload_value| {
            let digits: Vec<u8> = flock
                .digits()
                .into_iter()
                .chain((0..3).rev().map(|shift| (payload_value / 36u32.pow(shift) % 36) as u8))
                .collect();
            let check = Damm36::check_digit(digits.iter().copied());
            let text: String = digits.into_iter().chain([check]).map(Base36::char_of).collect();
            let code = DuckCode::parse(&text).expect("well-formed by construction");
            codec.decode(KeyGeneration::new(0), &code).is_ok()
        })
        .collect();
    assert!(outcomes.contains(&false), "no out-of-range payload seen");
}

proptest! {
    /// Any minted code survives arbitrary re-casing and separator placement.
    #[test]
    fn parse_is_case_and_separator_insensitive(
        seq in 1u32..=10_000,
        flock_seed in 0u32..46_656,
        lowercase_mask in any::<u8>(),
        dash_pos in 0usize..8,
    ) {
        let flock_text: String = (0..3).rev()
            .map(|shift| Base36::char_of((flock_seed / 36u32.pow(shift) % 36) as u8))
            .collect();
        let flock = FlockCode::parse(&flock_text).unwrap();
        let code = codec()
            .encode(KeyGeneration::new(0), &flock, FlockSeq::new(seq).unwrap())
            .unwrap();
        let mangled: String = code.as_str().chars().enumerate()
            .flat_map(|(i, c)| {
                let c = if lowercase_mask & (1 << (i % 8)) != 0 { c.to_ascii_lowercase() } else { c };
                (i == dash_pos).then_some('-').into_iter().chain([c])
            })
            .collect();
        prop_assert_eq!(DuckCode::parse(&mangled).unwrap(), code);
    }

    /// Every single-character mutation and adjacent transposition of a valid
    /// code fails to parse.
    #[test]
    fn parse_rejects_mutations_and_transpositions(
        seq in 1u32..=10_000,
        flock_seed in 0u32..46_656,
    ) {
        let flock_text: String = (0..3).rev()
            .map(|shift| Base36::char_of((flock_seed / 36u32.pow(shift) % 36) as u8))
            .collect();
        let flock = FlockCode::parse(&flock_text).unwrap();
        let code = codec()
            .encode(KeyGeneration::new(0), &flock, FlockSeq::new(seq).unwrap())
            .unwrap();
        let chars: Vec<char> = code.as_str().chars().collect();

        for (i, wrong) in (0..chars.len()).flat_map(|i| (0..36u8).map(move |d| (i, Base36::char_of(d)))) {
            if chars[i] != wrong {
                let mut bad = chars.clone();
                bad[i] = wrong;
                prop_assert!(DuckCode::parse(&bad.iter().collect::<String>()).is_err());
            }
        }
        for i in 0..chars.len() - 1 {
            if chars[i] != chars[i + 1] {
                let mut bad = chars.clone();
                bad.swap(i, i + 1);
                prop_assert!(DuckCode::parse(&bad.iter().collect::<String>()).is_err());
            }
        }
    }
}
