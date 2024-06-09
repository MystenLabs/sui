// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::mock_account::Account;
pub use move_tx_generator::MoveTxGenerator;
pub use package_publish_tx_generator::PackagePublishTxGenerator;
pub use root_object_create_tx_generator::RootObjectCreateTxGenerator;
pub use shared_object_create_tx_generator::SharedObjectCreateTxGenerator;
pub use non_move_tx_generator::NonMoveTxGenerator;
use sui_types::transaction::Transaction;

mod move_tx_generator;
mod package_publish_tx_generator;
mod root_object_create_tx_generator;
mod shared_object_create_tx_generator;
pub mod non_move_tx_generator;

pub trait TxGenerator: Send + Sync {
    /// Given an account that contains a sender address, a keypair for that address,
    /// and a list of gas objects owned by this address, generate a single transaction.
    fn generate_tx(&self, account: Account) -> Transaction;

    fn name(&self) -> &'static str;
}
