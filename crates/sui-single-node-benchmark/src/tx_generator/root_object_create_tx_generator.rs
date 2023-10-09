// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::tx_generator::TxGenerator;
use std::sync::Arc;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::crypto::AccountKeyPair;
use sui_types::transaction::{CallArg, Transaction, DEFAULT_VALIDATOR_GAS_PRICE};

pub struct RootObjectCreateTxGenerator {
    move_package: ObjectID,
    child_per_root: u64,
}

impl RootObjectCreateTxGenerator {
    pub fn new(move_package: ObjectID, child_per_root: u64) -> Self {
        Self {
            move_package,
            child_per_root,
        }
    }
}

impl TxGenerator for RootObjectCreateTxGenerator {
    fn generate_tx(
        &self,
        sender: SuiAddress,
        keypair: Arc<AccountKeyPair>,
        gas_objects: Arc<Vec<ObjectRef>>,
    ) -> Transaction {
        TestTransactionBuilder::new(sender, gas_objects[0], DEFAULT_VALIDATOR_GAS_PRICE)
            .move_call(
                self.move_package,
                "benchmark",
                "generate_dynamic_fields",
                vec![CallArg::Pure(bcs::to_bytes(&self.child_per_root).unwrap())],
            )
            .build_and_sign(keypair.as_ref())
    }

    fn name(&self) -> &'static str {
        "Root Object Creation Transaction Generator"
    }
}
