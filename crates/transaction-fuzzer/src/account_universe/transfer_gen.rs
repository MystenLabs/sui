// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Copyright (c) The Diem Core Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::account_universe::AccountCurrent;
use crate::{
    account_universe::{AUTransactionGen, AccountPairGen, AccountTriple, AccountUniverse},
    executor::{ExecutionResult, Executor},
};
use once_cell::sync::Lazy;
use proptest::prelude::*;
use proptest_derive::Arbitrary;
use std::sync::Arc;
use sui_protocol_config::ProtocolConfig;
use sui_types::base_types::ObjectRef;
use sui_types::execution_status::{ExecutionFailureStatus, ExecutionStatus};
use sui_types::{
    base_types::SuiAddress,
    error::{SuiError, UserInputError},
    messages::{GasData, TransactionData, TransactionKind, VerifiedTransaction},
    object::Object,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    utils::{to_sender_signed_transaction, to_sender_signed_transaction_with_multi_signers},
};

const GAS_UNIT_PRICE: u64 = 2;
const DEFAULT_TRANSFER_AMOUNT: u64 = 1;
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

#[derive(Arbitrary, Clone, Debug)]
#[proptest(params = "(u64, u64)")]
pub struct P2PTransferGenGasPriceInRange {
    sender_receiver: AccountPairGen,
    #[proptest(strategy = "params.0 ..= params.1")]
    gas_price: u64,
}

/// Represents a peer-to-peer transaction performed in the account universe with the gas budget,
/// gas price and number of gas coins randomly selected.
#[derive(Arbitrary, Clone, Debug)]
#[proptest(params = "(u64, u64)")]
pub struct P2PTransferGenRandGasRandPriceRandCoins {
    sender_receiver: AccountPairGen,
    #[proptest(strategy = "params.0 ..= params.1")]
    amount: u64,
    #[proptest(strategy = "gas_budget_selection_strategy()")]
    gas: u64,
    #[proptest(strategy = "gas_price_selection_strategy()")]
    gas_price: u64,
    #[proptest(strategy = "gas_coins_selection_strategy()")]
    gas_coins: u32,
}
/// Represents a peer-to-peer transaction performed in the account universe with the gas budget
/// and gas price randomly selected and sponsorship state also randomly selected.
#[derive(Arbitrary, Clone, Debug)]
#[proptest(params = "(u64, u64)")]
pub struct P2PTransferGenRandomGasRandomPriceRandomSponsorship {
    sender_receiver: AccountPairGen,
    #[proptest(strategy = "params.0 ..= params.1")]
    amount: u64,
    #[proptest(strategy = "gas_budget_selection_strategy()")]
    gas: u64,
    #[proptest(strategy = "gas_price_selection_strategy()")]
    gas_price: u64,
    #[proptest(strategy = "gas_coins_selection_strategy()")]
    gas_coins: u32,
    sponsorship: TransactionSponsorship,
}

#[derive(Arbitrary, Clone, Debug)]
pub enum TransactionSponsorship {
    // No sponsorship for the transaction.
    None,
    // Valid sponsorship for the transaction.
    Good,
    WrongGasOwner,
}

