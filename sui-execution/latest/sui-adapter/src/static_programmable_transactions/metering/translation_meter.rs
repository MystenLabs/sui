// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::gas_charger::GasCharger;
use sui_protocol_config::ProtocolConfig;
use sui_types::error::{ExecutionError, ExecutionErrorKind};

/// The [`TranslationMeter`] is responsible for metering gas usage for various operations
/// during the translation of a transaction. It interacts with and exposes interfaces to the
/// [`GasCharger`] it holds in order to deduct gas based on the operations performed.
///
/// It holds a reference to the `ProtocolConfig` to access protocol-specific configuration
/// parameters that may influence gas costs and limits.
pub struct TranslationMeter<'pc, 'gas> {
    protocol_config: &'pc ProtocolConfig,
    charger: &'gas mut GasCharger,
    charged: u64,
}

impl<'pc, 'gas> TranslationMeter<'pc, 'gas> {
    pub fn new(
        protocol_config: &'pc ProtocolConfig,
        gas_charger: &'gas mut GasCharger,
    ) -> TranslationMeter<'pc, 'gas> {
        TranslationMeter {
            protocol_config,
            charger: gas_charger,
            charged: 0,
        }
    }

    pub fn charge_base_inputs(&mut self, num_inputs: usize) -> Result<(), ExecutionError> {
        let amount = (num_inputs as u64)
            .max(1)
            .saturating_mul(self.protocol_config.translation_per_input_base_charge());
        self.charge(amount)
    }

    pub fn charge_pure_input_bytes(&mut self, num_bytes: usize) -> Result<(), ExecutionError> {
        let amount = (num_bytes as u64).max(1).saturating_mul(
            self.protocol_config
                .translation_pure_input_per_byte_charge(),
        );
        self.charge(amount)
    }

    pub fn charge_base_command(&mut self, num_args: usize) -> Result<(), ExecutionError> {
        let amount = (num_args as u64)
            .max(1)
            .saturating_mul(self.protocol_config.translation_per_command_base_charge());
        self.charge(amount)
    }

    /// Charge gas for loading types based on the number of type nodes loaded.
    /// The cost is calculated as `num_type_nodes * TYPE_LOAD_PER_NODE_MULTIPLIER`.
    /// This function assumes that `num_type_nodes` is non-zero.
    pub fn charge_num_type_nodes(&mut self, num_type_nodes: u64) -> Result<(), ExecutionError> {
        let amount = num_type_nodes
            .max(1)
            .saturating_mul(self.protocol_config.translation_per_type_node_charge());
        self.charge(amount)
    }

    pub fn charge_num_type_references(
        &mut self,
        num_type_references: u64,
    ) -> Result<(), ExecutionError> {
        let amount = self.reference_cost_formula(num_type_references.max(1))?;
        let amount =
            amount.saturating_mul(self.protocol_config.translation_per_reference_node_charge());
        self.charge(amount)
    }

    pub fn charge_num_linkage_entries(
        &mut self,
        num_linkage_entries: usize,
    ) -> Result<(), ExecutionError> {
        let amount = (num_linkage_entries as u64)
            .saturating_mul(self.protocol_config.translation_per_linkage_entry_charge())
            .max(1);
        self.charge(amount)
    }

    // We use a non-linear cost function for type references to account for the increased
    // complexity they introduce. The cost is calculated as:
    // cost = (num_type_references * (num_type_references + 1)) / 2
    //
    // Take &self to access protocol config if needed in the future.
    fn reference_cost_formula(&self, n: u64) -> Result<u64, ExecutionError> {
        let Some(n_succ) = n.checked_add(1) else {
            invariant_violation!("u64 overflow when calculating type reference cost")
        };
        Ok(n.saturating_mul(n_succ) / 2)
    }

    // Charge gas using a point charge mechanism based on the cumulative number of units charged so
    // far.
    fn charge(&mut self, amount: u64) -> Result<(), ExecutionError> {
        debug_assert!(amount > 0);
        let scaled_charge = self.calculate_point_charge(amount);
        self.charger
            .move_gas_status_mut()
            .deduct_gas(scaled_charge.into())
            .map_err(Self::gas_error)
    }

    // The point charge is calculated as:
    // point_multiplier = (n / translation_metering_step_resolution)^2
    // point_charge = point_multiplier * amount
    // where `n` is the cumulative number of units charged so far.
    //
    // This function updates the `charged` field with the new cumulative charge once the point
    // charge has been determined.
    fn calculate_point_charge(&mut self, amount: u64) -> u64 {
        debug_assert!(self.protocol_config.translation_metering_step_resolution() > 0);
        let point_multiplier = self
            .charged
            .checked_div(self.protocol_config.translation_metering_step_resolution())
            .unwrap_or(0)
            .max(1);
        debug_assert!(point_multiplier > 0);
        debug_assert!(amount > 0);
        let point_charge = point_multiplier
            .saturating_mul(point_multiplier)
            .saturating_mul(amount);
        self.charged = self.charged.saturating_add(point_charge);
        point_charge
    }

    fn gas_error<E>(e: E) -> ExecutionError
    where
        E: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
    {
        ExecutionError::new_with_source(ExecutionErrorKind::InsufficientGas, e)
    }
}
