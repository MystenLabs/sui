// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Copyright (c) The Diem Core Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    account_universe::{AUTransactionGen, AccountPair, AccountPairGen, AccountUniverse},
    executor::{ExecutionResult, Executor},
};
use once_cell::sync::Lazy;
use proptest::prelude::*;
use proptest_derive::Arbitrary;
use std::sync::Arc;
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    error::{SuiError, UserInputError},
    messages::{
        ExecutionFailureStatus, ExecutionStatus, TransactionData, TransactionKind,
        VerifiedTransaction,
    },
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    utils::to_sender_signed_transaction,
};

const GAS_UNIT_PRICE: u64 = 2;
const P2P_COMPUTE_GAS_USAGE: u64 = 1000;
const P2P_SUCCESS_STORAGE_USAGE: u64 = 1976000;
const P2P_FAILURE_STORAGE_USAGE: u64 = 988000;
const INSUFFICIENT_GAS_UNITS_THRESHOLD: u64 = 2;

static PROTOCOL_CONFIG: Lazy<ProtocolConfig> = Lazy::new(ProtocolConfig::get_for_max_version);

/// Represents a peer-to-peer transaction performed in the account universe.
///
/// The parameters are the minimum and maximum balances to transfer.
#[derive(Arbitrary, Clone, Debug)]
#[proptest(params = "(u64, u64)")]
pub struct P2PTransferGenGoodGas {
    sender_receiver: AccountPairGen,
    #[proptest(strategy = "params.0 ..= params.1")]
    amount: u64,
}

/// Represents a peer-to-peer transaction performed in the account universe with the gas budget
/// randomly selected.
#[derive(Arbitrary, Clone, Debug)]
#[proptest(params = "(u64, u64)")]
pub struct P2PTransferGenRandomGas {
    sender_receiver: AccountPairGen,
    #[proptest(strategy = "params.0 ..= params.1")]
    amount: u64,
    #[proptest(strategy = "gas_budget_selection_strategy()")]
    gas: u64,
}

/// Represents a peer-to-peer transaction performed in the account universe with the gas budget
/// and gas price randomly selected.
#[derive(Arbitrary, Clone, Debug)]
#[proptest(params = "(u64, u64)")]
pub struct P2PTransferGenRandomGasRandomPrice {
    sender_receiver: AccountPairGen,
    #[proptest(strategy = "params.0 ..= params.1")]
    amount: u64,
    #[proptest(strategy = "gas_budget_selection_strategy()")]
    gas: u64,
    #[proptest(strategy = "gas_price_selection_strategy()")]
    gas_price: u64,
}

fn p2p_success_gas(gas_price: u64) -> u64 {
    gas_price * P2P_COMPUTE_GAS_USAGE + P2P_SUCCESS_STORAGE_USAGE
}

fn p2p_failure_gas(gas_price: u64) -> u64 {
    gas_price * P2P_COMPUTE_GAS_USAGE + P2P_FAILURE_STORAGE_USAGE
}

fn gas_price_selection_strategy() -> impl Strategy<Value = u64> {
    prop_oneof![
        Just(0u64),
        1u64..10_000,
        Just(PROTOCOL_CONFIG.max_gas_price() - 1),
        Just(PROTOCOL_CONFIG.max_gas_price()),
        Just(PROTOCOL_CONFIG.max_gas_price() + 1),
        // Div and subtract so we don't need to worry about overflow in the test when computing our
        // success gas.
        Just(u64::MAX / P2P_COMPUTE_GAS_USAGE - 1 - P2P_SUCCESS_STORAGE_USAGE),
        Just(u64::MAX / P2P_COMPUTE_GAS_USAGE - P2P_SUCCESS_STORAGE_USAGE),
    ]
}

