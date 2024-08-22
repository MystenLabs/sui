// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::PartialVMResult;
use std::ops::Mul;

pub mod bound;
pub mod dummy;

/// Scope of metering
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scope {
    // Metering is for transaction level
    Transaction,
    // Metering is for package level
    Package,
    // Metering is for module level
    Module,
    // Metering is for function level
    Function,
}

/// Trait for a metering verification.
pub trait Meter {
    /// Indicates the begin of a new scope.
    fn enter_scope(&mut self, name: &str, scope: Scope);

    /// Transfer the amount of metering from once scope to the next. If the current scope has
    /// metered N units, the target scope will be charged with N*factor.
    fn transfer(&mut self, from: Scope, to: Scope, factor: f32) -> PartialVMResult<()>;

    /// Add the number of units to the meter, returns an error if a limit is hit.
    fn add(&mut self, scope: Scope, units: u128) -> PartialVMResult<()>;

    /// Adds the number of items.
    fn add_items(
        &mut self,
        scope: Scope,
        units_per_item: u128,
        items: usize,
    ) -> PartialVMResult<()> {
        if items == 0 {
            return Ok(());
        }
        self.add(scope, units_per_item.saturating_mul(items as u128))
    }

    /// Adds the number of items with growth factor
    #[deprecated(note = "this function is extremely slow and should be avoided")]
    fn add_items_with_growth(
        &mut self,
        scope: Scope,
        mut units_per_item: u128,
        items: usize,
        growth_factor: f32,
    ) -> PartialVMResult<()> {
        if items == 0 {
            return Ok(());
        }
        for _ in 0..items {
            self.add(scope, units_per_item)?;
            units_per_item = growth_factor.mul(units_per_item as f32) as u128;
        }
        Ok(())
    }
}

/// Convenience trait implementation to support static dispatch against a trait object.
impl Meter for &mut dyn Meter {
    fn enter_scope(&mut self, name: &str, scope: Scope) {
        (*self).enter_scope(name, scope)
    }

    fn transfer(&mut self, from: Scope, to: Scope, factor: f32) -> PartialVMResult<()> {
        (*self).transfer(from, to, factor)
    }

    fn add(&mut self, scope: Scope, units: u128) -> PartialVMResult<()> {
        (*self).add(scope, units)
    }
}
