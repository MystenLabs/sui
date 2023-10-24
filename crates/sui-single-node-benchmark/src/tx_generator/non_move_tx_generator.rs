// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::mock_account::Account;
use crate::tx_generator::TxGenerator;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::transaction::{Transaction, DEFAULT_VALIDATOR_GAS_PRICE};

pub struct NonMoveTxGenerator {}

impl NonMoveTxGenerator {
    pub fn new() -> Self {
        Self {}
    }
}

impl TxGenerator for NonMoveTxGenerator {
    fn generate_tx(&self, account: Account) -> Transaction {
        TestTransactionBuilder::new(
            account.sender,
            account.gas_objects[0],
            DEFAULT_VALIDATOR_GAS_PRICE,
        )
        .transfer_sui(None, account.sender)
        .build_and_sign(account.keypair.as_ref())
    }

    fn name(&self) -> &'static str {
        "Simple Transfer Transaction Generator"
    }
}
