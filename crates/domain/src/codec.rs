use std::collections::BTreeMap;

use fpe::ff1::{FlexibleNumeralString, FF1};

use crate::base36::Base36;
use crate::codes::{DuckCode, FlockCode, FlockSeq, KeyGeneration};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CodecError {
    #[error("no FF1 key configured for generation {0}")]
    UnknownGeneration(KeyGeneration),
    #[error("no key generations configured")]
    NoGenerations,
    #[error("current generation {0} has no configured key")]
    CurrentKeyMissing(KeyGeneration),
    #[error("payload decrypts outside the valid flock-sequence range ({0})")]
    SeqOutOfRange(u32),
}

/// Encodes and decodes duck codes: `(flock, seq)` ⟷ printed code, via FF1
/// format-preserving encryption over the 3-digit base36 payload domain,
/// tweaked by the flock prefix so every flock gets an independent permutation.
///
/// Holds one cipher per key generation; old generations are decode-only,
/// new flocks mint under [`Self::current_generation`].
pub struct DuckCodec {
    ciphers: BTreeMap<KeyGeneration, FF1<aes::Aes256>>,
    current: KeyGeneration,
}

impl DuckCodec {
    /// Build from the configured generation keys (32-byte AES-256 keys) and
    /// the generation new flocks should mint under.
    pub fn new(
        keys: impl IntoIterator<Item = (KeyGeneration, [u8; 32])>,
        current: KeyGeneration,
    ) -> Result<Self, CodecError> {
        let ciphers: BTreeMap<_, _> = keys
            .into_iter()
            .map(|(generation, key)| {
                let cipher = FF1::<aes::Aes256>::new(&key, Base36::RADIX)
                    .expect("a 32-byte key is always valid for AES-256 FF1");
                (generation, cipher)
            })
            .collect();
        (!ciphers.is_empty())
            .then_some(())
            .ok_or(CodecError::NoGenerations)?;
        ciphers
            .contains_key(&current)
            .then_some(())
            .ok_or(CodecError::CurrentKeyMissing(current))?;
        Ok(Self { ciphers, current })
    }

    /// The generation new flocks are created under.
    pub fn current_generation(&self) -> KeyGeneration {
        self.current
    }

    fn cipher(&self, generation: KeyGeneration) -> Result<&FF1<aes::Aes256>, CodecError> {
        self.ciphers
            .get(&generation)
            .ok_or(CodecError::UnknownGeneration(generation))
    }

    /// `(flock, seq)` → public code under the given generation:
    /// `prefix + FF1(seq, tweak = prefix) + check char`.
    pub fn encode(
        &self,
        generation: KeyGeneration,
        flock: &FlockCode,
        seq: FlockSeq,
    ) -> Result<DuckCode, CodecError> {
        let digits = Self::to_digits(u32::from(seq.get()));
        let encrypted = self
            .cipher(generation)?
            .encrypt(flock.tweak(), &FlexibleNumeralString::from(digits.to_vec()))
            .expect("payload length and radix are fixed and valid");
        let payload: Vec<u16> = encrypted.into();
        let payload: [u8; DuckCode::PAYLOAD_LEN] =
            std::array::from_fn(|i| payload[i] as u8);
        Ok(DuckCode::assemble(flock, payload))
    }

    /// Code → candidate sequence number under the given generation. The caller
    /// resolved the flock by prefix first (that's where `generation` comes
    /// from); existence of a duck at `(flock, seq)` is the database's question.
    pub fn decode(
        &self,
        generation: KeyGeneration,
        code: &DuckCode,
    ) -> Result<FlockSeq, CodecError> {
        let flock = code.flock_code();
        let digits: Vec<u16> = code.payload_digits().iter().map(|&d| u16::from(d)).collect();
        let decrypted = self
            .cipher(generation)?
            .decrypt(flock.tweak(), &FlexibleNumeralString::from(digits))
            .expect("payload length and radix are fixed and valid");
        let value = Self::from_digits(Vec::<u16>::from(decrypted));
        FlockSeq::new(value).map_err(|_| CodecError::SeqOutOfRange(value))
    }

    /// Big-endian base36 digits of a payload-domain value.
    fn to_digits(value: u32) -> [u16; DuckCode::PAYLOAD_LEN] {
        std::array::from_fn(|i| {
            let shift = (DuckCode::PAYLOAD_LEN - 1 - i) as u32;
            (value / Base36::RADIX.pow(shift) % Base36::RADIX) as u16
        })
    }

    fn from_digits(digits: Vec<u16>) -> u32 {
        digits
            .iter()
            .fold(0u32, |acc, &d| acc * Base36::RADIX + u32::from(d))
    }
}
