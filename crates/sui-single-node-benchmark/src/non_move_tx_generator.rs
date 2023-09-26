// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::TxGenerator;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::{ObjectRef, SuiAddress};
use sui_types::crypto::AccountKeyPair;
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::transaction::{VerifiedTransaction, DEFAULT_VALIDATOR_GAS_PRICE};

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
        keypair: &AccountKeyPair,
        gas_objects: &[ObjectRef],
    ) -> VerifiedExecutableTransaction {
        let transaction =
            TestTransactionBuilder::new(sender, gas_objects[0], DEFAULT_VALIDATOR_GAS_PRICE)
                .transfer_sui(None, sender)
                .build_and_sign(keypair);
        VerifiedExecutableTransaction::new_from_quorum_execution(
            VerifiedTransaction::new_unchecked(transaction),
            0,
        )
    }

    fn name(&self) -> &'static str {
        "Simple Transfer Transaction Generator"
    }
}
