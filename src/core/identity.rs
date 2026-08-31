//! Stable identifiers shared by gameplay, persistence, and multiplayer code.

use serde::{Deserialize, Serialize};

/// Stable numeric identifier for a player slot inside one game.
pub type PlayerId = u64;

/// Authenticated Supabase user identifier.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UserId(pub String);

impl UserId {
    /// Creates an identifier from its external string representation.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

/// Persisted game identifier assigned by the backend.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GameId(pub String);

impl GameId {
    /// Creates an identifier from its external string representation.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

/// Human-friendly code used to locate a game.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GameCode(pub String);

impl GameCode {
    /// Creates a normalized uppercase game code.
    pub fn new(value: impl AsRef<str>) -> Self {
        Self(value.as_ref().trim().to_ascii_uppercase())
    }

    /// Returns the normalized code as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