impl TransactionSponsorship {
    pub fn select_gas(
        &self,
        accounts: &mut AccountTriple,
        exec: &mut Executor,
        gas_coins: u32,
    ) -> (Vec<ObjectRef>, (u64, Object), SuiAddress) {
        match self {
            TransactionSponsorship::None => {
                let gas_object = accounts.account_1.new_gas_object(exec);
                let mut gas_amount = *accounts.account_1.current_balances.last().unwrap();
                let mut gas_coin_refs = vec![gas_object.compute_object_reference()];
                for _ in 1..gas_coins {
                    let gas_object = accounts.account_1.new_gas_object(exec);
                    gas_coin_refs.push(gas_object.compute_object_reference());
                    gas_amount += *accounts.account_1.current_balances.last().unwrap();
                }
                (
                    gas_coin_refs,
                    (gas_amount, gas_object),
                    accounts.account_1.initial_data.account.address,
                )
            }
            TransactionSponsorship::Good => {
                let gas_object = accounts.account_3.new_gas_object(exec);
                let mut gas_amount = *accounts.account_3.current_balances.last().unwrap();
                let mut gas_coin_refs = vec![gas_object.compute_object_reference()];
                for _ in 1..gas_coins {
                    let gas_object = accounts.account_3.new_gas_object(exec);
                    gas_coin_refs.push(gas_object.compute_object_reference());
                    gas_amount += *accounts.account_3.current_balances.last().unwrap();
                }
                (
                    gas_coin_refs,
                    (gas_amount, gas_object),
                    accounts.account_3.initial_data.account.address,
                )
            }
            TransactionSponsorship::WrongGasOwner => {
                let gas_object = accounts.account_1.new_gas_object(exec);
                let mut gas_amount = *accounts.account_1.current_balances.last().unwrap();
                let mut gas_coin_refs = vec![gas_object.compute_object_reference()];
                for _ in 1..gas_coins {
                    let gas_object = accounts.account_1.new_gas_object(exec);
                    gas_coin_refs.push(gas_object.compute_object_reference());
                    gas_amount += *accounts.account_1.current_balances.last().unwrap();
                }
                (
                    gas_coin_refs,
                    (gas_amount, gas_object),
                    accounts.account_3.initial_data.account.address,
                )
            }
        }
    }

    pub fn sign_transaction(
        &self,
        accounts: &AccountTriple,
        txn: TransactionData,
    ) -> VerifiedTransaction {
        match self {
            TransactionSponsorship::None => {
                to_sender_signed_transaction(txn, &accounts.account_1.initial_data.account.key)
            }
            TransactionSponsorship::Good | TransactionSponsorship::WrongGasOwner => {
                to_sender_signed_transaction_with_multi_signers(
                    txn,
                    vec![
                        &accounts.account_1.initial_data.account.key,
                        &accounts.account_3.initial_data.account.key,
                    ],
                )
            }
        }
    }

    pub fn sponsor<'a>(&self, account_triple: &'a mut AccountTriple) -> &'a mut AccountCurrent {
        match self {
            TransactionSponsorship::None => account_triple.account_1,
            TransactionSponsorship::Good | TransactionSponsorship::WrongGasOwner => {
                account_triple.account_3
            }
        }
    }
}

fn p2p_success_gas(gas_price: u64) -> u64 {
    gas_price * P2P_COMPUTE_GAS_USAGE + P2P_SUCCESS_STORAGE_USAGE
}

fn p2p_failure_gas(gas_price: u64) -> u64 {
    gas_price * P2P_COMPUTE_GAS_USAGE + P2P_FAILURE_STORAGE_USAGE
}

