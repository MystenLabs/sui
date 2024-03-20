// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::mock_account::Account;
use crate::tx_generator::TxGenerator;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{Argument, Transaction, DEFAULT_VALIDATOR_GAS_PRICE};

pub struct NonMoveTxGenerator {
    num_input_objects: u8,
}

impl NonMoveTxGenerator {
    pub fn new(num_input_objects: u8) -> Self {
        Self { num_input_objects }
    }
}

impl TxGenerator for NonMoveTxGenerator {
    fn generate_tx(&self, account: Account) -> Transaction {
        let mut pt = ProgrammableTransactionBuilder::new();
        pt.transfer_arg(account.sender, Argument::GasCoin);
        for i in 1..self.num_input_objects {
            pt.transfer_object(account.sender, account.gas_objects[i as usize])
                .unwrap();
        }
        TestTransactionBuilder::new(
            account.sender,
            account.gas_objects[0],
            DEFAULT_VALIDATOR_GAS_PRICE,
        )
        .programmable(pt.finish())
        .build_and_sign(account.keypair.as_ref())
    }

    fn name(&self) -> &'static str {
        "Simple Transfer Transaction Generator"
    }
}
