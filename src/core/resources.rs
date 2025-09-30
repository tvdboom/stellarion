use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Resources {
    pub metal: f32,
    pub crystal: f32,
    pub deuterium: f32,
    pub energy: f32,
}

impl Resources {
    pub fn new(metal: f32, crystal: f32, deuterium: f32, energy: f32) -> Self {
        Self {
            metal,
            crystal,
            deuterium,
            energy,
        }
    }
}

impl PartialOrd for Resources {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let all_gte = self.metal >= other.metal
            && self.crystal >= other.crystal
            && self.deuterium >= other.deuterium
            && self.energy >= other.energy;

        let all_lte = self.metal <= other.metal
            && self.crystal <= other.crystal
            && self.deuterium <= other.deuterium
            && self.energy <= other.energy;

        match (all_gte, all_lte) {
            (true, true) => Some(Ordering::Equal),
            (true, false) => Some(Ordering::Greater),
            (false, true) => Some(Ordering::Less),
            (false, false) => None,
        }
    }
}

macro_rules! resources_binary_ops {
    ($($trait:ident, $method:ident, $op:tt);*;) => {
        $(
            // Binary operations with Resources reference
            impl $trait<&Self> for Resources {
                type Output = Self;

                fn $method(self, rhs: &Resources) -> Self::Output {
                    Self {
                        metal: self.metal $op rhs.metal,
                        crystal: self.crystal $op rhs.crystal,
                        deuterium: self.deuterium $op rhs.deuterium,
                        energy: self.energy $op rhs.energy,
                    }
                }
            }

            // Binary operations with float
            impl<T: Into<f32>> $trait<T> for Resources {
                type Output = Self;

                fn $method(self, rhs: T) -> Self::Output {
                    let float = rhs.into();
                    Self {
                        metal: self.metal $op float,
                        crystal: self.crystal $op float,
                        deuterium: self.deuterium $op float,
                        energy: self.energy $op float,
                    }
                }
            }

            // Binary operations with float on reference
            impl<T: Into<f32>> $trait<T> for &Resources {
                type Output = Resources;

                fn $method(self, rhs: T) -> Resources {
                    let float = rhs.into();
                    Resources {
                        metal: self.metal $op float,
                        crystal: self.crystal $op float,
                        deuterium: self.deuterium $op float,
                        energy: self.energy $op float,
                    }
                }
            }
        )*
    };
}

resources_binary_ops!(
    Add, add, +;
    Sub, sub, -;
    Mul, mul, *;
    Div, div, /;
);

macro_rules! resources_assignment_ops {
    ($($trait:ident, $method:ident, $op:tt);*;) => {
        $(
            // Assignment operations with Resources
            impl $trait<Self> for Resources {
                fn $method(&mut self, rhs: Self) {
                    self.metal $op rhs.metal;
                    self.crystal $op rhs.crystal;
                    self.deuterium $op rhs.deuterium;
                    self.energy $op rhs.energy;
                }
            }

            // Assignment operations with Resources reference
            impl $trait<&Self> for Resources {
                fn $method(&mut self, rhs: &Self) {
                    self.metal $op rhs.metal;
                    self.crystal $op rhs.crystal;
                    self.deuterium $op rhs.deuterium;
                    self.energy $op rhs.energy;
                }
            }

            // Assignment operations with float
            impl<T: Into<f32>> $trait<T> for Resources {
                fn $method(&mut self, rhs: T) {
                    let float = rhs.into();
                    self.metal $op float;
                    self.crystal $op float;
                    self.deuterium $op float;
                    self.energy $op float;
                }
            }
        )*
    };
}

resources_assignment_ops!(
    AddAssign, add_assign, +=;
    SubAssign, sub_assign, -=;
    MulAssign, mul_assign, *=;
    DivAssign, div_assign, /=;
);