pub fn gas_price_selection_strategy() -> impl Strategy<Value = u64> {
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

pub fn gas_budget_selection_strategy() -> impl Strategy<Value = u64> {
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

fn gas_coins_selection_strategy() -> impl Strategy<Value = u32> {
    prop_oneof![
        2 => Just(1u32),
        6 => 2u32..PROTOCOL_CONFIG.max_gas_payment_objects(),
        1 => Just(PROTOCOL_CONFIG.max_gas_payment_objects()),
        1 => Just(PROTOCOL_CONFIG.max_gas_payment_objects() + 1),
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
        P2PTransferGenRandomGasRandomPriceRandomSponsorship {
            sender_receiver: self.sender_receiver.clone(),
            amount: self.amount,
            gas: self.gas,
            gas_price: GAS_UNIT_PRICE,
            gas_coins: 1,
            sponsorship: TransactionSponsorship::None,
        }
        .apply(universe, exec)
    }
}

impl AUTransactionGen for P2PTransferGenGasPriceInRange {
    fn apply(
        &self,
        universe: &mut AccountUniverse,
        exec: &mut Executor,
    ) -> (VerifiedTransaction, ExecutionResult) {
        P2PTransferGenRandomGasRandomPriceRandomSponsorship {
            sender_receiver: self.sender_receiver.clone(),
            amount: DEFAULT_TRANSFER_AMOUNT,
            gas: p2p_success_gas(self.gas_price),
            gas_price: self.gas_price,
            gas_coins: 1,
            sponsorship: TransactionSponsorship::None,
        }
        .apply(universe, exec)
    }
}

impl AUTransactionGen for P2PTransferGenRandomGasRandomPrice {
    fn apply(
        &self,
        universe: &mut AccountUniverse,
        exec: &mut Executor,
    ) -> (VerifiedTransaction, ExecutionResult) {
        P2PTransferGenRandomGasRandomPriceRandomSponsorship {
            sender_receiver: self.sender_receiver.clone(),
            amount: self.amount,
            gas: self.gas,
            gas_price: self.gas_price,
            gas_coins: 1,
            sponsorship: TransactionSponsorship::None,
        }
        .apply(universe, exec)
    }
}

impl AUTransactionGen for P2PTransferGenRandGasRandPriceRandCoins {
    fn apply(
        &self,
        universe: &mut AccountUniverse,
        exec: &mut Executor,
    ) -> (VerifiedTransaction, ExecutionResult) {
        P2PTransferGenRandomGasRandomPriceRandomSponsorship {
            sender_receiver: self.sender_receiver.clone(),
            amount: self.amount,
            gas: self.gas,
            gas_price: self.gas_price,
            gas_coins: self.gas_coins,
            sponsorship: TransactionSponsorship::None,
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
    too_many_gas_coins: bool,
    wrong_gas_owner: bool,
}

impl RunInfo {
    pub fn new(
        payer_balance: u64,
        rgp: u64,
        p2p: &P2PTransferGenRandomGasRandomPriceRandomSponsorship,
    ) -> Self {
        let to_deduct = p2p.amount as u128 + p2p.gas as u128;
        let enough_max_gas = payer_balance >= p2p.gas;
        let enough_computation_gas = p2p.gas >= p2p.gas_price * P2P_COMPUTE_GAS_USAGE;
        let enough_to_succeed = payer_balance as u128 >= to_deduct;
        let gas_budget_too_high = p2p.gas > PROTOCOL_CONFIG.max_tx_gas();
        let gas_budget_too_low = p2p.gas < PROTOCOL_CONFIG.base_tx_cost_fixed();
        let not_enough_gas = p2p.gas < p2p_success_gas(p2p.gas_price);
        let gas_price_too_low = p2p.gas_price < rgp;
        let gas_price_too_high = p2p.gas_price >= PROTOCOL_CONFIG.max_gas_price();
        let gas_price_greater_than_budget = p2p.gas_price > p2p.gas;
        let gas_units_too_low = p2p.gas_price > 0
            && p2p.gas / p2p.gas_price < INSUFFICIENT_GAS_UNITS_THRESHOLD
            || gas_price_greater_than_budget;
        let too_many_gas_coins = p2p.gas_coins >= PROTOCOL_CONFIG.max_gas_payment_objects();
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
            too_many_gas_coins,
            wrong_gas_owner: matches!(p2p.sponsorship, TransactionSponsorship::WrongGasOwner),
        }
    }
}

impl AUTransactionGen for P2PTransferGenRandomGasRandomPriceRandomSponsorship {
    fn apply(
        &self,
        universe: &mut AccountUniverse,
        exec: &mut Executor,
    ) -> (VerifiedTransaction, ExecutionResult) {
        let mut account_triple = self.sender_receiver.pick(universe);
        let (gas_coin_refs, (gas_balance, gas_object), gas_payer) =
            self.sponsorship
                .select_gas(&mut account_triple, exec, self.gas_coins);

        let AccountTriple {
            account_1: sender,
            account_2: recipient,
            ..
        } = &account_triple;
        // construct a p2p transfer of a random amount of SUI
        let txn = {
            let mut builder = ProgrammableTransactionBuilder::new();
            builder.transfer_sui(recipient.initial_data.account.address, Some(self.amount));
            builder.finish()
        };
        let sender_address = sender.initial_data.account.address;
        let kind = TransactionKind::ProgrammableTransaction(txn);
        let tx_data = TransactionData::new_with_gas_data(
            kind,
            sender_address,
            GasData {
                payment: gas_coin_refs,
                owner: gas_payer,
                price: self.gas_price,
                budget: self.gas,
            },
        );
        let signed_txn = self.sponsorship.sign_transaction(&account_triple, tx_data);
        let payer = self.sponsorship.sponsor(&mut account_triple);
        // *sender.current_balances.last().unwrap();
        let run_info = RunInfo::new(gas_balance, exec.get_reference_gas_price(), self);
        let status = match run_info {
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
                too_many_gas_coins: false,
                wrong_gas_owner: false,
            } => {
                self.fix_balance_and_gas_coins(payer, true);
                Ok(ExecutionStatus::Success)
            }
            RunInfo {
                too_many_gas_coins: true,
                ..
            } => Err(SuiError::UserInputError {
                error: UserInputError::SizeLimitExceeded {
                    limit: "maximum number of gas payment objects".to_string(),
                    value: "256".to_string(),
                },
            }),
            RunInfo {
                gas_price_too_low: true,
                ..
            } => Err(SuiError::UserInputError {
                error: UserInputError::GasPriceUnderRGP {
                    gas_price: self.gas_price,
                    reference_gas_price: exec.get_reference_gas_price(),
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
                    gas_balance: gas_balance as u128,
                    needed_gas_amount: self.gas as u128,
                },
            }),
            RunInfo {
                wrong_gas_owner: true,
                ..
            } => Err(SuiError::UserInputError {
                error: UserInputError::IncorrectUserSignature {
                    error: format!(
                               "Object {} is owned by account address {}, but given owner/signer address is {}",
                               gas_object.id(),
                                   sender_address,
                                   payer.initial_data.account.address,
                           )
                }
            }),
            RunInfo {
                enough_max_gas: true,
                enough_to_succeed: false,
                gas_units_too_low: false,
                ..
            } => {
                self.fix_balance_and_gas_coins(payer, false);
                Ok(ExecutionStatus::Failure {
                    error: ExecutionFailureStatus::InsufficientCoinBalance,
                    command: Some(0),
                })
            }
            RunInfo {
                enough_max_gas: true,
                ..
            } => {
                self.fix_balance_and_gas_coins(payer, false);
                Ok(ExecutionStatus::Failure {
                    error: ExecutionFailureStatus::InsufficientGas,
                    command: None,
                })
            }
        };
        (signed_txn, status)
    }
}

impl P2PTransferGenRandomGasRandomPriceRandomSponsorship {
    fn fix_balance_and_gas_coins(&self, sender: &mut AccountCurrent, success: bool) {
        // collect all the coins smashed and update the balance of the one true gas coin.
        // Gas objects are all coming from genesis which implies there is no rebate.
        // In making things simple that does not really exercise an important aspect
        // of the gas logic
        let mut smash_balance = 0;
        for _ in 1..self.gas_coins {
            sender.current_coins.pop().expect("coin must exist");
            smash_balance += sender.current_balances.pop().expect("balance must exist");
        }
        *sender.current_balances.last_mut().unwrap() += smash_balance;
        // Fine to cast to u64 at this point, since otherwise enough_max_gas would be false
        // since sender_balance is a u64.
        if success {
            *sender.current_balances.last_mut().unwrap() -=
                self.amount + p2p_success_gas(self.gas_price);
        } else {
            *sender.current_balances.last_mut().unwrap() -=
                std::cmp::min(self.gas, p2p_failure_gas(self.gas_price));
        }
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
