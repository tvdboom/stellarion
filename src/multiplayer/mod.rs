//! Mockable multiplayer coordination and Supabase infrastructure.

pub mod authority;
pub mod backend;
#[cfg(feature = "app")]
pub mod client;
pub mod memory;
pub mod model;
pub mod realtime;
pub mod recovery;
pub mod supabase;
