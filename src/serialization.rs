//! Strict helpers for fields whose value may be null but whose key is mandatory.

use serde::{Deserialize, Deserializer};

/// Deserializes an explicitly present optional value.
pub(crate) fn required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}
