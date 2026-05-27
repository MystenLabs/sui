// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::gas_charger::GasCharger;
use sui_protocol_config::ProtocolConfig;
use sui_types::error::ExecutionErrorTrait;
use sui_types::execution_status::ExecutionErrorKind;

/// The [`TranslationMeter`] is responsible for metering gas usage for various operations
/// during the translation of a transaction. It interacts with and exposes interfaces to the
/// [`GasCharger`] it holds in order to deduct gas based on the operations performed.
///
/// It holds a reference to the `ProtocolConfig` to access protocol-specific configuration
/// parameters that may influence gas costs and limits.
pub struct TranslationMeter<'pc, 'gas> {
    protocol_config: &'pc ProtocolConfig,
    charger: &'gas mut GasCharger,
}

impl<'pc, 'gas> TranslationMeter<'pc, 'gas> {
    pub fn new(
        protocol_config: &'pc ProtocolConfig,
        gas_charger: &'gas mut GasCharger,
    ) -> TranslationMeter<'pc, 'gas> {
        TranslationMeter {
            protocol_config,
            charger: gas_charger,
        }
    }

    pub fn charge_base_inputs<E: ExecutionErrorTrait>(
        &mut self,
        num_inputs: usize,
    ) -> Result<(), E> {
        let amount = (num_inputs as u64)
            .max(1)
            .saturating_mul(self.protocol_config.translation_per_input_base_charge());
        self.charge(amount)
    }

    pub fn charge_pure_input_bytes<E: ExecutionErrorTrait>(
        &mut self,
        num_bytes: usize,
    ) -> Result<(), E> {
        let amount = (num_bytes as u64).max(1).saturating_mul(
            self.protocol_config
                .translation_pure_input_per_byte_charge(),
        );
        self.charge(amount)
    }

    pub fn charge_base_command<E: ExecutionErrorTrait>(
        &mut self,
        num_args: usize,
    ) -> Result<(), E> {
        let amount = (num_args as u64)
            .max(1)
            .saturating_mul(self.protocol_config.translation_per_command_base_charge());
        self.charge(amount)
    }

    /// Charge gas for loading types based on the number of type nodes loaded.
    /// The cost is calculated as `num_type_nodes * TYPE_LOAD_PER_NODE_MULTIPLIER`.
    /// This function assumes that `num_type_nodes` is non-zero.
    pub fn charge_num_type_nodes<E: ExecutionErrorTrait>(
        &mut self,
        num_type_nodes: u64,
    ) -> Result<(), E> {
        let amount = num_type_nodes
            .max(1)
            .saturating_mul(self.protocol_config.translation_per_type_node_charge());
        self.charge(amount)
    }

    pub fn charge_num_type_references<E: ExecutionErrorTrait>(
        &mut self,
        num_type_references: u64,
    ) -> Result<(), E> {
        let amount = self.reference_cost_formula(num_type_references.max(1))?;
        let amount =
            amount.saturating_mul(self.protocol_config.translation_per_reference_node_charge());
        self.charge(amount)
    }

    pub fn charge_num_linkage_entries<E: ExecutionErrorTrait>(
        &mut self,
        num_linkage_entries: usize,
    ) -> Result<(), E> {
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
    fn reference_cost_formula<E: ExecutionErrorTrait>(&self, n: u64) -> Result<u64, E> {
        let Some(n_succ) = n.checked_add(1) else {
            invariant_violation!("u64 overflow when calculating type reference cost")
        };
        Ok(n.saturating_mul(n_succ) / 2)
    }

    // Charge gas using a point charge mechanism based on the cumulative number of units charged so
    // far.
    fn charge<E: ExecutionErrorTrait>(&mut self, amount: u64) -> Result<(), E> {
        debug_assert!(amount > 0);
        self.charger
            .move_gas_status_mut()
            .deduct_gas(amount.into())
            .map_err(Self::gas_error)
    }

    fn gas_error<T, E>(e: T) -> E
    where
        T: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
        E: ExecutionErrorTrait,
    {
        E::new_with_source(ExecutionErrorKind::InsufficientGas, e)
    }
}
