// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::tx_generator::TxGenerator;
use std::sync::Arc;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::{ObjectRef, SuiAddress};
use sui_types::crypto::AccountKeyPair;
use sui_types::transaction::{Transaction, DEFAULT_VALIDATOR_GAS_PRICE};

pub struct NonMoveTxGenerator {}

impl NonMoveTxGenerator {
    pub fn new() -> Self {
        Self {}
    }
}

impl TxGenerator for NonMoveTxGenerator {
    fn generate_tx(
        &self,
        sender: SuiAddress,
        keypair: Arc<AccountKeyPair>,
        gas_objects: Arc<Vec<ObjectRef>>,
    ) -> Transaction {
        TestTransactionBuilder::new(sender, gas_objects[0], DEFAULT_VALIDATOR_GAS_PRICE)
            .transfer_sui(None, sender)
            .build_and_sign(keypair.as_ref())
    }

    fn name(&self) -> &'static str {
        "Simple Transfer Transaction Generator"
    }
}
