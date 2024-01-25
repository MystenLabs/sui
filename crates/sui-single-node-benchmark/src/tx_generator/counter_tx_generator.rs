// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use super::TxGenerator;
use crate::mock_account::Account;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::{
    base_types::{ObjectID, ObjectRef, SuiAddress},
    transaction::{CallArg, ObjectArg, Transaction, DEFAULT_VALIDATOR_GAS_PRICE},
};

pub struct CounterTxGenerator {
    move_package: ObjectID,
    counter_objects: HashMap<SuiAddress, ObjectRef>,
    txs_per_counter: u64,
}

impl CounterTxGenerator {
    pub fn new(
        move_package: ObjectID,
        counter_objects: HashMap<SuiAddress, ObjectRef>,
        txs_per_counter: u64,
    ) -> Self {
        Self {
            move_package,
            counter_objects,
            txs_per_counter,
        }
    }
}

impl TxGenerator for CounterTxGenerator {
    fn generate_txs(&self, account: Account) -> Vec<Transaction> {
        let counter = self.counter_objects.get(&account.sender).unwrap();
        let mut txs = Vec::with_capacity(self.txs_per_counter as usize);
        for i in 0..self.txs_per_counter {
            let tx = TestTransactionBuilder::new(
                account.sender,
                account.gas_objects[i as usize],
                DEFAULT_VALIDATOR_GAS_PRICE,
            )
            .move_call(
                self.move_package,
                "benchmark",
                "increment_counter",
                vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(*counter))],
            )
            .build_and_sign(account.keypair.as_ref());

            txs.push(tx);
        }
        txs
        // TestTransactionBuilder::new(
        //     account.sender,
        //     account.gas_objects[0],
        //     DEFAULT_VALIDATOR_GAS_PRICE,
        // )
        // .move_call(
        //     self.move_package,
        //     "benchmark",
        //     "increment_counter",
        //     vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(*counter))],
        // )
        // .build_and_sign(account.keypair.as_ref())
    }

    fn name(&self) -> &'static str {
        "Counter Increment Transaction Generator"
    }
}
