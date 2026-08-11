// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Transaction-building helpers shared across the integration tests.

use sui_types::base_types::ObjectRef;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::AccountKeyPair;
use sui_types::digests::TransactionDigest;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::Transaction;
use sui_types::transaction::TransactionData;

use crate::FullCluster;

/// 5 SUI — a default gas budget generous enough for the simple transactions these tests build.
pub const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;

/// Execute a transfer of `amount` MIST from `sender` to itself, paid for by and gas-threaded through
/// `gas`, signed by `kp`. Returns the new gas object reference (to thread into the next transaction)
/// and the transaction's digest.
pub fn send_sui(
    cluster: &mut FullCluster,
    sender: SuiAddress,
    kp: &AccountKeyPair,
    gas: ObjectRef,
    amount: u64,
) -> (ObjectRef, TransactionDigest) {
    let rgp = cluster.reference_gas_price();

    let mut builder = ProgrammableTransactionBuilder::new();
    builder.transfer_sui(sender, Some(amount));

    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        rgp,
    );

    let (fx, _) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![kp]))
        .expect("Failed to execute transaction");
    assert!(fx.status().is_ok(), "transaction failed: {:?}", fx.status());

    (fx.gas_object().unwrap().0, *fx.transaction_digest())
}
