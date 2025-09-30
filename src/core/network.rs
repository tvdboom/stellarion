use crate::utils::get_local_ip;
use bevy::prelude::*;

#[derive(Resource)]
pub struct Ip(pub String);

impl Default for Ip {
    fn default() -> Self {
        Self(get_local_ip().to_string())
    }
}
