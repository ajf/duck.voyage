/// Substring blocklist for generated identifiers. Random base36 draws can
/// and do spell real words (a flock prefix `ASS` happened in the field on
/// day one), so prefix draws and code minting skip anything matching.
/// Over-blocking is harmless — a skipped candidate just means another draw
/// or the next sequence number — so err on the side of adding entries.
pub struct Profanity;

impl Profanity {
    /// Uppercase substrings. Three-letter entries catch bad flock prefixes
    /// exactly; longer entries catch words appearing anywhere in a 7-char
    /// code. Includes a few digit-for-letter spellings.
    const BLOCKED: &'static [&'static str] = &[
        "ANAL", "ANUS", "ARSE", "ASS", "A55", "BOOB", "CNT", "COC", "COK", "CRAP", "CUM",
        "CUNT", "DAMN", "DCK", "DICK", "DIK", "DYKE", "FAG", "FCK", "FUC", "FUK", "FUX",
        "GAY", "GOOK", "HELL", "HOMO", "JIZ", "KKK", "KYKE", "MILF", "NAZI", "NIG", "PEDO",
        "PISS", "PIS", "PORN", "PUSY", "RAPE", "SEX", "5EX", "SHIT", "SLUT", "SPIC", "TIT",
        "TURD", "TWAT", "TWT", "VAG", "WANK", "WHOR",
    ];

    /// True when the (base36) text contains any blocked substring, in any
    /// case. Generation sites skip such candidates.
    pub fn matches(text: &str) -> bool {
        let upper = text.to_ascii_uppercase();
        Self::BLOCKED.iter().any(|word| upper.contains(word))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catches_the_field_report_and_friends() {
        ["ASS", "ass", "QK7ASSX", "FUK", "0SEX111", "PA55", "SH1T"]
            .iter()
            .zip([true, true, true, true, true, true, false])
            .for_each(|(text, expected)| {
                assert_eq!(Profanity::matches(text), expected, "{text}");
            });
    }

    #[test]
    fn leaves_ordinary_codes_alone() {
        ["QK7", "PGN", "XFR", "QK7XFRZ", "00A6ESV", "ZZZF5L5"]
            .iter()
            .for_each(|text| assert!(!Profanity::matches(text), "{text}"));
    }
}