fn gas_budget_selection_strategy() -> impl Strategy<Value = u64> {
    prop_oneof![
        Just(0u64),
        Just(PROTOCOL_CONFIG.base_tx_cost_fixed() - 1),
        Just(PROTOCOL_CONFIG.base_tx_cost_fixed()),
        Just(PROTOCOL_CONFIG.base_tx_cost_fixed() + 1),
        1_000_000u64..=3_000_000,
        Just(PROTOCOL_CONFIG.max_tx_gas() - 1),
        Just(PROTOCOL_CONFIG.max_tx_gas()),
        Just(PROTOCOL_CONFIG.max_tx_gas() + 1),
        Just(u64::MAX - 1),
        Just(u64::MAX)
    ]
}

impl AUTransactionGen for P2PTransferGenGoodGas {
    fn apply(
        &self,
        universe: &mut AccountUniverse,
        exec: &mut Executor,
    ) -> (VerifiedTransaction, ExecutionResult) {
        P2PTransferGenRandomGas {
            sender_receiver: self.sender_receiver.clone(),
            amount: self.amount,
            gas: p2p_success_gas(GAS_UNIT_PRICE),
        }
        .apply(universe, exec)
    }
}

impl AUTransactionGen for P2PTransferGenRandomGas {
    fn apply(
        &self,
        universe: &mut AccountUniverse,
        exec: &mut Executor,
    ) -> (VerifiedTransaction, ExecutionResult) {
        P2PTransferGenRandomGasRandomPrice {
            sender_receiver: self.sender_receiver.clone(),
            amount: self.amount,
            gas: self.gas,
            gas_price: GAS_UNIT_PRICE,
        }
        .apply(universe, exec)
    }
}

// Encapsulates information needed to determine the result of a transaction execution.
#[derive(Debug)]
struct RunInfo {
    enough_max_gas: bool,
    enough_computation_gas: bool,
    enough_to_succeed: bool,
    not_enough_gas: bool,
    gas_budget_too_high: bool,
    gas_budget_too_low: bool,
    gas_price_too_high: bool,
    gas_price_too_low: bool,
    gas_units_too_low: bool,
}

impl RunInfo {
    pub fn new(sender_balance: u64, p2p: &P2PTransferGenRandomGasRandomPrice) -> Self {
        let to_deduct = p2p.amount as u128 + p2p.gas as u128;
        let enough_max_gas = sender_balance >= p2p.gas;
        let enough_computation_gas = p2p.gas >= p2p.gas_price * P2P_COMPUTE_GAS_USAGE;
        let enough_to_succeed = sender_balance as u128 >= to_deduct;
        let gas_budget_too_high = p2p.gas > PROTOCOL_CONFIG.max_tx_gas();
        let gas_budget_too_low = p2p.gas < PROTOCOL_CONFIG.base_tx_cost_fixed();
        let not_enough_gas = p2p.gas < p2p_success_gas(p2p.gas_price);
        let gas_price_too_low = p2p.gas_price < 1;
        let gas_price_too_high = p2p.gas_price >= PROTOCOL_CONFIG.max_gas_price();
        let gas_price_greater_than_budget = p2p.gas_price > p2p.gas;
        let gas_units_too_low = p2p.gas_price > 0
            && p2p.gas / p2p.gas_price < INSUFFICIENT_GAS_UNITS_THRESHOLD
            || gas_price_greater_than_budget;
        Self {
            enough_max_gas,
            enough_computation_gas,
            enough_to_succeed,
            not_enough_gas,
            gas_budget_too_high,
            gas_budget_too_low,
            gas_price_too_high,
            gas_price_too_low,
            gas_units_too_low,
        }
    }
}

