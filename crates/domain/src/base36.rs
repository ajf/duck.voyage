/// The base36 alphabet shared by every code in the system: digits `0-9` then
/// `A-Z`, parsed case-insensitively, always displayed uppercase.
pub struct Base36;

impl Base36 {
    pub const RADIX: u32 = 36;

    /// Digit value of a character, accepting either case. `None` for anything
    /// outside `[0-9A-Za-z]`.
    pub fn digit_of(c: char) -> Option<u8> {
        c.to_digit(Self::RADIX).map(|d| d as u8)
    }

    /// Canonical (uppercase) character for a digit value in `0..36`.
    ///
    /// # Panics
    /// If `d >= 36`; callers hold digits produced by `digit_of` or the codec,
    /// which are in range by construction.
    pub fn char_of(d: u8) -> char {
        char::from_digit(u32::from(d), Self::RADIX)
            .expect("digit < 36 by construction")
            .to_ascii_uppercase()
    }
}
