use crate::core::units::Description;
use bevy::prelude::Component;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign};
use strum_macros::EnumIter;

#[derive(Component, EnumIter, Clone, Debug)]
pub enum ResourceName {
    Metal,
    Crystal,
    Deuterium,
}

impl Description for ResourceName {
    fn description(&self) -> &str {
        match self {
            ResourceName::Metal => "Metal is the most basic resource, used in almost all constructions and ships.",
            ResourceName::Crystal => "Crystal is a more advanced resource, essential for high-tech buildings and ships.",
            ResourceName::Deuterium => "Deuterium is a rare and valuable resource, primarily used for high-tech ships and as fuel.",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Resources {
    pub metal: usize,
    pub crystal: usize,
    pub deuterium: usize,
}

impl Resources {
    pub fn new(metal: usize, crystal: usize, deuterium: usize) -> Self {
        Self {
            metal,
            crystal,
            deuterium,
        }
    }

    pub fn get(&self, resource: &ResourceName) -> usize {
        match resource {
            ResourceName::Metal => self.metal,
            ResourceName::Crystal => self.crystal,
            ResourceName::Deuterium => self.deuterium,
        }
    }
}

impl PartialOrd for Resources {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let all_gte = self.metal >= other.metal
            && self.crystal >= other.crystal
            && self.deuterium >= other.deuterium;

        let all_lte = self.metal <= other.metal
            && self.crystal <= other.crystal
            && self.deuterium <= other.deuterium;

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
                    }
                }
            }

            // Binary operations with usize
            impl<T: Into<usize>> $trait<T> for Resources {
                type Output = Self;

                fn $method(self, rhs: T) -> Self::Output {
                    let u = rhs.into();
                    Self {
                        metal: self.metal $op u,
                        crystal: self.crystal $op u,
                        deuterium: self.deuterium $op u,
                    }
                }
            }

            // Binary operations with usize on reference
            impl<T: Into<usize>> $trait<T> for &Resources {
                type Output = Resources;

                fn $method(self, rhs: T) -> Resources {
                    let float = rhs.into();
                    Resources {
                        metal: self.metal $op float,
                        crystal: self.crystal $op float,
                        deuterium: self.deuterium $op float,
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
                }
            }

            // Assignment operations with Resources reference
            impl $trait<&Self> for Resources {
                fn $method(&mut self, rhs: &Self) {
                    self.metal $op rhs.metal;
                    self.crystal $op rhs.crystal;
                    self.deuterium $op rhs.deuterium;
                }
            }

            // Assignment operations with usize
            impl<T: Into<usize>> $trait<T> for Resources {
                fn $method(&mut self, rhs: T) {
                    let u = rhs.into();
                    self.metal $op u;
                    self.crystal $op u;
                    self.deuterium $op u;
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
