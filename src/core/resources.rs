//! Resource names, saturating resource bundles, and safe economy arithmetic.

use std::cmp::Ordering;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign};

use bevy::prelude::Component;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

use crate::core::units::Description;

#[derive(
    Component, EnumIter, Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize,
)]
/// Tradable and producible resource categories.
pub enum ResourceName {
    #[default]
    /// The metal resource.
    Metal,
    /// The crystal resource.
    Crystal,
    /// The deuterium resource.
    Deuterium,
}

impl ResourceName {
    /// Returns the next resource kind in display order.
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

    /// Returns the previous resource kind in display order.
    pub fn prev(&self, skip: Option<ResourceName>) -> ResourceName {
        let mut prev = match self {
            ResourceName::Metal => ResourceName::Deuterium,
            ResourceName::Crystal => ResourceName::Metal,
            ResourceName::Deuterium => ResourceName::Crystal,
        };

        if skip == Some(prev) {
            prev = prev.prev(None);
        }

        prev
    }
}

impl Description for ResourceName {
    /// Returns the user-facing description of this gameplay value.
    fn description(&self) -> &str {
        match self {
            ResourceName::Metal => "Metal is the most basic resource, used in almost all constructions and ships.",
            ResourceName::Crystal => "Crystal is a more advanced resource, essential for high-level buildings and ships.",
            ResourceName::Deuterium => "Deuterium is a rare and valuable resource, primarily used for high-level ships and as fuel.",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
/// Saturating bundle of metal, crystal, and deuterium amounts.
pub struct Resources {
    /// Stored metal amount.
    pub metal: usize,
    /// Stored crystal amount.
    pub crystal: usize,
    /// Stored deuterium amount.
    pub deuterium: usize,
}

impl Resources {
    /// Creates a new value from the supplied state.
    pub fn new(metal: usize, crystal: usize, deuterium: usize) -> Self {
        Self {
            metal,
            crystal,
            deuterium,
        }
    }

    /// Returns state for the requested stable identifier.
    pub fn get(&self, resource: &ResourceName) -> usize {
        match resource {
            ResourceName::Metal => self.metal,
            ResourceName::Crystal => self.crystal,
            ResourceName::Deuterium => self.deuterium,
        }
    }

    /// Returns mutable state for the requested stable identifier.
    pub fn get_mut(&mut self, resource: &ResourceName) -> &mut usize {
        match resource {
            ResourceName::Metal => &mut self.metal,
            ResourceName::Crystal => &mut self.crystal,
            ResourceName::Deuterium => &mut self.deuterium,
        }
    }

    /// Returns the component-wise minimum of two resource bundles.
    pub fn min(&self) -> usize {
        self.metal.min(self.crystal).min(self.deuterium)
    }
}

impl PartialOrd for Resources {
    /// Compares resource bundles when every component has a consistent ordering.
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
    /// Sums an iterator of resource bundles with saturating arithmetic.
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::default(), |acc, x| acc + x)
    }
}
impl Resources {
    #[inline]
    /// Applies safe op without allowing arithmetic overflow.
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
    /// Applies safe scalar without allowing arithmetic overflow.
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
    ($($trait:ident, $method:ident, $operation:expr);*;) => {
        $(
            impl $trait<Self> for Resources {
                /// Resulting resource bundle produced by this arithmetic operator.
                type Output = Self;

                fn $method(self, rhs: Resources) -> Self::Output {
                    self.safe_op(rhs, $operation)
                }
            }

            impl<T: Into<usize>> $trait<T> for Resources {
                /// Resulting resource bundle produced by this arithmetic operator.
                type Output = Self;

                fn $method(self, rhs: T) -> Self::Output {
                    let rhs = rhs.into();
                    self.safe_scalar(rhs, $operation)
                }
            }
        )*
    };
}

resources_binary_ops!(
    Add, add, |a: usize, b: usize| a.saturating_add(b);
    Sub, sub, |a: usize, b: usize| a.saturating_sub(b);
    Mul, mul, |a: usize, b: usize| a.saturating_mul(b);
    Div, div, |a: usize, b: usize| a.checked_div(b).unwrap_or(usize::MAX);
);

macro_rules! resources_assignment_ops {
    ($($trait:ident, $method:ident, $binary_trait:ident, $binary_method:ident);*;) => {
        $(
            impl $trait<Self> for Resources {
                fn $method(&mut self, rhs: Self) {
                    *self = $binary_trait::$binary_method(*self, rhs);
                }
            }

            impl $trait<&Self> for Resources {
                fn $method(&mut self, rhs: &Self) {
                    *self = $binary_trait::$binary_method(*self, *rhs);
                }
            }

            impl<T: Into<usize>> $trait<T> for Resources {
                fn $method(&mut self, rhs: T) {
                    *self = $binary_trait::$binary_method(*self, rhs.into());
                }
            }
        )*
    };
}

resources_assignment_ops!(
    AddAssign, add_assign, Add, add;
    SubAssign, sub_assign, Sub, sub;
    MulAssign, mul_assign, Mul, mul;
    DivAssign, div_assign, Div, div;
);

#[cfg(test)]
#[path = "../../tests/core/resources.rs"]
mod tests;
