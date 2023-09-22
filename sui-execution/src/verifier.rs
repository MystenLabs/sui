// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::CompiledModule;
use sui_protocol_config::ProtocolConfig;
use sui_types::error::SuiResult;

pub trait Verifier {
    /// Run the bytecode verifier with a meter limit
    ///
    /// This function only fails if the verification does not complete within the limit.  If the
    /// modules fail to verify but verification completes within the meter limit, the function
    /// succeeds.
    fn meter_compiled_modules(
        &mut self,
        protocol_config: &ProtocolConfig,
        modules: &[CompiledModule],
    ) -> SuiResult<()>;

    fn meter_module_bytes(
        &mut self,
        protocol_config: &ProtocolConfig,
        module_bytes: &[Vec<u8>],
    ) -> SuiResult<()> {
        let Ok(modules) = module_bytes
            .iter()
            .map(|b| {
                CompiledModule::deserialize_with_config(
                    b,
                    protocol_config.move_binary_format_version(),
                    protocol_config.no_extraneous_module_bytes(),
                )
            })
            .collect::<Result<Vec<_>, _>>()
        else {
            // Although we failed, we don't care since it wasn't because of a timeout.
            return Ok(());
        };

        self.meter_compiled_modules(protocol_config, &modules)
    }

    fn meter_compiled_modules_with_overrides(
        &mut self,
        modules: &[CompiledModule],
        protocol_config: &ProtocolConfig,
        config_overrides: &VerifierOverrides,
    ) -> SuiResult<VerifierMeteredValues>;
}

/// Controls verifier config values to override.
pub struct VerifierOverrides {
    pub max_per_fun_meter_units: Option<u128>,
    pub max_per_mod_meter_units: Option<u128>,
}

/// When returning from `meter_compiled_modules_with_overrides` `VerifierMeteredValues`
/// will report the actual value that the verifier used for the overrides, and the values
/// that were overridden (the limits as known to the config).
pub struct VerifierMeteredValues {
    pub max_per_fun_meter_current: Option<u128>,
    pub max_per_mod_meter_current: Option<u128>,
    pub fun_meter_units_result: u128,
    pub mod_meter_units_result: u128,
}

impl VerifierOverrides {
    /// Create a new `VerifierOverrides` with the given overrides.
    /// `None` implies the specific check is unbound.
    pub fn new(
        max_per_fun_meter_units: Option<u128>,
        max_per_mod_meter_units: Option<u128>,
    ) -> Self {
        Self {
            max_per_fun_meter_units,
            max_per_mod_meter_units,
        }
    }
}

impl VerifierMeteredValues {
    /// Create a new `VerifierOverrides` with the given overrides.
    /// `None` implies the specific check is unbound.
    pub fn new(
        max_per_fun_meter_current: Option<u128>,
        max_per_mod_meter_current: Option<u128>,
        fun_meter_units_result: u128,
        mod_meter_units_result: u128,
    ) -> Self {
        Self {
            max_per_fun_meter_current,
            max_per_mod_meter_current,
            fun_meter_units_result,
            mod_meter_units_result,
        }
    }
}
