// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_bytecode_verifier_meter::{Meter, Scope};
use move_core_types::vm_status::StatusCode;
use move_vm_config::verifier::MeterConfig;

struct SuiVerifierMeterBounds {
    name: String,
    ticks: u128,
    max_ticks: Option<u128>,
}

impl SuiVerifierMeterBounds {
    fn add(&mut self, ticks: u128) -> PartialVMResult<()> {
        let max_ticks = self.max_ticks.unwrap_or(u128::MAX);

        let new_ticks = self.ticks.saturating_add(ticks);
        if new_ticks >= max_ticks {
            return Err(PartialVMError::new(StatusCode::PROGRAM_TOO_COMPLEX)
                    .with_message(format!(
                        "program too complex. Ticks exceeded `{}` will exceed limits: `{} current + {} new > {} max`)",
                        self.name, self.ticks, ticks, max_ticks
                    )));
        }
        self.ticks = new_ticks;
        Ok(())
    }
}

pub struct SuiVerifierMeter {
    transaction_bounds: SuiVerifierMeterBounds,
    package_bounds: SuiVerifierMeterBounds,
    module_bounds: SuiVerifierMeterBounds,
    function_bounds: SuiVerifierMeterBounds,
}

impl SuiVerifierMeter {
    pub fn new(config: MeterConfig) -> Self {
        Self {
            transaction_bounds: SuiVerifierMeterBounds {
                name: "<unknown>".to_string(),
                ticks: 0,
                max_ticks: None,
            },

            // Not used for now to keep backward compat
            package_bounds: SuiVerifierMeterBounds {
                name: "<unknown>".to_string(),
                ticks: 0,
                max_ticks: None,
            },
            module_bounds: SuiVerifierMeterBounds {
                name: "<unknown>".to_string(),
                ticks: 0,
                max_ticks: config.max_per_mod_meter_units,
            },
            function_bounds: SuiVerifierMeterBounds {
                name: "<unknown>".to_string(),
                ticks: 0,
                max_ticks: config.max_per_fun_meter_units,
            },
        }
    }

    fn get_bounds_mut(&mut self, scope: Scope) -> &mut SuiVerifierMeterBounds {
        match scope {
            Scope::Transaction => &mut self.transaction_bounds,
            Scope::Package => &mut self.package_bounds,
            Scope::Module => &mut self.module_bounds,
            Scope::Function => &mut self.function_bounds,
        }
    }

    fn get_bounds(&self, scope: Scope) -> &SuiVerifierMeterBounds {
        match scope {
            Scope::Transaction => &self.transaction_bounds,
            Scope::Package => &self.package_bounds,
            Scope::Module => &self.module_bounds,
            Scope::Function => &self.function_bounds,
        }
    }

    pub fn get_usage(&self, scope: Scope) -> u128 {
        self.get_bounds(scope).ticks
    }

    pub fn get_limit(&self, scope: Scope) -> Option<u128> {
        self.get_bounds(scope).max_ticks
    }
}

impl Meter for SuiVerifierMeter {
    fn enter_scope(&mut self, name: &str, scope: Scope) {
        let bounds = self.get_bounds_mut(scope);
        bounds.name = name.into();
        bounds.ticks = 0;
    }

    fn transfer(&mut self, from: Scope, to: Scope, factor: f32) -> PartialVMResult<()> {
        let ticks = (self.get_bounds_mut(from).ticks as f32 * factor) as u128;
        self.add(to, ticks)
    }

    fn add(&mut self, scope: Scope, ticks: u128) -> PartialVMResult<()> {
        self.get_bounds_mut(scope).add(ticks)
    }
}
