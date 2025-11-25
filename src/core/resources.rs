use std::cmp::Ordering;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign};

use bevy::prelude::Component;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

use crate::core::units::Description;

#[derive(Component, EnumIter, Clone, Copy, Debug, Default, PartialEq)]
pub enum ResourceName {
    #[default]
    Metal,
    Crystal,
    Deuterium,
}

impl ResourceName {
    pub fn next(&self, skip: Option<ResourceName>) -> ResourceName {
        let mut next = match self {
            ResourceName::Metal => ResourceName::Crystal,
            ResourceName::Crystal => ResourceName::Deuterium,
            ResourceName::Deuterium => ResourceName::Metal,
        };

        if skip == Some(next) {
            next = next.next(None);
        }

        next
    }

    pub fn prev(&self, skip: Option<ResourceName>) -> ResourceName {
        let mut prev = match self {
            ResourceName::Metal => ResourceName::Deuterium,
            ResourceName::Crystal => ResourceName::Metal,
            ResourceName::Deuterium => ResourceName::Metal,
        };

        if skip == Some(prev) {
            prev = prev.prev(None);
        }

        prev
    }
}

impl Description for ResourceName {
    fn description(&self) -> &str {
        match self {
            ResourceName::Metal => "Metal is the most basic resource, used in almost all constructions and ships.",
            ResourceName::Crystal => "Crystal is a more advanced resource, essential for high-level buildings and ships.",
            ResourceName::Deuterium => "Deuterium is a rare and valuable resource, primarily used for high-level ships and as fuel.",
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

    pub fn get_mut(&mut self, resource: &ResourceName) -> &mut usize {
        match resource {
            ResourceName::Metal => &mut self.metal,
            ResourceName::Crystal => &mut self.crystal,
            ResourceName::Deuterium => &mut self.deuterium,
        }
    }

    pub fn min(&self) -> usize {
        self.metal.min(self.crystal).min(self.deuterium)
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

impl Sum for Resources {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::default(), |acc, x| acc + x)
    }
}
impl Resources {
    #[inline]
    fn safe_op<F>(self, rhs: Resources, f: F) -> Self
    where
        F: Fn(usize, usize) -> usize,
    {
        Self {
            metal: f(self.metal, rhs.metal),
            crystal: f(self.crystal, rhs.crystal),
            deuterium: f(self.deuterium, rhs.deuterium),
        }
    }

    #[inline]
    fn safe_scalar<F>(self, rhs: usize, f: F) -> Self
    where
        F: Fn(usize, usize) -> usize,
    {
        Self {
            metal: f(self.metal, rhs),
            crystal: f(self.crystal, rhs),
            deuterium: f(self.deuterium, rhs),
        }
    }
}

macro_rules! resources_binary_ops {
    ($($trait:ident, $method:ident, $op:tt);*;) => {
        $(
            impl $trait<Self> for Resources {
                type Output = Self;

                fn $method(self, rhs: Resources) -> Self::Output {
                    if stringify!($trait) == "Div" {
                        self.safe_op(rhs, |a, b| if b == 0 { usize::MAX } else { a / b })
                    } else {
                        self.safe_op(rhs, |a, b| a $op b)
                    }
                }
            }

            impl<T: Into<usize>> $trait<T> for Resources {
                type Output = Self;

                fn $method(self, rhs: T) -> Self::Output {
                    let rhs = rhs.into();
                    if stringify!($trait) == "Div" {
                        self.safe_scalar(rhs, |a, b| if b == 0 { usize::MAX } else { a / b })
                    } else {
                        self.safe_scalar(rhs, |a, b| a $op b)
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
