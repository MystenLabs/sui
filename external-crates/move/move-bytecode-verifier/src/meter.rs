// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::VerifierConfig;
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::vm_status::StatusCode;
use std::ops::Mul;

/// Scope of meterinng
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scope {
    // Metering is for module level
    Module,
    // Metering is for function level
    Function,
}

pub const ANALYZE_FUNCTION_BASE_COST: u128 = 10;
pub const EXECUTE_BLOCK_BASE_COST: u128 = 10;
pub const PER_BACKEDGE_COST: u128 = 10;
pub const PER_SUCCESSOR_COST: u128 = 10;

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

pub struct BoundMeter {
    mod_bounds: Bounds,
    fun_bounds: Bounds,
}

struct Bounds {
    name: String,
    units: u128,
    max: Option<u128>,
}

impl Meter for BoundMeter {
    fn enter_scope(&mut self, name: &str, scope: Scope) {
        let bounds = self.get_bounds_mut(scope);
        bounds.name = name.into();
        bounds.units = 0;
    }

    fn transfer(&mut self, from: Scope, to: Scope, factor: f32) -> PartialVMResult<()> {
        let units = (self.get_bounds_mut(from).units as f32 * factor) as u128;
        self.add(to, units)
    }

    fn add(&mut self, scope: Scope, units: u128) -> PartialVMResult<()> {
        self.get_bounds_mut(scope).add(units)
    }
}

impl Bounds {
    fn add(&mut self, units: u128) -> PartialVMResult<()> {
        if let Some(max) = self.max {
            let new_units = self.units.saturating_add(units);
            if new_units > max {
                // TODO: change to a new status PROGRAM_TOO_COMPLEX once this is rolled out. For
                // now we use an existing code to avoid breaking changes on potential rollback.
                return Err(PartialVMError::new(StatusCode::CONSTRAINT_NOT_SATISFIED)
                    .with_message(format!(
                        "program too complex (in `{}` with `{} current + {} new > {} max`)",
                        self.name, self.units, units, max
                    )));
            }
            self.units = new_units;
        }
        Ok(())
    }
}

impl BoundMeter {
    pub fn new(config: &VerifierConfig) -> Self {
        Self {
            mod_bounds: Bounds {
                name: "<unknown>".to_string(),
                units: 0,
                max: config.max_per_fun_meter_units,
            },
            fun_bounds: Bounds {
                name: "<unknown>".to_string(),
                units: 0,
                max: config.max_per_fun_meter_units,
            },
        }
    }

    fn get_bounds_mut(&mut self, scope: Scope) -> &mut Bounds {
        if scope == Scope::Module {
            &mut self.mod_bounds
        } else {
            &mut self.fun_bounds
        }
    }

    fn get_bounds(&self, scope: Scope) -> &Bounds {
        if scope == Scope::Module {
            &self.mod_bounds
        } else {
            &self.fun_bounds
        }
    }

    pub fn get_usage(&self, scope: Scope) -> u128 {
        self.get_bounds(scope).units
    }

    pub fn get_limit(&self, scope: Scope) -> Option<u128> {
        self.get_bounds(scope).max
    }
}

pub struct DummyMeter;
impl Meter for DummyMeter {
    fn enter_scope(&mut self, _name: &str, _scope: Scope) {}
    fn transfer(&mut self, _from: Scope, _to: Scope, _factor: f32) -> PartialVMResult<()> {
        Ok(())
    }
    fn add(&mut self, _scope: Scope, _units: u128) -> PartialVMResult<()> {
        Ok(())
    }
}
