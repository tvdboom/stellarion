//! Persisted deterministic random-number state for simulation decisions.

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Serializable seed plus monotonically increasing simulation stream number.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DeterministicRngState {
    /// Secret-independent game seed persisted with the game state.
    pub seed: [u8; 32],
    /// Number of simulation streams already consumed.
    pub sequence: u64,
}

impl DeterministicRngState {
    /// Creates deterministic state from an explicit 256-bit seed.
    pub const fn new(seed: [u8; 32]) -> Self {
        Self {
            seed,
            sequence: 0,
        }
    }

    /// Creates deterministic state from a compact seed, primarily for tests and local games.
    pub fn from_u64(seed: u64) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"stellarion-game-seed-v1");
        hasher.update(seed.to_le_bytes());
        Self::new(hasher.finalize().into())
    }

    /// Returns the next independent deterministic stream and advances the persisted cursor.
    pub fn next_rng(&mut self) -> ChaCha8Rng {
        let mut hasher = Sha256::new();
        hasher.update(b"stellarion-simulation-stream-v1");
        hasher.update(self.seed);
        hasher.update(self.sequence.to_le_bytes());
        self.sequence = self.sequence.saturating_add(1);
        ChaCha8Rng::from_seed(hasher.finalize().into())
    }
}

impl Default for DeterministicRngState {
    /// Uses a fixed seed so default values are reproducible and safe in tests.
    fn default() -> Self {
        Self::from_u64(0)
    }
}