impl AUTransactionGen for P2PTransferGenRandomGasRandomPrice {
    fn apply(
        &self,
        universe: &mut AccountUniverse,
        exec: &mut Executor,
    ) -> (VerifiedTransaction, ExecutionResult) {
        let AccountPair {
            account_1: sender,
            account_2: recipient,
            ..
        } = self.sender_receiver.pick(universe);

        let gas_object = sender.new_gas_object(exec);
        // construct a p2p transfer of a random amount of SUI
        let txn = {
            let mut builder = ProgrammableTransactionBuilder::new();
            builder.transfer_sui(recipient.initial_data.account.address, Some(self.amount));
            builder.finish()
        };
        let kind = TransactionKind::ProgrammableTransaction(txn);
        let tx_data = TransactionData::new(
            kind,
            sender.initial_data.account.address,
            gas_object.compute_object_reference(),
            self.gas,
            self.gas_price,
        );
        let signed_txn = to_sender_signed_transaction(tx_data, &sender.initial_data.account.key);
        let sender_balance = *sender.current_balances.last().unwrap();
        let status = match RunInfo::new(sender_balance, self) {
            RunInfo {
                enough_max_gas: true,
                enough_computation_gas: true,
                enough_to_succeed: true,
                not_enough_gas: false,
                gas_budget_too_high: false,
                gas_budget_too_low: false,
                gas_price_too_low: false,
                gas_price_too_high: false,
                gas_units_too_low: false,
            } => {
                // Fine to cast to u64 at this point, since otherwise enough_max_gas would be false
                // since sender_balance is a u64.
                *sender.current_balances.last_mut().unwrap() -=
                    self.amount + p2p_success_gas(self.gas_price);
                Ok(ExecutionStatus::Success)
            }
            RunInfo {
                gas_price_too_low: true,
                ..
            } => Err(SuiError::UserInputError {
                error: UserInputError::GasPriceUnderRGP {
                    gas_price: self.gas_price,
                    reference_gas_price: 1,
                },
            }),
            RunInfo {
                gas_price_too_high: true,
                ..
            } => Err(SuiError::UserInputError {
                error: UserInputError::GasPriceTooHigh {
                    max_gas_price: PROTOCOL_CONFIG.max_gas_price(),
                },
            }),
            RunInfo {
                gas_budget_too_low: true,
                ..
            } => Err(SuiError::UserInputError {
                error: UserInputError::GasBudgetTooLow {
                    gas_budget: self.gas,
                    min_budget: PROTOCOL_CONFIG.base_tx_cost_fixed(),
                },
            }),
            RunInfo {
                gas_budget_too_high: true,
                ..
            } => Err(SuiError::UserInputError {
                error: UserInputError::GasBudgetTooHigh {
                    gas_budget: self.gas,
                    max_budget: PROTOCOL_CONFIG.max_tx_gas(),
                },
            }),
            RunInfo {
                enough_max_gas: false,
                ..
            } => Err(SuiError::UserInputError {
                error: UserInputError::GasBalanceTooLow {
                    gas_balance: sender_balance as u128,
                    needed_gas_amount: self.gas as u128,
                },
            }),
            RunInfo {
                enough_max_gas: true,
                enough_to_succeed: false,
                gas_units_too_low: false,
                ..
            } => {
                *sender.current_balances.last_mut().unwrap() -=
                    std::cmp::min(self.gas, p2p_failure_gas(self.gas_price));
                Ok(ExecutionStatus::Failure {
                    error: ExecutionFailureStatus::InsufficientCoinBalance,
                    command: Some(0),
                })
            }
            RunInfo {
                enough_max_gas: true,
                ..
            } => {
                *sender.current_balances.last_mut().unwrap() -=
                    std::cmp::min(self.gas, p2p_failure_gas(self.gas_price));
                Ok(ExecutionStatus::Failure {
                    error: ExecutionFailureStatus::InsufficientGas,
                    command: None,
                })
            }
        };
        (signed_txn, status)
    }
}

pub fn p2p_transfer_strategy(
    min: u64,
    max: u64,
) -> impl Strategy<Value = Arc<dyn AUTransactionGen + 'static>> {
    prop_oneof![
        3 => any_with::<P2PTransferGenGoodGas>((min, max)).prop_map(P2PTransferGenGoodGas::arced),
        2 => any_with::<P2PTransferGenRandomGasRandomPrice>((min, max)).prop_map(P2PTransferGenRandomGasRandomPrice::arced),
        1 => any_with::<P2PTransferGenRandomGas>((min, max)).prop_map(P2PTransferGenRandomGas::arced),
    ]
}
