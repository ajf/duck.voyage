use crate::base36::Base36;
use crate::damm::Damm36;

/// Which FF1 key generation a flock's codes are minted under. Stored on the
/// flock row, not encoded in the duck code itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyGeneration(u16);

impl KeyGeneration {
    pub fn new(generation: u16) -> Self {
        Self(generation)
    }

    pub fn get(self) -> u16 {
        self.0
    }
}

impl std::fmt::Display for KeyGeneration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FlockSeqError {
    #[error("flock sequence {0} outside 1..={max}", max = FlockSeq::MAX)]
    OutOfRange(u32),
}

/// A duck's 1-based position within its flock — the value the code payload
/// encrypts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FlockSeq(u16);

impl FlockSeq {
    pub const MAX: u16 = 10_000;

    pub fn new(seq: u32) -> Result<Self, FlockSeqError> {
        u16::try_from(seq)
            .ok()
            .filter(|&s| (1..=Self::MAX).contains(&s))
            .map(Self)
            .ok_or(FlockSeqError::OutOfRange(seq))
    }

    pub fn get(self) -> u16 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FlockCodeError {
    #[error("flock prefix must be exactly {len} characters, got {0}", len = FlockCode::LEN)]
    BadLength(usize),
    #[error("invalid character {0:?} in flock prefix")]
    BadChar(char),
}

/// A flock's visible 3-character base36 prefix (`QK7` in `QK7-MX2P`).
/// Randomly assigned from the unused pool, never reused.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FlockCode(String);

impl FlockCode {
    pub const LEN: usize = 3;

    /// Parse and normalize (uppercase) a flock prefix.
    pub fn parse(raw: &str) -> Result<Self, FlockCodeError> {
        let chars: Vec<char> = raw.chars().collect();
        (chars.len() == Self::LEN)
            .then_some(())
            .ok_or(FlockCodeError::BadLength(chars.len()))?;
        chars
            .iter()
            .map(|&c| {
                Base36::digit_of(c)
                    .map(Base36::char_of)
                    .ok_or(FlockCodeError::BadChar(c))
            })
            .collect::<Result<String, _>>()
            .map(Self)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The prefix's digit values — the leading digits of every code in the
    /// flock, and part of the Damm input.
    pub fn digits(&self) -> [u8; Self::LEN] {
        let mut out = [0u8; Self::LEN];
        self.0
            .chars()
            .map(|c| Base36::digit_of(c).expect("validated on construction"))
            .zip(out.iter_mut())
            .for_each(|(d, slot)| *slot = d);
        out
    }

    /// The FF1 tweak for this flock: the ASCII bytes of the normalized prefix.
    /// Gives every flock an independent permutation under a shared key.
    pub fn tweak(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl std::fmt::Display for FlockCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DuckCodeError {
    #[error("duck code must be exactly {len} base36 characters, got {0}", len = DuckCode::LEN)]
    BadLength(usize),
    #[error("invalid character {0:?} in duck code")]
    BadChar(char),
    #[error("check character mismatch")]
    BadChecksum,
}

/// The public 7-character code printed on a duck: 3-char flock prefix,
/// 3-char FF1 payload, 1 Damm check character. Displayed grouped `QK7-MX2P`.
/// Validated on construction — holding one means charset, length, and
/// checksum already passed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DuckCode(String);

impl DuckCode {
    pub const LEN: usize = 7;
    pub const PAYLOAD_LEN: usize = 3;

    /// The only way to obtain a `DuckCode` from untrusted input. Strips `-`,
    /// spaces, and dots; accepts any case.
    pub fn parse(raw: &str) -> Result<Self, DuckCodeError> {
        let normalized = raw
            .chars()
            .filter(|c| !matches!(c, '-' | ' ' | '.'))
            .map(|c| Base36::digit_of(c).map(Base36::char_of).ok_or(DuckCodeError::BadChar(c)))
            .collect::<Result<String, _>>()?;
        (normalized.chars().count() == Self::LEN)
            .then_some(())
            .ok_or(DuckCodeError::BadLength(normalized.chars().count()))?;
        Damm36::validate(
            normalized
                .chars()
                .map(|c| Base36::digit_of(c).expect("charset checked above")),
        )
        .then_some(Self(normalized))
        .ok_or(DuckCodeError::BadChecksum)
    }

    /// Assemble a code from its parts, computing the check character. Only the
    /// codec calls this; it is `pub(crate)` so a checksum-less code can't be
    /// built from outside the domain crate.
    pub(crate) fn assemble(flock: &FlockCode, payload: [u8; Self::PAYLOAD_LEN]) -> Self {
        let body_digits: Vec<u8> = flock.digits().into_iter().chain(payload).collect();
        let check = Damm36::check_digit(body_digits.iter().copied());
        let text: String = body_digits
            .into_iter()
            .chain([check])
            .map(Base36::char_of)
            .collect();
        Self(text)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The 3-char flock prefix this code belongs to.
    pub fn flock_code(&self) -> FlockCode {
        FlockCode::parse(&self.0[..FlockCode::LEN]).expect("validated on construction")
    }

    /// The encrypted payload digits (positions 3..6).
    pub fn payload_digits(&self) -> [u8; Self::PAYLOAD_LEN] {
        let mut out = [0u8; Self::PAYLOAD_LEN];
        self.0[FlockCode::LEN..FlockCode::LEN + Self::PAYLOAD_LEN]
            .chars()
            .map(|c| Base36::digit_of(c).expect("validated on construction"))
            .zip(out.iter_mut())
            .for_each(|(d, slot)| *slot = d);
        out
    }

    /// Human-facing grouped form: `QK7-MX2P`.
    pub fn display_grouped(&self) -> String {
        format!("{}-{}", &self.0[..FlockCode::LEN], &self.0[FlockCode::LEN..])
    }
}

impl std::fmt::Display for DuckCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
