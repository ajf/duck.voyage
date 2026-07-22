#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ImoError {
    #[error("IMO number must be exactly 7 digits, got {0} characters")]
    BadLength(usize),
    #[error("invalid character {0:?} in IMO number")]
    BadChar(char),
    #[error("IMO check digit mismatch")]
    BadCheckDigit,
}

/// A ship's IMO number: seven digits where the last is a check digit (the
/// first six weighted 7,6,5,4,3,2; the sum's last digit must equal digit 7).
/// The rename-stable identity anchor for vessels.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImoNumber(String);

impl ImoNumber {
    pub const LEN: usize = 7;

    /// Parse from digits, with an optional leading `IMO` tag and surrounding
    /// whitespace tolerated (`"IMO 9074729"`).
    pub fn parse(raw: &str) -> Result<Self, ImoError> {
        let trimmed = raw
            .trim()
            .trim_start_matches("IMO")
            .trim_start_matches("imo")
            .trim_start();
        let digits: Vec<u32> = trimmed
            .chars()
            .map(|c| c.to_digit(10).ok_or(ImoError::BadChar(c)))
            .collect::<Result<_, _>>()?;
        (digits.len() == Self::LEN)
            .then_some(())
            .ok_or(ImoError::BadLength(digits.len()))?;
        let weighted: u32 = digits[..6]
            .iter()
            .zip((2..=7).rev())
            .map(|(&d, w)| d * w)
            .sum();
        (weighted % 10 == digits[6])
            .then(|| Self(trimmed.to_owned()))
            .ok_or(ImoError::BadCheckDigit)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ImoNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "IMO {}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_known_valid_numbers() {
        // 9074729: 9·7+0·6+7·5+4·4+7·3+2·2 = 139 → check 9.
        // 1234567: 1·7+2·6+3·5+4·4+5·3+6·2 = 77 → check 7.
        ["9074729", "1234567", "IMO 9074729", "  9074729  "]
            .iter()
            .for_each(|raw| {
                assert!(ImoNumber::parse(raw).is_ok(), "{raw:?} should parse");
            });
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(ImoNumber::parse("9074720"), Err(ImoError::BadCheckDigit));
        assert_eq!(ImoNumber::parse("907472"), Err(ImoError::BadLength(6)));
        assert_eq!(ImoNumber::parse("90747290"), Err(ImoError::BadLength(8)));
        assert_eq!(ImoNumber::parse("907472X"), Err(ImoError::BadChar('X')));
        assert_eq!(ImoNumber::parse(""), Err(ImoError::BadLength(0)));
    }
}
