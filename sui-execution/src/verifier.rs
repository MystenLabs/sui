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
}
