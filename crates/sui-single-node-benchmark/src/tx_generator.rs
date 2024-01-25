// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::mock_account::Account;
pub use counter_create_tx_generator::CounterCreateTxGenerator;
pub use counter_tx_generator::CounterTxGenerator;
pub use move_tx_generator::MoveTxGenerator;
pub use non_move_tx_generator::NonMoveTxGenerator;
pub use root_object_create_tx_generator::RootObjectCreateTxGenerator;
use sui_types::transaction::Transaction;

pub mod counter_create_tx_generator;
pub mod counter_tx_generator;
pub mod move_tx_generator;
pub mod non_move_tx_generator;
pub mod root_object_create_tx_generator;

pub trait TxGenerator: Send + Sync {
    /// Given an account that contains a sender address, a keypair for that address,
    /// and a list of gas objects owned by this address, generate a single transaction.
    fn generate_txs(&self, account: Account) -> Vec<Transaction>;

    fn name(&self) -> &'static str;
}
