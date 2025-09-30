use crate::core::resources::Resources;
use bevy::prelude::*;
use bevy_renet::renet::ClientId;
use serde::{Deserialize, Serialize};

#[derive(Resource, Clone, Serialize, Deserialize)]
pub struct Players(pub Vec<Player>);

impl Default for Players {
    fn default() -> Self {
        Self(vec![Player::default()])
    }
}

impl Players {
    pub fn get(&self, id: ClientId) -> &Player {
        self.0.iter().find(|p| p.id == id).unwrap()
    }

    pub fn get_mut(&mut self, id: ClientId) -> &mut Player {
        self.0.iter_mut().find(|p| p.id == id).unwrap()
    }

    pub fn main(&self) -> &Player {
        self.0.first().unwrap()
    }

    pub fn main_mut(&mut self) -> &mut Player {
        self.0.first_mut().unwrap()
    }

    pub fn main_id(&self) -> ClientId {
        self.main().id
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Player {
    pub id: ClientId,
    pub resources: Resources,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            id: 0,
            resources: Resources {
                metal: 150.,
                crystal: 150.,
                deuterium: 150.,
                energy: 150.,
            },
        }
    }
}

impl Player {
    pub fn new(id: ClientId) -> Self {
        Self { id, ..default() }
    }
}
