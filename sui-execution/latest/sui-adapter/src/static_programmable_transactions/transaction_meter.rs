// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::cell::RefCell;

use crate::gas_charger::GasCharger;
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    error::{ExecutionError, ExecutionErrorKind},
    move_package::MovePackage,
};

// TODO: Determine the appropriate constants for these values.

/// The multiplier for the number of type nodes when charging for type loading.
/// This is multiplied by the number of type nodes to get the total cost.
/// This should be a small number to avoid excessive gas costs for loading types.
const TYPE_LOAD_PER_NODE_MULTIPLIER: u64 = 1;

/// The cost to load a package per byte based on the `package.size()`. This is a fixed cost to
/// account for the overhead of loading a package.
const PACKAGE_LOAD_COST_PER_BYTE: u64 = 1;

/// The base charge for each command in a programmable transaction. This is a fixed cost to
/// account for the overhead of processing each command.
const PER_COMMAND_BASE_CHARGE: u64 = 1;

/// Static assertions to ensure that the constants are valid.
const _: () = {
    assert!(TYPE_LOAD_PER_NODE_MULTIPLIER > 0);
    assert!(PACKAGE_LOAD_COST_PER_BYTE > 0);
    assert!(PER_COMMAND_BASE_CHARGE > 0);
};

#[macro_export]
macro_rules! gas_charger_mut {
    ($meter:expr) => {
        $meter
            .charger
            .try_borrow_mut()
            .map_err(|e| make_invariant_violation!("Failed to borrow gas charger mutably: {}", e))?
    };
}

#[macro_export]
macro_rules! gas_charger_imm {
    ($meter:expr) => {
        $meter.charger.try_borrow().map_err(|e| {
            make_invariant_violation!("Failed to borrow gas charger immutably: {}", e)
        })?
    };
}

macro_rules! gated {
    ($self:ident) => {
        if !$self.protocol_config.ptb_gas_charging() {
            return Ok(());
        }
    };
}

/// The [`TransactionMeter`] is responsible for metering gas usage for various operations
/// during the execution of a transaction. It interacts with and exposes interfaces to the
/// [`GasCharger`] it holds in order to deduct gas based on the operations performed.
///
/// It provides methods to charge gas for loading types and packages and other charging that occurs
/// during the translation of a programmable transaction, as well as during the execution of a
/// transaction.
///
/// It holds a reference to the `ProtocolConfig` to access protocol-specific configuration
/// parameters that may influence gas costs and limits.
pub struct TransactionMeter<'gas, 'pc> {
    #[allow(dead_code)]
    protocol_config: &'pc ProtocolConfig,
    pub charger: RefCell<&'gas mut GasCharger>,
}

impl<'gas, 'pc> TransactionMeter<'gas, 'pc> {
    pub fn new(
        gas_charger: &'gas mut GasCharger,
        protocol_config: &'pc ProtocolConfig,
    ) -> TransactionMeter<'gas, 'pc> {
        TransactionMeter {
            protocol_config,
            charger: RefCell::new(gas_charger),
        }
    }

    pub fn charge_command_base(&self) -> Result<(), ExecutionError> {
        gated!(self);
        gas_charger_mut!(self)
            .move_gas_status_mut()
            .deduct_gas(PER_COMMAND_BASE_CHARGE.into())
            .map_err(Self::gas_error)
    }

    /// Charge gas for loading types based on the number of type nodes loaded.
    /// The cost is calculated as `num_type_nodes * TYPE_LOAD_PER_NODE_MULTIPLIER`.
    /// This function assumes that `num_type_nodes` is non-zero.
    pub fn charge_num_type_nodes(&self, num_type_nodes: u64) -> Result<(), ExecutionError> {
        gated!(self);
        debug_assert!(num_type_nodes > 0);
        let amount = num_type_nodes.saturating_mul(TYPE_LOAD_PER_NODE_MULTIPLIER);
        // amount should always be non-zero since num_type_nodes is non-zero and
        // TYPE_LOAD_PER_NODE_MULTIPLIER is non-zero.
        debug_assert!(amount > 0);
        gas_charger_mut!(self)
            .move_gas_status_mut()
            .deduct_gas(amount.into())
            .map_err(Self::gas_error)
    }

    pub fn charge_package_load(&self, package: &MovePackage) -> Result<(), ExecutionError> {
        gated!(self);
        let amount = (package.size() as u64).saturating_mul(PACKAGE_LOAD_COST_PER_BYTE);
        // amount should always be non-zero since package.size() is non-zero and
        // PACKAGE_LOAD_COST_PER_BYTE is non-zero.
        debug_assert!(amount > 0);
        gas_charger_mut!(self)
            .move_gas_status_mut()
            .deduct_gas(amount.into())
            .map_err(Self::gas_error)
    }

    fn gas_error<E>(e: E) -> ExecutionError
    where
        E: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
    {
        ExecutionError::new_with_source(ExecutionErrorKind::InsufficientGas, e)
    }
}
