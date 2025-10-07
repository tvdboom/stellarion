use crate::core::resources::Resources;

pub mod buildings;
pub mod defense;
pub mod missions;
pub mod ships;

pub trait Description {
    fn description(&self) -> &str;
}

pub trait Price {
    fn price(&self) -> Resources;
}

pub trait Combat {
    fn health(&self) -> usize;
    fn shield(&self) -> usize;
    fn damage(&self) -> usize;
    fn speed(&self) -> f32;
    fn fuel_consumption(&self) -> usize;
}
