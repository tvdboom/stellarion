//! High-entropy game and recovery code generation, normalization, and hashing.

use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use thiserror::Error;

use crate::core::identity::GameCode;

const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const RECOVERY_BYTES: usize = 10;
const RECOVERY_SYMBOLS: usize = 16;
const LEGACY_RECOVERY_SYMBOLS: usize = 39;

/// Recovery secret shown only to the player who owns a slot.
pub struct RecoveryCode(String);

impl RecoveryCode {
    /// Generates an 80-bit recovery secret using the operating system or browser RNG.
    pub fn generate() -> Result<Self, RecoveryCodeError> {
        let mut random = [0_u8; RECOVERY_BYTES];
        getrandom::fill(&mut random)
            .map_err(|error| RecoveryCodeError::Entropy(error.to_string()))?;
        let canonical = encode_crockford(&random);
        Ok(Self(group(&canonical, 4)))
    }

    /// Parses and canonicalizes a user-entered recovery code.
    pub fn parse(value: impl AsRef<str>) -> Result<Self, RecoveryCodeError> {
        let canonical = normalize(value.as_ref());
        if !matches!(canonical.len(), RECOVERY_SYMBOLS | LEGACY_RECOVERY_SYMBOLS)
            || !canonical.bytes().all(|byte| CROCKFORD.contains(&byte))
        {
            return Err(RecoveryCodeError::Malformed);
        }
        Ok(Self(group(&canonical, 4)))
    }

    /// Returns the formatted code intended for explicit user display.
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Derives the one-way hash stored by the backend.
    pub fn hash(&self) -> RecoveryHash {
        let mut hasher = Sha256::new();
        hasher.update(b"stellarion-player-recovery-v1");
        hasher.update(normalize(&self.0).as_bytes());
        RecoveryHash(hex::encode(hasher.finalize()))
    }
}

/// Hex-encoded recovery hash safe to persist in Supabase.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryHash(pub String);

impl RecoveryHash {
    /// Verifies another hash without leaking an early mismatch position.
    pub fn constant_time_eq(&self, other: &Self) -> bool {
        self.0.as_bytes().ct_eq(other.0.as_bytes()).into()
    }
}

/// Malformed input or unavailable secure entropy.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum RecoveryCodeError {
    /// Browser or operating-system entropy was unavailable.
    #[error("secure random-number generation failed: {0}")]
    Entropy(String),
    /// User-entered code has the wrong length or alphabet.
    #[error("recovery code is malformed")]
    Malformed,
}

/// Generates a six-character, human-friendly candidate game code.
pub fn generate_game_code() -> Result<GameCode, RecoveryCodeError> {
    let mut random = [0_u8; 4];
    getrandom::fill(&mut random).map_err(|error| RecoveryCodeError::Entropy(error.to_string()))?;
    Ok(GameCode::new(&encode_crockford(&random)[..6]))
}

/// Generates an opaque local identifier for mock anonymous authentication.
pub fn generate_user_token() -> Result<String, RecoveryCodeError> {
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random).map_err(|error| RecoveryCodeError::Entropy(error.to_string()))?;
    Ok(hex::encode(random))
}

/// Encodes bytes with Crockford's ambiguity-resistant Base32 alphabet.
fn encode_crockford(bytes: &[u8]) -> String {
    let mut output = String::with_capacity((bytes.len() * 8).div_ceil(5));
    let mut buffer = 0_u32;
    let mut bits = 0_u8;
    for byte in bytes {
        buffer = (buffer << 8) | u32::from(*byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            let index = ((buffer >> bits) & 0x1f) as usize;
            output.push(CROCKFORD[index] as char);
        }
    }
    if bits > 0 {
        let index = ((buffer << (5 - bits)) & 0x1f) as usize;
        output.push(CROCKFORD[index] as char);
    }
    output
}

/// Removes separators and maps commonly confused Crockford characters.
fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_ascii_whitespace() && *character != '-')
        .map(|character| match character.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            other => other,
        })
        .collect()
}

/// Inserts separators without retaining an additional plaintext representation.
fn group(value: &str, width: usize) -> String {
    if width == 0 {
        return value.to_string();
    }
    let separators = value.len().saturating_sub(1) / width;
    let mut grouped = String::with_capacity(value.len().saturating_add(separators));
    for (index, character) in value.chars().enumerate() {
        if index > 0 && index % width == 0 {
            grouped.push('-');
        }
        grouped.push(character);
    }
    grouped
}

#[cfg(test)]
#[path = "../../tests/multiplayer/recovery.rs"]
mod tests;
