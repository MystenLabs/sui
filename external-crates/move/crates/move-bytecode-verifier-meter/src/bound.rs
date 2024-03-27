// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::vm_status::StatusCode;
use move_vm_config::verifier::VerifierConfig;
use crate::{Meter, Scope};

/// Module and function level metering.
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
                max: config.max_per_mod_meter_units,
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
