// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Copyright (c) The Diem Core Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    account_universe::{AUTransactionGen, AccountPair, AccountPairGen, AccountUniverse},
    executor::{ExecutionResult, Executor},
};
use proptest::prelude::*;
use proptest_derive::Arbitrary;
use std::sync::Arc;
use sui_types::{
    error::{SuiError, UserInputError},
    messages::{
        ExecutionFailureStatus, ExecutionStatus, TransactionData, TransactionKind,
        VerifiedTransaction,
    },
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    utils::to_sender_signed_transaction,
};

const GAS_UNIT_PRICE: u64 = 1;
const P2P_SUCCESS_GAS_USAGE: u64 = 1000 + 1976000;
const P2P_FAILURE_GAS_USAGE: u64 = 1000 + 988000;

/// Represents a peer-to-peer transaction performed in the account universe.
///
/// The parameters are the minimum and maximum balances to transfer.
#[derive(Arbitrary, Clone, Debug)]
#[proptest(params = "(u64, u64)")]
pub struct P2PTransferGen {
    sender_receiver: AccountPairGen,
    #[proptest(strategy = "params.0 ..= params.1")]
    amount: u64,
}

impl AUTransactionGen for P2PTransferGen {
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
            P2P_SUCCESS_GAS_USAGE,
            GAS_UNIT_PRICE,
        );
        let signed_txn = to_sender_signed_transaction(tx_data, &sender.initial_data.account.key);
        let gas_amount = P2P_SUCCESS_GAS_USAGE * GAS_UNIT_PRICE;
        let to_deduct = self.amount + gas_amount;
        let sender_balance = *sender.current_balances.last().unwrap();
        // Now determine the state transition
        let enough_max_gas = sender_balance >= gas_amount;
        let enough_to_transfer = sender_balance >= self.amount;
        let enough_to_succeed = sender_balance >= to_deduct;
        let status = match (enough_max_gas, enough_to_transfer, enough_to_succeed) {
            (true, true, true) => {
                *sender.current_balances.last_mut().unwrap() -= to_deduct;
                Ok(ExecutionStatus::Success)
            }
            (true, _, _) => {
                *sender.current_balances.last_mut().unwrap() -=
                    P2P_FAILURE_GAS_USAGE * GAS_UNIT_PRICE;
                Ok(ExecutionStatus::Failure {
                    error: ExecutionFailureStatus::InsufficientCoinBalance,
                    command: Some(0),
                })
            }
            (false, _, _) => Err(SuiError::UserInputError {
                error: UserInputError::GasBalanceTooLow {
                    gas_balance: sender_balance as u128,
                    needed_gas_amount: gas_amount as u128,
                },
            }),
        };
        (signed_txn, status)
    }
}

pub fn p2ptransfer_strategy(
    min: u64,
    max: u64,
) -> impl Strategy<Value = Arc<dyn AUTransactionGen + 'static>> {
    prop_oneof![
        3 => any_with::<P2PTransferGen>((min, max)).prop_map(P2PTransferGen::arced),
    ]
}
